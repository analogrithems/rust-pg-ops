use anyhow::{Context, Result};
use std::process::Command;

pub async fn dump_database(
    name: &str,
    output: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
) -> Result<()> {
    let mut cmd = Command::new("pg_dump");
    cmd.arg("--dbname").arg(name)
        .arg("--file").arg(output)
        .arg("--host").arg(host)
        .arg("--port").arg(port.to_string());

    if let Some(user) = username {
        cmd.arg("--username").arg(user);
    }

    let status = cmd
        .status()
        .context("Failed to execute pg_dump")?;

    if !status.success() {
        anyhow::bail!("pg_dump failed with status: {}", status);
    }

    Ok(())
}

pub async fn restore_database(
    name: &str,
    input: &str,
    host: &str,
    port: u16,
    username: Option<&str>,
) -> Result<()> {
    let mut cmd = Command::new("pg_restore");
    cmd.arg("--dbname").arg(name)
        .arg("--host").arg(host)
        .arg("--port").arg(port.to_string())
        .arg(input);

    if let Some(user) = username {
        cmd.arg("--username").arg(user);
    }

    let status = cmd
        .status()
        .context("Failed to execute pg_restore")?;

    if !status.success() {
        anyhow::bail!("pg_restore failed with status: {}", status);
    }

    Ok(())
}
