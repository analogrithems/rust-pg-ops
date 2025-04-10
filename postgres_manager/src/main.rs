mod backup;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;
use tokio_postgres::Config;
use tracing::{info, error};

#[derive(Parser)]
#[command(name = "pgman")]
#[command(about = "PostgreSQL database management tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "localhost")]
    address: String,

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
}

async fn connect_ssl(config: &Config, verify: bool, root_cert_path: Option<&str>) -> Result<tokio_postgres::Client> {
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

async fn connect_no_ssl(config: &Config) -> Result<tokio_postgres::Client> {
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
    let mut config = Config::new();
    config.host(&cli.address);
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli: Cli = Cli::parse();
    let client = connect(&cli).await?;

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
                &cli.address,
                cli.port,
                cli.username.as_deref(),
            ).await?
        }
        Commands::Restore { name, input } => {
            info!("Restoring database '{}' from '{}'", name, input);
            backup::restore_database(
                name,
                input,
                &cli.address,
                cli.port,
                cli.username.as_deref(),
            ).await?
        }
        }

    Ok(())
}
