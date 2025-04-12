mod backup;
mod ui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, command, arg};

use tokio_postgres::Config as PgConfig;
use tokio_postgres::config::SslMode;
use log::{error, info, warn, LevelFilter};
use log4rs::{append::file::FileAppender, config::{Appender, Config as LogConfig, Root}, encode::pattern::PatternEncoder};
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;
use std::io;

#[derive(Parser)]
#[command(name = "postgres_manager")]
#[command(about = "PostgreSQL database management tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long)]
    file: Option<String>,

    #[arg(short = 'H', long)]
    host: Option<String>,

    #[arg(short, default_value = "5432", long)]
    port: Option<u16>,

    #[arg(short, long)]
    username: Option<String>,

    #[arg(short = 'P', long)]
    password: Option<String>,

    #[arg(short = 'D', default_value = "postgres", long)]
    db_name: Option<String>,

    #[arg(long, default_value = "true", help = "Enable SSL for the connection")]
    use_ssl: bool,

    #[arg(long, help = "Path to custom root certificates")]
    root_cert_path: Option<String>,

    #[arg(long, default_value = "false", help = "Verify SSL certificates")]
    verify_ssl: bool,

    #[arg(short = 'B', long, help = "S3 bucket name")]
    bucket: Option<String>,

    #[arg(short = 'R', long, help = "AWS region")]
    region: Option<String>,

    #[arg(short = 'x', long, default_value = "postgres", help = "Prefix for snapshot keys")]
    prefix: Option<String>,

    #[arg(short = 'E', long, help = "Custom endpoint URL for S3")]
    endpoint_url: Option<String>,

    #[arg(short = 'A', long, help = "AWS access key ID")]
    access_key_id: Option<String>,

    #[arg(short = 'S', long, help = "AWS secret access key")]
    secret_access_key: Option<String>,

    #[arg(long, default_value = "true", help = "Force path-style access to S3")]
    path_style: bool,
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

    /// Browse and restore S3 snapshots using TUI
    BrowseSnapshots,
}

async fn connect_ssl(config: &PgConfig, verify: bool, root_cert_path: Option<&str>) -> Result<tokio_postgres::Client> {
    let mut builder = TlsConnector::builder();
    if !verify {
        builder.danger_accept_invalid_certs(true);
    }
    if let Some(path) = root_cert_path {
        let cert_data = std::fs::read(path)?;
        let cert = native_tls::Certificate::from_pem(&cert_data)?;
        builder.add_root_certificate(cert);
    }
    let connector = builder.build()?;
    let connector = MakeTlsConnector::new(connector);

    let (client, connection) = config.connect(connector).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("connection error: {}", e);
        }
    });

    Ok(client)
}

async fn connect_no_ssl(config: &PgConfig) -> Result<tokio_postgres::Client> {
    let (client, connection) = config.connect(tokio_postgres::NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("connection error: {}", e);
        }
    });

    Ok(client)
}

async fn connect(cli: &Cli) -> Result<Option<tokio_postgres::Client>> {
    if !cli.host.is_some() && !cli.port.is_some() && !cli.username.is_some() && !cli.password.is_some() {
        // If no PostgreSQL settings are provided, return None
        return Ok(None);
    }

    let mut config = PgConfig::new();

    if cli.use_ssl {
        config.ssl_mode(SslMode::Require);
    }

    // Set default host and port if not provided
    config.host(&cli.host.clone().unwrap_or_else(|| "localhost".to_string()));
    config.port(cli.port.unwrap_or(5432));

    if let Some(ref user) = cli.username {
        config.user(user);
    }

    if let Some(ref password) = cli.password {
        config.password(password);
    }

    let result = if cli.use_ssl {
        connect_ssl(&config, cli.verify_ssl, cli.root_cert_path.as_deref()).await
    } else {
        connect_no_ssl(&config).await
    };

    match result {
        Ok(client) => Ok(Some(client)),
        Err(e) => {
            warn!("Failed to connect to PostgreSQL: {}", e);
            Ok(None)
        }
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
            if let Some(client) = client {
                list_databases(&client).await?;
            } else {
                error!("PostgreSQL connection required for this command");
                return Ok(());
            }
        }
        Commands::Create { name } => {
            if let Some(client) = client {
                create_database(&client, &name).await?;
            } else {
                error!("PostgreSQL connection required for this command");
                return Ok(());
            }
        }
        Commands::Drop { name } => {
            if let Some(client) = client {
                drop_database(&client, &name).await?;
            } else {
                error!("PostgreSQL connection required for this command");
                return Ok(());
            }
        }
        Commands::Clone { name } => {
            if let Some(client) = client {
                clone_database(&client, &name).await?;
            } else {
                error!("PostgreSQL connection required for this command");
                return Ok(());
            }
        }
        Commands::Dump { name, output } => {
            if let Some(_) = client {
                info!("Dumping database '{}' to '{}'", name, output);
                backup::dump_database(
                    &name,
                    &output,
                    &cli.host.clone().unwrap_or_else(|| "localhost".to_string()),
                    cli.port.unwrap_or(5432),
                    cli.username.as_deref(),
                    cli.password.as_deref(),
                    cli.use_ssl,
                )
                .await?
            } else {
                error!("PostgreSQL connection required for this command");
                return Ok(());
            }
        }
        Commands::Restore { name, input } => {
            if let Some(_) = client {
                backup::restore_database(
                    &name,
                    &input,
                    &cli.host.clone().unwrap_or_else(|| "localhost".to_string()),
                    cli.port.unwrap_or(5432),
                    cli.username.as_deref(),
                    cli.password.as_deref(),
                    cli.use_ssl,
                )
                .await?
            } else {
                error!("PostgreSQL connection required for this command");
                return Ok(());
            }
        }
        Commands::BrowseSnapshots => {
            // Use the new UI module to browse snapshots
            let res = ui::run_tui(
                cli.bucket,
                cli.region,
                cli.prefix,
                cli.endpoint_url,
                cli.access_key_id,
                cli.secret_access_key,
                true, // path_style
            ).await?;

            if let Some(snapshot_key) = res {
                // Handle the selected snapshot
                info!("Selected snapshot: {}", snapshot_key);
            }
        }
    }

    Ok(())
}
