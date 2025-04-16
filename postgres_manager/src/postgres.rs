use anyhow::{Context, Result};

use tokio_postgres::Config as PgConfig;

use log::{error, info};
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;

pub async fn connect_ssl(config: &PgConfig, verify: bool, root_cert_path: Option<&str>) -> Result<tokio_postgres::Client> {
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

pub async fn connect_no_ssl(config: &PgConfig) -> Result<tokio_postgres::Client> {
  let (client, connection) = config.connect(tokio_postgres::NoTls).await?;

  tokio::spawn(async move {
      if let Err(e) = connection.await {
          error!("connection error: {}", e);
      }
  });

  Ok(client)
}

pub async fn list_databases(client: &tokio_postgres::Client) -> Result<()> {
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

pub async fn create_database(client: &tokio_postgres::Client, name: &str) -> Result<()> {
  client
      .execute(&format!("CREATE DATABASE \"{}\";", name), &[])
      .await
      .context("Failed to create database")?;

  info!("Database '{}' created successfully", name);
  Ok(())
}

pub async fn clone_database(client: &tokio_postgres::Client, name: &str) -> Result<()> {
  let new_name = format!("{}-clone", name);
  client
      .execute(&format!("CREATE DATABASE \"{}\" WITH TEMPLATE \"{}\" OWNER \"{}\" ;", new_name, name, name), &[])
      .await
      .context("Failed to clone database")?;

  info!("Database '{}' cloned to '{}' successfully", name, new_name);
  Ok(())
}

pub async fn drop_database(client: &tokio_postgres::Client, name: &str) -> Result<()> {
  client
      .execute(&format!("DROP DATABASE \"{}\" WITH (FORCE);", name), &[])
      .await
      .context("Failed to drop database")?;

  info!("Database '{}' dropped successfully", name);
  Ok(())
}

pub async fn drop_database_with_force(client: &tokio_postgres::Client, name: &str) -> Result<()> {
  client
      .execute(&format!("DROP DATABASE \"{}\" WITH (FORCE);", name), &[])
      .await
      .context("Failed to drop database")?;

  info!("Database '{}' dropped successfully", name);
  Ok(())
}

pub async fn rename_database(client: &tokio_postgres::Client, old_name: &str, new_name: &str) -> Result<()> {
  client
      .execute(&format!("ALTER DATABASE \"{}\" RENAME TO \"{}\";", old_name, new_name), &[])
      .await
      .context("Failed to rename database")?;

  info!("Database '{}' renamed to '{}' successfully", old_name, new_name);
  Ok(())
}

pub async fn set_database_owner(client: &tokio_postgres::Client, name: &str, owner: &str) -> Result<()> {
  client
      .execute(&format!("ALTER DATABASE \"{}\" OWNER TO \"{}\";", name, owner), &[])
      .await
      .context("Failed to set database owner")?;

  info!("Database '{}' owner set to '{}' successfully", name, owner);
  Ok(())
}
