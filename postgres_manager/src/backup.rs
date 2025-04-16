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

pub fn restore_database(
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
        // Set PGSSLMODE to require
        log::info!("Setting PGSSLMODE to require");
        std::env::set_var("PGSSLMODE", "require");
    }else {
        // Set PGSSLMODE to disable
        log::info!("Setting PGSSLMODE to disable");
        std::env::set_var("PGSSLMODE", "disable");
    }

    debug!("Building pg_restore command");

    let mut cmd = Command::new("pg_restore");
    cmd.arg("--host").arg(host)
        .arg("--port").arg(port.to_string())
        .arg("-C").arg("-c").arg("--if-exists")
        .arg("--dbname").arg(name)
        .arg(input);

    if let Some(user) = username {
        cmd.arg("--username").arg(user);
    }

    if let Some(pass) = password {
        std::env::set_var("PGPASSWORD", pass);
    }

    // Create a debug-friendly representation of the command
    let cmd_str = format!("pg_restore --host {} --port {} -C -c --if-exists --dbname {} {} {}",
        host, port, name, username.map_or(String::new(), |u| format!(" --username {}", u)), input,
    );
    debug!("Executing pg_restore command: {} to database {}", cmd_str, name);
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
