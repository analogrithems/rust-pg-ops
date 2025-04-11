mod backup;
mod tui;

use anyhow::{anyhow, Context, Result};
use aws_sdk_s3::Client as S3Client;
use clap::{Parser, Subcommand};
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio_postgres::Config as PgConfig;
use log::{debug, error, info, LevelFilter};
use log4rs::{append::file::FileAppender, config::{Appender, Config as LogConfig, Root}, encode::pattern::PatternEncoder};

#[derive(Parser)]
#[command(name = "pgman")]
#[command(about = "PostgreSQL database management tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long)]
    file: Option<String>,

    #[arg(short = 'H', long, default_value = "localhost")]
    host: String,

    #[arg(short, long, default_value = "5432")]
    port: u16,

    #[arg(short, long)]
    username: Option<String>,

    #[arg(short = 'P', long)]
    password: Option<String>,

    #[arg(long, default_value = "true", help = "Enable SSL for the connection")]
    use_ssl: bool,

    #[arg(long, help = "Path to custom root certificates")]
    root_cert_path: Option<String>,

    #[arg(long, default_value = "false", help = "Verify SSL certificates")]
    verify_ssl: bool,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "List all databases")]
    List,

    #[command(about = "Create a new database")]
    Create {
        #[arg(help = "Name of the database to create")]
        name: String,
    },

    #[command(about = "Clone a database")]
    Clone {
        #[arg(help = "Name of the database to clone from. Will create a new database with the name '<same_name>-clone'")]
        name: String,
    },

    #[command(about = "Drop a database")]
    Drop {
        #[arg(help = "Name of the database to drop")]
        name: String,
    },

    #[command(about = "Dump a database")]
    Dump {
        #[arg(help = "Name of the database to dump")]
        name: String,

        #[arg(help = "Output file path")]
        output: String,
    },

    #[command(about = "Restore a database from dump")]
    Restore {
        #[arg(help = "Name of the database to restore to")]
        name: String,

        #[arg(help = "Input dump file path")]
        input: String,
    },

    #[command(about = "Browse and restore S3 snapshots using TUI")]
    BrowseSnapshots {
        #[arg(help = "S3 bucket containing the snapshots")]
        bucket: Option<String>,

        #[arg(help = "AWS region")]
        region: Option<String>,

        #[arg(long, help = "Custom endpoint URL (e.g. for MinIO)")]
        endpoint_url: Option<String>,

        #[arg(long, help = "AWS access key ID")]
        access_key_id: Option<String>,

        #[arg(long, help = "AWS secret access key")]
        secret_access_key: Option<String>,

        #[arg(long, help = "Use path-style addressing")]
        path_style: bool,
    },
}

async fn connect_ssl(config: &PgConfig, verify: bool, root_cert_path: Option<&str>) -> Result<tokio_postgres::Client> {
    let mut builder = TlsConnector::builder();

    if !verify {
        builder.danger_accept_invalid_certs(true);
    }

    if let Some(cert_path) = root_cert_path {
        let cert_data = std::fs::read(cert_path)
            .context("Failed to read root certificate file")?;
        let cert = native_tls::Certificate::from_pem(&cert_data)
            .context("Failed to parse root certificate")?;
        builder.add_root_certificate(cert);
    }

    let connector = MakeTlsConnector::new(builder.build()?);
    let (client, connection) = config
        .connect(connector)
        .await
        .context("Failed to connect to PostgreSQL with SSL")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("SSL connection error: {}", e);
        }
    });

    Ok(client)
}

async fn connect_no_ssl(config: &PgConfig) -> Result<tokio_postgres::Client> {
    let (client, connection) = config
        .connect(tokio_postgres::NoTls)
        .await
        .context("Failed to connect to PostgreSQL without SSL")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("Connection error: {}", e);
        }
    });

    Ok(client)
}

async fn connect(cli: &Cli) -> Result<tokio_postgres::Client> {
    let mut config = PgConfig::new();
    config.host(&cli.host);
    config.port(cli.port);

    if let Some(username) = &cli.username {
        config.user(username);
    }

    if let Some(password) = &cli.password {
        config.password(password);
    }

    if cli.use_ssl {
        connect_ssl(&config, cli.verify_ssl, cli.root_cert_path.as_deref()).await
    } else {
        connect_no_ssl(&config).await
    }
}

async fn list_databases(client: &tokio_postgres::Client) -> Result<()> {
    let rows = client
        .query("SELECT datname FROM pg_database WHERE datistemplate = false;", &[])
        .await?;

    println!("Available databases:");
    for row in rows {
        let name: String = row.get(0);
        println!("  - {}", name);
    }

    Ok(())
}

async fn create_database(client: &tokio_postgres::Client, name: &str) -> Result<()> {
    client
        .execute(&format!("CREATE DATABASE \"{}\";", name), &[])
        .await
        .context("Failed to create database")?;

    info!("Database '{}' created successfully", name);
    Ok(())
}

async fn clone_database(client: &tokio_postgres::Client, name: &str) -> Result<()> {
    let new_name = format!("{}-clone", name);
    client
        .execute(&format!("CREATE DATABASE \"{}\" WITH TEMPLATE \"{}\" OWNER \"{}\" ;", new_name, name, name), &[])
        .await
        .context("Failed to clone database")?;

    info!("Database '{}' cloned to '{}' successfully", name, new_name);
    Ok(())
}

async fn drop_database(client: &tokio_postgres::Client, name: &str) -> Result<()> {
    client
        .execute(&format!("DROP DATABASE \"{}\" WITH (FORCE);", name), &[])
        .await
        .context("Failed to drop database")?;

    info!("Database '{}' dropped successfully", name);
    Ok(())
}

fn parse_snapshot_key(snapshot_key: &str) -> Result<(String, String)> {
    debug!("Parsing snapshot key: {}", snapshot_key);
    let parts: Vec<&str> = snapshot_key.split('/').collect();
    if parts.len() < 2 {
        return Err(anyhow!("Invalid snapshot key format"));
    }

    let bucket = parts[0].to_string();
    let key = parts[1..].join("/");
    debug!("Parsed bucket: {}, key: {}", bucket, key);

    Ok((bucket, key))
}

async fn restore_from_s3(client: &tokio_postgres::Client, cli: &Cli, snapshot_key: &str) -> Result<()> {
    debug!("Starting S3 snapshot restoration from key: {}", snapshot_key);
    debug!("Parsing snapshot key: {}", snapshot_key);
    let (bucket, key) = parse_snapshot_key(snapshot_key)?;
    debug!("Parsed bucket: {}, key: {}", bucket, key);

    // Create a new database for restore
    let restore_db = format!("{}-restore", key.trim_end_matches(".dump"));
    debug!("Will restore to database: {}", restore_db);
    debug!("Creating target database: {}", restore_db);
    create_database(client, &restore_db).await?;

    // Download snapshot from S3
    debug!("Loading AWS config from environment");
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    debug!("Creating S3 client");
    let s3_client = S3Client::new(&config);

    debug!("Creating temporary file for snapshot");
    let temp_file = tempfile::NamedTempFile::new()?;
    let temp_path = temp_file.path().to_path_buf();
    debug!("Temporary file created at: {}", temp_path.display());

    debug!("Requesting object from S3: bucket={}, key={}", bucket, key);
    let object = s3_client
        .get_object()
        .bucket(bucket)
        .key(&key)
        .send()
        .await?;
    debug!("Successfully retrieved object from S3");

    debug!("Converting object body to async reader");
    let mut body = object.body.into_async_read();
    debug!("Opening temporary file for async writing");
    let file = File::create(&temp_path).await?;
    let mut writer = BufWriter::new(file);

    debug!("Copying S3 object to temporary file");
    let mut buffer = vec![0; 8192];
    loop {
        let n = body.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n]).await?;
    }
    writer.flush().await?;
    debug!("Successfully copied object to temporary file");

    // Restore the database
    backup::restore_database(
        &restore_db,
        temp_path.to_str().unwrap(),
        &cli.host,
        cli.port,
        cli.username.as_deref(),
        cli.password.as_deref(),
        cli.use_ssl,
    )
    .await?;

    println!("Successfully restored snapshot to database: {}", restore_db);

    info!("Successfully restored snapshot to database: {}", restore_db);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Configure logging
    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} {l} {t} - {m}{n}")))
        .build("postgres_manager.log")?;

    let log_config = LogConfig::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .build(Root::builder().appender("logfile").build(LevelFilter::Debug))?;

    log4rs::init_config(log_config)?;
    info!("Starting postgres_manager");


    let cli: Cli = Cli::parse();
    let client = connect(&cli).await?;

    // Add PGSSLMODE environment variable if SSL is enabled
    if cli.use_ssl {
        std::env::set_var("PGSSLMODE", "require");
    }

    match &cli.command {
        Commands::List => {
            list_databases(&client).await?;
        }
        Commands::Create { name } => {
            create_database(&client, name).await?;
        }
        Commands::Drop { name } => {
            drop_database(&client, name).await?;
        }
        Commands::Clone { name } => {
            clone_database(&client, name).await?;
        }
        Commands::Dump { name, output } => {
            info!("Dumping database '{}' to '{}'", name, output);
            backup::dump_database(
                name,
                output,
                &cli.host,
                cli.port,
                cli.username.as_deref(),
                cli.password.as_deref(),
                cli.use_ssl,
            ).await?
        }
        Commands::Restore { name, input } => {
            backup::restore_database(
                name,
                input,
                &cli.host,
                cli.port,
                cli.username.as_deref(),
                cli.password.as_deref(),
                cli.use_ssl,
            )
            .await?
        }
        Commands::BrowseSnapshots { bucket, region, endpoint_url, access_key_id, secret_access_key, path_style } => {
            if let Some(snapshot_key) = tui::run_tui(
                bucket.clone(),
                region.clone(),
                endpoint_url.clone(),
                access_key_id.clone(),
                secret_access_key.clone(),
                *path_style,
            ).await? {
                restore_from_s3(&client, &cli, &snapshot_key).await?
            }
        }
        }

    Ok(())
}
