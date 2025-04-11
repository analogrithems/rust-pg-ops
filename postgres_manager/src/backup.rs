use anyhow::{Context, Result};
use std::process::Command;
use log::{debug, error};

pub async fn dump_database(
    name: &str,
    output: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
    ssl: bool,
) -> Result<()> {

    // Add PGSSLMODE environment variable if SSL is enabled
    if ssl {
        std::env::set_var("PGSSLMODE", "require");
    }

    debug!("Building pg_dump command");
    let mut cmd = Command::new("pg_dump");
    cmd.arg("--dbname").arg(name)
        .arg("--file").arg(output)
        .arg("--host").arg(host)
        .arg("--port").arg(port.to_string());

    if let Some(user) = username {
        cmd.arg("--username").arg(user);
    }

    if let Some(pass) = password {
        cmd.arg("--password").arg(pass);
    }

    debug!("Executing pg_dump command");
    let output = cmd
        .output()
        .context("Failed to execute pg_dump")?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        error!("pg_dump failed: {}", error_msg);
        anyhow::bail!("pg_dump failed: {}", error_msg);
    }

    Ok(())
}

pub async fn restore_database(
    name: &str,
    input: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
    ssl: bool,
) -> Result<()> {
    // Add PGSSLMODE environment variable if SSL is enabled
    if ssl {
        std::env::set_var("PGSSLMODE", "require");
    }

    debug!("Building pg_restore command");
    let mut cmd = Command::new("pg_restore");
    cmd.arg("--dbname").arg(name)
        .arg("--host").arg(host)
        .arg("--file").arg(input)
        .arg("--port").arg(port.to_string());

    if let Some(user) = username {
        cmd.arg("--username").arg(user);
    }

    if let Some(pass) = password {
        cmd.arg("--password").arg(pass);
    }

    debug!("Executing pg_restore command");
    let output = cmd
        .output()
        .context("Failed to execute pg_restore")?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        error!("pg_restore failed: {}", error_msg);
        anyhow::bail!("pg_restore failed: {}", error_msg);
    }

    Ok(())
}
