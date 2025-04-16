use postgres_manager::{backup, ui, config};

use anyhow::Result;
use clap::{Parser, Subcommand, command, arg};
use postgres_manager::postgres;
use tokio_postgres::config::SslMode;
use tokio_postgres::Config as PgConfig;
use log::{error, info, warn, LevelFilter};
use log4rs::{append::file::FileAppender, config::{Appender, Config as LogConfig, Root}, encode::pattern::PatternEncoder};

#[derive(Parser)]
#[command(name = "postgres_manager")]
#[command(about = "PostgreSQL database management tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, help = "Postgres File Path")]
    file: Option<String>,

    #[arg(short = 'H', long, env = "PG_HOST", help = "Postgres Host")]
    host: Option<String>,

    #[arg(short, default_value = "5432", long, env = "PG_PORT", help = "Postgres Port")]
    port: Option<u16>,

    #[arg(short, long, env = "PG_USERNAME", help = "Postgres Username")]
    username: Option<String>,

    #[arg(short = 'P', long, env = "PG_PASSWORD", help = "Postgres Password")]
    password: Option<String>,

    #[arg(short = 'D', default_value = "postgres", long, env = "PG_DB_NAME", help = "Postgres Database Name")]
    db_name: Option<String>,

    #[arg(long, default_value = "false", env = "PG_USE_SSL", help = "Postgres Enable SSL")]
    use_ssl: bool,

    #[arg(long, env = "PG_ROOT_CERT_PATH", help = "Postgres Path to custom root certificates")]
    root_cert_path: Option<String>,

    #[arg(long, default_value = "false", env = "PG_VERIFY_SSL", help = "Postgres Verify SSL certificates")]
    verify_ssl: bool,

    #[arg(short = 'B', long, env = "S3_BUCKET", help = "S3 Bucket Name")]
    bucket: Option<String>,

    #[arg(short = 'R', long, env = "S3_REGION", help = "S3 Region")]
    region: Option<String>,

    #[arg(short = 'x', long, default_value = "postgres", env = "S3_PREFIX", help = "S3 Prefix for snapshot keys")]
    prefix: Option<String>,

    #[arg(short = 'E', long, env = "S3_ENDPOINT_URL", help = "S3 Endpoint URL")]
    endpoint_url: Option<String>,

    #[arg(short = 'A', long, env = "S3_ACCESS_KEY_ID", help = "S3 Access Key ID")]
    access_key_id: Option<String>,

    #[arg(short = 'S', long, env = "S3_SECRET_ACCESS_KEY", help = "S3 Secret Access Key")]
    secret_access_key: Option<String>,

    #[arg(long, default_value = "true", env = "S3_PATH_STYLE", help = "S3 Force path-style")]
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

    #[command(about = "Drop a database with force")]
    DropForce {
        #[arg(help = "Name of the database to drop")]
        name: String,
    },

    #[command(about = "Rename a database")]
    Rename {
        #[arg(help = "Name of the database to rename")]
        old_name: String,

        #[arg(help = "New name for the database")]
        new_name: String,
    },

    #[command(about = "Set database owner")]
    SetOwner {
        #[arg(help = "Name of the database")]
        name: String,

        #[arg(help = "New owner for the database")]
        owner: String,
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
        postgres::connect_ssl(&config, cli.verify_ssl, cli.root_cert_path.as_deref()).await
    } else {
        postgres::connect_no_ssl(&config).await
    };

    match result {
        Ok(client) => Ok(Some(client)),
        Err(e) => {
            warn!("Failed to connect to PostgreSQL: {}", e);
            Ok(None)
        }
    }
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

    // Load environment variables from .env file
    config::load_env();
    info!("Loaded environment variables");

    let cli: Cli = Cli::parse();
    let client = connect(&cli).await?;

    // Add PGSSLMODE environment variable if SSL is enabled
    if cli.use_ssl {
        std::env::set_var("PGSSLMODE", "require");
    }

    match &cli.command {
        Commands::List => {
            if let Some(client) = client {
                postgres::list_databases(&client).await?;
            } else {
                error!("PostgreSQL connection required for postgres::list_databases");
                return Ok(());
            }
        }
        Commands::Create { name } => {
            if let Some(client) = client {
                postgres::create_database(&client, &name).await?;
            } else {
                error!("PostgreSQL connection required for postgres::create_database");
                return Ok(());
            }
        }
        Commands::Drop { name } => {
            if let Some(client) = client {
                postgres::drop_database(&client, &name).await?;
            } else {
                error!("PostgreSQL connection required for postgres::drop_database");
                return Ok(());
            }
        }
        Commands::Clone { name } => {
            if let Some(client) = client {
                postgres::clone_database(&client, &name).await?;
            } else {
                error!("PostgreSQL connection required for postgres::clone_database");
                return Ok(());
            }
        }
        Commands::DropForce { name } => {
            if let Some(client) = client {
                postgres::drop_database_with_force(&client, &name).await?;
            } else {
                error!("PostgreSQL connection required for postgres::drop_database_with_force");
                return Ok(());
            }
        }
        Commands::Rename { old_name, new_name } => {
            if let Some(client) = client {
                postgres::rename_database(&client, &old_name, &new_name).await?;
            } else {
                error!("PostgreSQL connection required for postgres::rename_database");
                return Ok(());
            }
        }
        Commands::SetOwner { name, owner } => {
            if let Some(client) = client {
                postgres::set_database_owner(&client, &name, &owner).await?;
            } else {
                error!("PostgreSQL connection required for postgres::set_database_owner");
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
                error!("PostgreSQL connection required for postgres::dump_database");
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
                )?
            } else {
                error!("PostgreSQL connection required for postgres::restore_database");
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
                // The snapshot has been downloaded and restored through the UI
                info!("Snapshot processed: {}", snapshot_key);
                // The restore operation is handled within the UI flow
            }
        }
    }

    Ok(())
}
