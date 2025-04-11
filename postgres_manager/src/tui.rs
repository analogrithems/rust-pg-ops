use anyhow::{Result, anyhow};
use aws_sdk_s3::{Client as S3Client, config::Credentials};
use log::{debug, error, warn};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;
use crate::{connect_ssl, connect_no_ssl};

#[derive(Debug, Default)]
pub struct S3Config {
    pub error_message: Option<String>,
    pub bucket: String,
    pub region: String,
    pub prefix: String,
    pub endpoint_url: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub path_style: bool,
}

#[derive(Default)]
pub struct PostgresConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_ssl: bool,
    pub db_name: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

#[derive(PartialEq)]
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

use aws_sdk_s3::primitives::DateTime;
use chrono::{DateTime as ChronoDateTime, NaiveDateTime, Utc};
use log::info;
use humansize::{format_size, BINARY};

#[derive(Debug, Clone)]
pub struct BackupMetadata {
    pub key: String,
    pub size: i64,
    pub last_modified: DateTime,
}

#[derive(Debug, PartialEq)]
pub enum PopupState {
    Hidden,
    ConfirmRestore,
    TestS3Result(String),
    TestPgResult(String),
}

pub struct SnapshotBrowser {
    pub config: S3Config,
    pub pg_config: PostgresConfig,
    pub popup_state: PopupState,
    pub snapshots: Vec<BackupMetadata>,
    pub state: ListState,
    pub s3_client: Option<S3Client>,
    pub focus: FocusField,
    pub input_mode: InputMode,
    pub input_buffer: String,
}

impl SnapshotBrowser {
    pub async fn test_s3_connection(&mut self) -> Result<()> {
        if self.s3_client.is_none() {
            if let Err(e) = self.init_s3_client().await {
                self.popup_state = PopupState::TestS3Result(format!("S3 connection failed: {}", e));
                return Ok(());
            }
        }

        let test_result = match self.s3_client.as_ref().unwrap()
            .list_objects_v2()
            .bucket(&self.config.bucket)
            .prefix("test-connection")
            .send()
            .await {
            Ok(_) => "S3 connection successful!".to_string(),
            Err(e) => format!("S3 connection failed: {}", e),
        };
        self.popup_state = PopupState::TestS3Result(test_result);
        Ok(())
    }

    pub async fn test_pg_connection(&mut self) -> Result<()> {
        let mut pg_config = tokio_postgres::Config::new();
        pg_config.host(self.pg_config.host.as_deref().unwrap_or("localhost"))
            .port(self.pg_config.port.unwrap_or(5432))
            .user(self.pg_config.username.as_deref().unwrap_or(""))
            .password(self.pg_config.password.as_deref().unwrap_or(""))
            .dbname(self.pg_config.db_name.as_deref().unwrap_or("postgres"));

        let connect_result = if self.pg_config.use_ssl {
            // For now, we'll accept invalid certs in test mode
            // In a production environment, you'd want to verify certs
            connect_ssl(&pg_config, false, None).await
        } else {
            connect_no_ssl(&pg_config).await
        };

        match connect_result {
            Ok(_) => {
                self.popup_state = PopupState::TestPgResult("PostgreSQL connection successful!".to_string());
            }
            Err(e) => {
                self.popup_state = PopupState::TestPgResult(format!("PostgreSQL connection failed: {}", e));
            }
        }
        Ok(())
    }
    pub fn new(config: S3Config, pg_config: PostgresConfig) -> Self {
        Self {
            config,
            pg_config,
            snapshots: Vec::new(),
            state: ListState::default(),
            s3_client: None,
            focus: FocusField::SnapshotList,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            popup_state: PopupState::Hidden,
        }
    }

    pub async fn verify_s3_settings(&self) -> Result<()> {
        debug!("Verifying S3 settings");
        if self.config.bucket.is_empty() {
            error!("S3 bucket name is empty");
            return Err(anyhow!("S3 bucket name is required"));
        }
        if self.config.region.is_empty() {
            error!("AWS region is empty");
            return Err(anyhow!("AWS region is required"));
        }
        if self.config.access_key_id.is_none() || self.config.secret_access_key.is_none() {
            error!("AWS credentials are missing");
            return Err(anyhow!("AWS credentials are required"));
        }
        Ok(())
    }

    fn set_error(&mut self, message: Option<String>) {
        self.config.error_message = message;
    }

    async fn init_s3_client(&mut self) -> Result<()> {
        if let Err(e) = self.verify_s3_settings().await {
            error!("Failed to verify S3 settings: {}", e);
            self.set_error(Some(format!("Error: {}", e)));
            return Err(e);
        }
        debug!("Initializing S3 client with config: {:?}", self.config);
        debug!("Creating config loader with region: {}", self.config.region);
        let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(self.config.region.clone()));

        if let Some(endpoint) = &self.config.endpoint_url {
            debug!("Using custom endpoint URL: {}", endpoint);
            config_loader = config_loader.endpoint_url(endpoint);
        }

        if let Some(access_key_id) = &self.config.access_key_id {
            debug!("Using provided access key ID");
            if let Some(secret_access_key) = &self.config.secret_access_key {
                config_loader = config_loader
                    .credentials_provider(Credentials::new(
                        access_key_id,
                        secret_access_key,
                        None,
                        None,
                        "Custom",
                    ));
            }
        }

        let config = config_loader.load().await;
        let mut builder = aws_sdk_s3::config::Builder::from(&config);
        if self.config.path_style {
            debug!("Enabling path-style addressing");
            builder = builder.force_path_style(true);
        }

        self.s3_client = Some(S3Client::from_conf(builder.build()));
        Ok(())
    }



    pub async fn load_snapshots(&mut self) -> Result<()> {
        debug!("Loading snapshots from bucket: {}", self.config.bucket);
        if self.s3_client.is_none() {
            if let Err(e) = self.init_s3_client().await {
                error!("Failed to initialize S3 client: {}", e);
                return Err(e);
            }
        }

        if self.config.bucket.is_empty() {
            return Ok(());
        }

        let client = self.s3_client.as_ref().ok_or_else(|| {
            error!("S3 client not initialized");
            anyhow!("S3 client not initialized")
        })?;

        debug!("Listing objects in bucket: {}", self.config.bucket);
        let mut request = client
            .list_objects_v2()
            .bucket(&self.config.bucket);

        if !self.config.prefix.is_empty() {
            debug!("Using prefix filter: {}", self.config.prefix);
            request = request.prefix(&self.config.prefix);
        }

        let objects = match request.send().await
        {
            Ok(objects) => objects,
            Err(e) => {
                error!("Failed to list objects in bucket {}: {}", self.config.bucket, e);
                warn!("Please verify your S3 settings and try again");
                self.set_error(Some(format!("Error: Failed to list objects in bucket: {}", e)));
                return Err(anyhow!("Failed to list objects in bucket: {}", e));
            }
        };

        if let Some(contents) = objects.contents {
            self.snapshots = contents
                .into_iter()
                .filter_map(|obj| {
                    match (obj.key, obj.size, obj.last_modified) {
                        (Some(key), size, Some(last_modified)) => Some(BackupMetadata {
                            key: key.clone(),
                            size: size.unwrap_or(0),
                            last_modified,
                        }),
                        _ => None,
                    }
                })
                .collect();
            debug!("Found {} snapshots in bucket", self.snapshots.len());
            self.snapshots.sort_by(|a, b| a.key.cmp(&b.key)); // Sort alphabetically
        }

        if self.snapshots.is_empty() {
            self.set_error(Some(format!("No snapshots found in bucket '{}'", self.config.bucket)));
        } else {
            self.set_error(None);
            self.state.select(Some(0));
        }

        Ok(())
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.snapshots.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.snapshots.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn selected_snapshot(&self) -> Option<&BackupMetadata> {
        self.state
            .selected()
            .and_then(|i| self.snapshots.get(i))
    }
}

#[allow(dead_code)]
pub async fn run_tui(
    bucket: Option<String>,
    region: Option<String>,
    prefix: Option<String>,
    endpoint_url: Option<String>,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    path_style: bool,
) -> Result<Option<String>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut browser = SnapshotBrowser::new(
        S3Config {
            bucket: bucket.unwrap_or_default(),
            region: region.unwrap_or_else(|| "us-west-2".to_string()),
            prefix: prefix.unwrap_or_default(),
            endpoint_url,
            access_key_id,
            secret_access_key,
            path_style,
            error_message: None,
        },
        PostgresConfig::default(),
    );
    browser.popup_state = PopupState::Hidden;
    browser.load_snapshots().await?;

    let result = run_app(&mut terminal, browser).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

#[allow(dead_code)]
fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length((r.height - height) / 2),
            Constraint::Length(height),
            Constraint::Length((r.height - height) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut browser: SnapshotBrowser) -> Result<Option<String>> {
    // Initial snapshot load
    if let Err(e) = browser.load_snapshots().await {
        error!("Failed to load initial snapshots: {}", e);
    }
    loop {
        terminal.draw(|f| ui(f, &mut browser))?;

        if let Event::Key(key) = event::read()? {
            match browser.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') => return Ok(None),
                    KeyCode::Char('t') => {
                        match browser.focus {
                            FocusField::Bucket | FocusField::Region | FocusField::Prefix |
                            FocusField::EndpointUrl | FocusField::AccessKeyId |
                            FocusField::SecretAccessKey | FocusField::PathStyle => {
                                if let Err(e) = browser.test_s3_connection().await {
                                    browser.popup_state = PopupState::TestS3Result(format!("Error: {}", e));
                                }
                            }
                            FocusField::PgHost | FocusField::PgPort | FocusField::PgUsername |
                            FocusField::PgPassword | FocusField::PgSsl | FocusField::PgDbName => {
                                if let Err(e) = browser.test_pg_connection().await {
                                    browser.popup_state = PopupState::TestPgResult(format!("Error: {}", e));
                                }
                            }
                            _ => {}
                        }
                    },
                    KeyCode::Enter => {
                        if browser.focus == FocusField::SnapshotList {
                            if browser.selected_snapshot().is_some() {
                                browser.popup_state = PopupState::ConfirmRestore;
                            }
                        }
                    }
                    KeyCode::Tab => {
                        browser.focus = match browser.focus {
                            FocusField::SnapshotList => FocusField::Bucket,
                            // S3 Settings
                            FocusField::Bucket => FocusField::Region,
                            FocusField::Region => FocusField::Prefix,
                            FocusField::Prefix => FocusField::EndpointUrl,
                            FocusField::EndpointUrl => FocusField::AccessKeyId,
                            FocusField::AccessKeyId => FocusField::SecretAccessKey,
                            FocusField::SecretAccessKey => FocusField::PathStyle,
                            FocusField::PathStyle => FocusField::PgHost,
                            // PostgreSQL Settings
                            FocusField::PgHost => FocusField::PgPort,
                            FocusField::PgPort => FocusField::PgUsername,
                            FocusField::PgUsername => FocusField::PgPassword,
                            FocusField::PgPassword => FocusField::PgSsl,
                            FocusField::PgSsl => FocusField::PgDbName,
                            FocusField::PgDbName => FocusField::SnapshotList,

                        };
                    }
                    // Edit mode
                    KeyCode::Char('e') if browser.focus != FocusField::SnapshotList => {
                        browser.input_mode = InputMode::Editing;
                        browser.input_buffer = match browser.focus {
                            FocusField::Bucket => browser.config.bucket.clone(),
                            FocusField::Region => browser.config.region.clone(),
                            FocusField::Prefix => browser.config.prefix.clone(),
                            FocusField::EndpointUrl => browser.config.endpoint_url.clone().unwrap_or_default(),
                            FocusField::AccessKeyId => browser.config.access_key_id.clone().unwrap_or_default(),
                            FocusField::SecretAccessKey => browser.config.secret_access_key.clone().unwrap_or_default(),
                            FocusField::PathStyle => browser.config.path_style.to_string(),
                            FocusField::PgHost => browser.pg_config.host.clone().unwrap_or_default(),
                            FocusField::PgPort => browser.pg_config.port.map(|p| p.to_string()).unwrap_or_default(),
                            FocusField::PgUsername => browser.pg_config.username.clone().unwrap_or_default(),
                            FocusField::PgPassword => browser.pg_config.password.clone().unwrap_or_default(),
                            FocusField::PgSsl => browser.pg_config.use_ssl.to_string(),
                            FocusField::PgDbName => browser.pg_config.db_name.clone().unwrap_or_default(),
                            _ => String::new(),
                        };
                    }

                    // S3 Settings shortcuts (1-7)
                    KeyCode::Char('b') => browser.focus = FocusField::Bucket,
                    KeyCode::Char('R') => browser.focus = FocusField::Region,
                    KeyCode::Char('x') => browser.focus = FocusField::Prefix,
                    KeyCode::Char('E') => browser.focus = FocusField::EndpointUrl,
                    KeyCode::Char('a') => browser.focus = FocusField::AccessKeyId,
                    KeyCode::Char('s') => browser.focus = FocusField::SecretAccessKey,
                    KeyCode::Char('y') => {
                        if browser.popup_state == PopupState::ConfirmRestore {
                            if let Some(snapshot) = browser.selected_snapshot() {
                                return Ok(Some(snapshot.key.clone()));
                            }
                        } else {
                            browser.focus = FocusField::PathStyle;
                        }
                    },
                    // PostgreSQL Settings shortcuts (a-h)
                    KeyCode::Char('h') => browser.focus = FocusField::PgHost,
                    KeyCode::Char('p') => browser.focus = FocusField::PgPort,
                    KeyCode::Char('u') => browser.focus = FocusField::PgUsername,
                    KeyCode::Char('f') => browser.focus = FocusField::PgPassword,
                    KeyCode::Char('l') => browser.focus = FocusField::PgSsl,
                    KeyCode::Char('n') => {
                        if browser.popup_state == PopupState::ConfirmRestore {
                            browser.popup_state = PopupState::Hidden;
                        } else {
                            browser.focus = FocusField::PgDbName;
                        }
                    },
                    KeyCode::Esc => {
                        match browser.popup_state {
                            PopupState::TestS3Result(_) | PopupState::TestPgResult(_) | PopupState::ConfirmRestore => {
                                browser.popup_state = PopupState::Hidden;
                            },
                            _ => {}
                        }
                    },
                    KeyCode::Down | KeyCode::Char('j') if browser.focus == FocusField::SnapshotList => browser.next(),
                    KeyCode::Up | KeyCode::Char('k') if browser.focus == FocusField::SnapshotList => browser.previous(),
                    KeyCode::Char('r') => {
                        if let Err(e) = browser.load_snapshots().await {
                            error!("Failed to refresh snapshots: {}", e);
                        }
                    },
                    KeyCode::Char('e') => {
                        browser.input_mode = InputMode::Editing;
                        // Pre-populate input buffer with current value based on focus
                        browser.input_buffer = match browser.focus {
                            FocusField::Bucket => browser.config.bucket.clone(),
                            FocusField::Region => browser.config.region.clone(),
                            FocusField::Prefix => browser.config.prefix.clone(),
                            FocusField::EndpointUrl => browser.config.endpoint_url.clone().unwrap_or_default(),
                            FocusField::AccessKeyId => browser.config.access_key_id.clone().unwrap_or_default(),
                            FocusField::SecretAccessKey => browser.config.secret_access_key.clone().unwrap_or_default(),
                            FocusField::PathStyle => browser.config.path_style.to_string(),
                            FocusField::PgHost => browser.pg_config.host.clone().unwrap_or_default(),
                            FocusField::PgPort => browser.pg_config.port.map(|p| p.to_string()).unwrap_or_default(),
                            FocusField::PgUsername => browser.pg_config.username.clone().unwrap_or_default(),
                            FocusField::PgPassword => browser.pg_config.password.clone().unwrap_or_default(),
                            FocusField::PgSsl => browser.pg_config.use_ssl.to_string(),
                            FocusField::PgDbName => browser.pg_config.db_name.clone().unwrap_or_default(),
                            _ => String::new(),
                        };
                    },
                    _ => {},
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        if browser.focus == FocusField::SnapshotList {
                            browser.popup_state = PopupState::ConfirmRestore;
                        } else {
                            debug!("Enter key pressed, attempting to initialize S3 client");
                            browser.input_mode = InputMode::Normal;
                            match browser.focus {
                            FocusField::Bucket => browser.config.bucket = browser.input_buffer.clone(),
                            FocusField::Region => browser.config.region = browser.input_buffer.clone(),
                            FocusField::Prefix => browser.config.prefix = browser.input_buffer.clone(),
                            FocusField::EndpointUrl => {
                                browser.config.endpoint_url = if browser.input_buffer.is_empty() {
                                    None
                                } else {
                                    Some(browser.input_buffer.clone())
                                };
                            }
                            FocusField::AccessKeyId => {
                                browser.config.access_key_id = if browser.input_buffer.is_empty() {
                                    None
                                } else {
                                    Some(browser.input_buffer.clone())
                                };
                            }
                            FocusField::SecretAccessKey => {

                                browser.config.secret_access_key = if browser.input_buffer.is_empty() {
                                    None
                                } else {
                                    Some(browser.input_buffer.clone())
                                };
                            }
                            FocusField::PathStyle => {
                                browser.config.path_style = browser.input_buffer.to_lowercase() == "true";
                            }
                            _ => {}
                        }
                        // Clear any existing client to force reinitialization
                        browser.s3_client = None;
                        if let Err(e) = browser.load_snapshots().await {
                            error!("Failed to list snapshots: {}", e);
                        }
                        }
                    }
                    KeyCode::Esc => {
                        if browser.popup_state != PopupState::Hidden {
                            browser.popup_state = PopupState::Hidden;
                        } else {
                            browser.input_mode = InputMode::Normal;
                        }
                    }
                    KeyCode::Char(c) => {
                        if browser.popup_state != PopupState::Hidden {
                            match c {
                                'y' => {
                                    if let Some(snapshot) = browser.selected_snapshot() {
                                        info!("Selected backup for restore: {} ({})", snapshot.key, format_size(snapshot.size as u64, BINARY));
                                        return Ok(Some(snapshot.key.clone()));
                                    }
                                }
                                'n' => browser.popup_state = PopupState::Hidden,
                                _ => {},
                            }
                        } else {
                            browser.input_buffer.push(c);
                        }
                    }
                    KeyCode::Backspace => {
                        browser.input_buffer.pop();
                    }
                    _ => {}
                },
            }
        }
    }
}

fn ui(f: &mut Frame, browser: &mut SnapshotBrowser) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(3),  // Input Mode
            Constraint::Length(10), // Configuration
            Constraint::Min(0),     // Snapshots
        ])
        .split(f.size());

    // Input mode
    let input_mode = match browser.input_mode {
        InputMode::Normal => "Press 'e' to edit | 'q' to quit | 'r' to refresh",
        InputMode::Editing => "Press <Enter> to save | <Esc> to cancel",
    };


    // Split the snapshots area into list and help sections
    let list_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Snapshots list
            Constraint::Length(3),   // Help text
        ])
        .split(chunks[3]);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("PostgreSQL S3 Snapshots", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(input_mode, Style::default()),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Config section
    let config_block = Block::default()
        .title("Configuration")
        .borders(Borders::ALL);
    let config_area = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([
            Constraint::Ratio(1, 2),  // S3 Settings
            Constraint::Ratio(1, 2),  // PostgreSQL Settings
        ])
        .split(chunks[2]);

    // S3 Settings
    let s3_block = Block::default()
        .title("S3 Settings ")
        .borders(Borders::ALL);
    let s3_inner = s3_block.inner(config_area[0]);
    f.render_widget(s3_block, config_area[0]);

    let s3_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 2),
            Constraint::Ratio(1, 2),
        ])
        .split(s3_inner);

    let s3_left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Bucket (1)
            Constraint::Length(1),  // Region (2)
            Constraint::Length(1),  // Prefix (3)
            Constraint::Length(1),  // Endpoint (4)
        ])
        .split(s3_columns[0]);

    let s3_right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Access Key (5)
            Constraint::Length(1),  // Secret Key (6)
            Constraint::Length(1),  // Path Style (7)
            Constraint::Length(1),  // Empty
        ])
        .split(s3_columns[1]);

    // PostgreSQL Settings
    let pg_block = Block::default()
        .title("PostgreSQL Settings ")
        .borders(Borders::ALL);
    let pg_inner = pg_block.inner(config_area[1]);
    f.render_widget(pg_block, config_area[1]);

    let pg_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 2),
            Constraint::Ratio(1, 2),
        ])
        .split(pg_inner);

    let pg_left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Host (q)
            Constraint::Length(1),  // Port (w)
            Constraint::Length(1),  // Username (e)
        ])
        .split(pg_columns[0]);

    let pg_right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Password (r)
            Constraint::Length(1),  // SSL (t)
            Constraint::Length(1),  // DB Name (y)
        ])
        .split(pg_columns[1]);

    let prefix_style = if browser.focus == FocusField::Prefix {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let prefix_line = Line::from(vec![
        Span::raw("[x] Prefix: "),
        Span::styled(
            if browser.focus == FocusField::Prefix && browser.input_mode == InputMode::Editing {
                &browser.input_buffer
            } else {
                &browser.config.prefix
            },
            prefix_style,
        ),
    ]);

    let bucket_style = if browser.focus == FocusField::Bucket {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let region_style = if browser.focus == FocusField::Region {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let endpoint_style = if browser.focus == FocusField::EndpointUrl {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let access_key_style = if browser.focus == FocusField::AccessKeyId {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let secret_key_style = if browser.focus == FocusField::SecretAccessKey {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let path_style_style = if browser.focus == FocusField::PathStyle {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let bucket_line = Line::from(vec![
        Span::raw("[b] Bucket: "),
        Span::styled(&browser.config.bucket, bucket_style),
    ]);
    let region_line = Line::from(vec![
        Span::raw("[R] Region: "),
        Span::styled(&browser.config.region, region_style),
    ]);
    let endpoint_line = Line::from(vec![
        Span::raw("[E] Endpoint: "),
        Span::styled(
            browser.config.endpoint_url.as_deref().unwrap_or(""),
            endpoint_style,
        ),
    ]);

    fn mask_key(key: &str) -> String {
        if key.len() <= 8 {
            "*".repeat(key.len())
        } else {
            format!("{}.....{}",
                &key[..4],
                &key[key.len().saturating_sub(4)..]
            )
        }
    }

    let masked_access_key = mask_key(browser.config.access_key_id.as_deref().unwrap_or(""));
    let access_key_line = Line::from(vec![
        Span::raw("[a] Access Key ID: "),
        Span::styled(
            &masked_access_key,
            access_key_style,
        ),
    ]);

    let masked_secret_key = mask_key(browser.config.secret_access_key.as_deref().unwrap_or(""));
    let secret_key_line = Line::from(vec![
        Span::raw("[s] Secret Key: "),
        Span::styled(
            &masked_secret_key,
            secret_key_style,
        ),
    ]);

    let path_style_line = Line::from(vec![
        Span::raw("[y] Path Style: "),
        Span::styled(
            browser.config.path_style.to_string(),
            path_style_style,
        ),
    ]);

    // Render S3 settings
    f.render_widget(Paragraph::new(bucket_line), s3_left[0]);
    f.render_widget(Paragraph::new(region_line), s3_left[1]);
    f.render_widget(Paragraph::new(prefix_line), s3_left[2]);
    f.render_widget(Paragraph::new(endpoint_line), s3_left[3]);
    f.render_widget(Paragraph::new(access_key_line), s3_right[0]);
    f.render_widget(Paragraph::new(secret_key_line), s3_right[1]);
    f.render_widget(Paragraph::new(path_style_line), s3_right[2]);

    // Create PostgreSQL lines
    let host_style = if browser.focus == FocusField::PgHost { Style::default().fg(Color::Yellow) } else { Style::default() };
    let port_style = if browser.focus == FocusField::PgPort { Style::default().fg(Color::Yellow) } else { Style::default() };
    let username_style = if browser.focus == FocusField::PgUsername { Style::default().fg(Color::Yellow) } else { Style::default() };
    let password_style = if browser.focus == FocusField::PgPassword { Style::default().fg(Color::Yellow) } else { Style::default() };
    let ssl_style = if browser.focus == FocusField::PgSsl { Style::default().fg(Color::Yellow) } else { Style::default() };
    let dbname_style = if browser.focus == FocusField::PgDbName { Style::default().fg(Color::Yellow) } else { Style::default() };

    let host_line = Line::from(vec![
        Span::raw("[h] Host: "),
        Span::styled(
            browser.pg_config.host.as_deref().unwrap_or(""),
            host_style,
        ),
    ]);

    let port_line = Line::from(vec![
        Span::raw("[p] Port: "),
        Span::styled(
            browser.pg_config.port.map(|p| p.to_string()).unwrap_or_default(),
            port_style,
        ),
    ]);

    let username_line = Line::from(vec![
        Span::raw("[u] Username: "),
        Span::styled(
            browser.pg_config.username.as_deref().unwrap_or(""),
            username_style,
        ),
    ]);

    let masked_pg_password = mask_key(browser.pg_config.password.as_deref().unwrap_or(""));
    let password_line = Line::from(vec![
        Span::raw("[f] Password: "),
        Span::styled(
            &masked_pg_password,
            password_style,
        ),
    ]);

    let ssl_line = Line::from(vec![
        Span::raw("[l] SSL: "),
        Span::styled(
            if browser.pg_config.use_ssl { "Yes" } else { "No" },
            ssl_style,
        ),
    ]);

    let dbname_line = Line::from(vec![
        Span::raw("[n] DB Name: "),
        Span::styled(
            browser.pg_config.db_name.as_deref().unwrap_or(""),
            dbname_style,
        ),
    ]);

    // Render PostgreSQL settings
    f.render_widget(Paragraph::new(host_line), pg_left[0]);
    f.render_widget(Paragraph::new(port_line), pg_left[1]);
    f.render_widget(Paragraph::new(username_line), pg_left[2]);
    f.render_widget(Paragraph::new(password_line), pg_right[0]);
    f.render_widget(Paragraph::new(ssl_line), pg_right[1]);
    f.render_widget(Paragraph::new(dbname_line), pg_right[2]);
    f.render_widget(config_block, chunks[2]);

    // Input mode
    if browser.input_mode == InputMode::Editing {
        // Create a small rect at the top of the screen for input
        let input_area = Rect {
            x: 0,
            y: 0,
            width: f.size().width,
            height: 3,
        };
        let input = Paragraph::new(browser.input_buffer.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title("Input"));

        f.render_widget(Clear, input_area);
        f.render_widget(input, input_area);
    }

    let items: Vec<ListItem> = browser
        .snapshots
        .iter()
        .enumerate()
        .map(|(i, snapshot)| {
            let style = if Some(i) == browser.state.selected() {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let timestamp = snapshot.last_modified.as_secs_f64() as i64;
            let naive = NaiveDateTime::from_timestamp_opt(timestamp, 0).unwrap_or_default();
            let datetime: ChronoDateTime<Utc> = ChronoDateTime::from_naive_utc_and_offset(naive, Utc);
            let date = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
            let size = format_size(snapshot.size as u64, BINARY);
            let content = format!("{:<60} {} {}", snapshot.key, date, size);
            ListItem::new(content).style(style)
        })
        .collect();

    let snapshots_block = Block::default()
        .title("Snapshots")
        .borders(Borders::ALL);
    let inner = snapshots_block.inner(list_chunks[0]);
    f.render_widget(&snapshots_block, chunks[3]);

    let snapshots_list = List::new(items)
        .style(Style::default().fg(Color::White))
        .block(Block::default())
        .highlight_symbol(">> ");

    f.render_stateful_widget(snapshots_list, inner, &mut browser.state);

    let help = Paragraph::new("↑/↓: Navigate • Enter: Select • t: Test Connection • q: Quit")
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, list_chunks[1]);

    // Draw popups if active
    match &browser.popup_state {
        PopupState::ConfirmRestore => {
            if let Some(snapshot) = browser.selected_snapshot() {
                let popup_block = Block::default()
                    .title("Confirm Restore")
                    .borders(Borders::ALL);

                let area = centered_rect(60, 10, f.size());
                f.render_widget(Clear, area); // Clear the background

                let popup = Paragraph::new(vec![
                    Line::from(vec![Span::raw("Are you sure you want to restore this backup?")]),
                    Line::from(vec![]),
                    Line::from(vec![Span::raw("File: "), Span::styled(
                        &snapshot.key,
                        Style::default().add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![Span::raw("Date: "), Span::styled(
                        {
                            let timestamp = snapshot.last_modified.as_secs_f64() as i64;
                            let naive = NaiveDateTime::from_timestamp_opt(timestamp, 0).unwrap_or_default();
                            let datetime: ChronoDateTime<Utc> = ChronoDateTime::from_naive_utc_and_offset(naive, Utc);
                            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                        },
                        Style::default().add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![Span::raw("Size: "), Span::styled(
                        format_size(snapshot.size as u64, BINARY),
                        Style::default().add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![]),
                    Line::from(vec![Span::raw("Press 'y' to confirm or 'n' to cancel")]),
                ])
                .block(popup_block)
                .alignment(Alignment::Center);

                f.render_widget(popup, area);
            }
        }
        PopupState::TestS3Result(result) => {
            let area = centered_rect(60, 5, f.size());
            f.render_widget(Clear, area);
            let popup = Paragraph::new(vec![
                Line::from(vec![Span::raw("S3 Connection Test")]),
                Line::from(vec![]),
                Line::from(vec![Span::styled(
                    result,
                    Style::default().add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![]),
                Line::from(vec![Span::raw("Press Esc to dismiss")]),
            ])
            .block(Block::default().title("Test Result").borders(Borders::ALL))
            .alignment(Alignment::Center);
            f.render_widget(popup, area);
        }
        PopupState::TestPgResult(result) => {
            let area = centered_rect(60, 5, f.size());
            f.render_widget(Clear, area);
            let popup = Paragraph::new(vec![
                Line::from(vec![Span::raw("PostgreSQL Connection Test")]),
                Line::from(vec![]),
                Line::from(vec![Span::styled(
                    result,
                    Style::default().add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![]),
                Line::from(vec![Span::raw("Press Esc to dismiss")]),
            ])
            .block(Block::default().title("Test Result").borders(Borders::ALL))
            .alignment(Alignment::Center);
            f.render_widget(popup, area);
        }
        PopupState::Hidden => {}
    }

    if let Some(error) = &browser.config.error_message {
        let error_block = Block::default()
            .title("Error")
            .borders(Borders::ALL);
        let error_paragraph = Paragraph::new(error.as_str())
            .block(error_block);
        f.render_widget(error_paragraph, chunks[3]);
    }
}
