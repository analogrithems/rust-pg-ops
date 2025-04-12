use crate::ui::models::{S3Config, PostgresConfig};
use std::env;

/// Load environment variables from .env file or from the file specified in DOTENV_PATH
pub fn load_env() {
    // Check if DOTENV_PATH is set and use that file instead of the default .env
    if let Ok(dotenv_path) = env::var("DOTENV_PATH") {
        dotenvy::from_path(dotenv_path).ok();
    } else {
        dotenvy::dotenv().ok();
    }
}

/// Get environment variable with a default value
fn get_env_with_default(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Get boolean environment variable
fn get_env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(val) => val.to_lowercase() == "true",
        Err(_) => default,
    }
}

/// Load S3 configuration from environment variables
pub fn load_s3_config() -> S3Config {
    S3Config {
        bucket: get_env_with_default("S3_BUCKET", ""),
        region: get_env_with_default("S3_REGION", "us-west-2"),
        prefix: get_env_with_default("S3_PREFIX", "backups/"),
        endpoint_url: get_env_with_default("S3_ENDPOINT_URL", ""),
        access_key_id: get_env_with_default("S3_ACCESS_KEY_ID", ""),
        secret_access_key: get_env_with_default("S3_SECRET_ACCESS_KEY", ""),
        path_style: get_env_bool("S3_PATH_STYLE", true),
        error_message: None,
    }
}

/// Load PostgreSQL configuration from environment variables
pub fn load_postgres_config() -> PostgresConfig {
    PostgresConfig {
        host: Some(get_env_with_default("PG_HOST", "localhost")),
        port: Some(get_env_with_default("PG_PORT", "5432").parse().unwrap_or(5432)),
        username: Some(get_env_with_default("PG_USERNAME", "postgres")),
        password: Some(get_env_with_default("PG_PASSWORD", "")),
        use_ssl: get_env_bool("PG_USE_SSL", false),
        db_name: Some(get_env_with_default("PG_DB_NAME", "postgres")),
    }
}
