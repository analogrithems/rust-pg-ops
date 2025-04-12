use aws_sdk_s3::primitives::DateTime as AwsDateTime;
use std::fmt;

/// Configuration for S3 connection
#[derive(Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub prefix: String,
    pub endpoint_url: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub path_style: bool,
    pub error_message: Option<String>,
}

impl S3Config {
    pub fn mask_secret(&self, secret: &str) -> String {
        if secret.len() <= 4 {
            return "*".repeat(secret.len());
        }
        let visible_chars = 4;
        let hidden_chars = secret.len() - visible_chars;
        format!("{}{}", "*".repeat(hidden_chars), &secret[hidden_chars..])
    }

    pub fn masked_access_key(&self) -> String {
        self.mask_secret(&self.access_key_id)
    }

    pub fn masked_secret_key(&self) -> String {
        self.mask_secret(&self.secret_access_key)
    }
}

/// Configuration for PostgreSQL connection
#[derive(Default)]
pub struct PostgresConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_ssl: bool,
    pub db_name: Option<String>,
}

/// Input mode for the UI
#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

/// Metadata for a backup
#[derive(Clone, Debug, PartialEq)]
pub struct BackupMetadata {
    pub key: String,
    pub size: i64,
    pub last_modified: AwsDateTime,
}

/// State of the popup
#[derive(Debug, PartialEq)]
pub enum PopupState {
    Hidden,
    ConfirmRestore,
    Downloading(BackupMetadata, f32, f64),
    ConfirmCancel(BackupMetadata, f32, f64),
    TestS3Result(String),
    TestPgResult(String),
    Error(String),
    Success(String),
}

/// Focus field for the UI
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FocusField {
    // S3 Settings (10-19)
    Bucket,          // Alt+1
    Region,          // Alt+2
    Prefix,          // Alt+3
    EndpointUrl,     // Alt+4
    AccessKeyId,     // Alt+5
    SecretAccessKey, // Alt+6
    PathStyle,       // Alt+7

    // PostgreSQL Settings (20-29)
    PgHost,          // Alt+q
    PgPort,          // Alt+w
    PgUsername,      // Alt+e
    PgPassword,      // Alt+r
    PgSsl,          // Alt+t
    PgDbName,        // Alt+y
    SnapshotList,
}

impl fmt::Display for FocusField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FocusField::Bucket => write!(f, "Bucket"),
            FocusField::Region => write!(f, "Region"),
            FocusField::Prefix => write!(f, "Prefix"),
            FocusField::EndpointUrl => write!(f, "Endpoint URL"),
            FocusField::AccessKeyId => write!(f, "Access Key ID"),
            FocusField::SecretAccessKey => write!(f, "Secret Access Key"),
            FocusField::PathStyle => write!(f, "Path Style"),
            FocusField::PgHost => write!(f, "PostgreSQL Host"),
            FocusField::PgPort => write!(f, "PostgreSQL Port"),
            FocusField::PgUsername => write!(f, "PostgreSQL Username"),
            FocusField::PgPassword => write!(f, "PostgreSQL Password"),
            FocusField::PgSsl => write!(f, "PostgreSQL SSL"),
            FocusField::PgDbName => write!(f, "PostgreSQL Database"),
            FocusField::SnapshotList => write!(f, "Snapshot List"),
        }
    }
}
