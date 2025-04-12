use anyhow::{anyhow, Result};
use aws_sdk_s3::{Client as S3Client, config::Credentials};
use crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode}, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}, execute};
use log::{debug, error, info};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;
use std::time::Duration;
use std::io::stdout;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;

use crate::ui::models::{S3Config, PostgresConfig, BackupMetadata, PopupState, InputMode, FocusField};

/// Snapshot browser for managing S3 backups
pub struct SnapshotBrowser {
    pub config: S3Config,
    pub pg_config: PostgresConfig,
    pub s3_client: Option<S3Client>,
    pub snapshots: Vec<BackupMetadata>,
    pub selected_idx: Option<usize>,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub focus: FocusField,
    pub popup_state: PopupState,
    pub temp_file: Option<String>,
}

impl SnapshotBrowser {
    pub async fn test_s3_connection(&mut self) -> Result<()> {
        if self.s3_client.is_none() {
            if let Err(e) = self.init_s3_client().await {
                self.popup_state = PopupState::Error(format!("Failed to initialize S3 client: {}", e));
                return Err(e);
            }
        }

        match self.s3_client.as_ref().unwrap().list_buckets().send().await {
            Ok(resp) => {
                let buckets = resp.buckets();
                let bucket_names: Vec<String> = buckets
                    .iter()
                    .filter_map(|b| b.name().map(|s| s.to_string()))
                    .collect();

                let result = format!("Successfully connected to S3!\nAvailable buckets: {}",
                    if bucket_names.is_empty() { "None".to_string() } else { bucket_names.join(", ") });
                self.popup_state = PopupState::TestS3Result(result);
                Ok(())
            },
            Err(e) => {
                let error_msg = format!("Failed to connect to S3: {}", e);
                self.popup_state = PopupState::Error(error_msg.clone());
                Err(anyhow!(error_msg))
            }
        }
    }

    pub async fn test_pg_connection(&mut self) -> Result<()> {
        // Validate PostgreSQL settings
        if self.pg_config.host.is_none() || self.pg_config.host.as_ref().unwrap().is_empty() {
            self.popup_state = PopupState::Error("PostgreSQL host is required".to_string());
            return Err(anyhow!("PostgreSQL host is required"));
        }

        if self.pg_config.port.is_none() {
            self.popup_state = PopupState::Error("PostgreSQL port is required".to_string());
            return Err(anyhow!("PostgreSQL port is required"));
        }

        if self.pg_config.username.is_none() || self.pg_config.username.as_ref().unwrap().is_empty() {
            self.popup_state = PopupState::Error("PostgreSQL username is required".to_string());
            return Err(anyhow!("PostgreSQL username is required"));
        }

        // Construct connection string
        let conn_string = format!(
            "host={} port={} user={} password={} {}{}",
            self.pg_config.host.as_ref().unwrap(),
            self.pg_config.port.unwrap(),
            self.pg_config.username.as_ref().unwrap(),
            self.pg_config.password.as_ref().unwrap_or(&String::new()),
            if self.pg_config.use_ssl { "sslmode=require " } else { "" },
            if let Some(db) = &self.pg_config.db_name { format!("dbname={}", db) } else { String::new() }
        );

        // For now, just show a success message
        self.popup_state = PopupState::TestPgResult(format!("Connection string: {}", conn_string));
        Ok(())
    }

    pub fn new(config: S3Config, pg_config: PostgresConfig) -> Self {
        Self {
            config,
            pg_config,
            s3_client: None,
            snapshots: Vec::new(),
            selected_idx: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            focus: FocusField::SnapshotList,
            popup_state: PopupState::Hidden,
            temp_file: None,
        }
    }

    pub fn verify_s3_settings(&self) -> Result<()> {
        if self.config.bucket.is_empty() {
            return Err(anyhow!("Bucket name is required"));
        }

        if self.config.region.is_empty() {
            return Err(anyhow!("Region is required"));
        }

        if self.config.endpoint_url.is_empty() {
            return Err(anyhow!("Endpoint URL is required"));
        }

        if self.config.access_key_id.is_empty() {
            return Err(anyhow!("Access Key ID is required"));
        }

        if self.config.secret_access_key.is_empty() {
            return Err(anyhow!("Secret Access Key is required"));
        }

        Ok(())
    }

    pub fn set_error(&mut self, message: Option<String>) {
        self.config.error_message = message;
    }

    pub async fn init_s3_client(&mut self) -> Result<()> {
        if let Err(e) = self.verify_s3_settings() {
            self.set_error(Some(e.to_string()));
            return Err(e);
        }

        // Clear any previous error
        self.set_error(None);

        let credentials = Credentials::new(
            &self.config.access_key_id,
            &self.config.secret_access_key,
            None, None, "postgres-manager"
        );

        let mut config_builder = aws_sdk_s3::config::Builder::new()
            .credentials_provider(credentials)
            .region(aws_sdk_s3::config::Region::new(self.config.region.clone()));

        if !self.config.endpoint_url.is_empty() {
            let endpoint_url = if !self.config.endpoint_url.starts_with("http") {
                format!("http://{}", self.config.endpoint_url)
            } else {
                self.config.endpoint_url.clone()
            };

            config_builder = config_builder.endpoint_url(endpoint_url);
        }

        if self.config.path_style {
            config_builder = config_builder.force_path_style(true);
        }

        // Add behavior version which is required by AWS SDK
        config_builder = config_builder.behavior_version(aws_sdk_s3::config::BehaviorVersion::latest());

        let config = config_builder.build();
        self.s3_client = Some(S3Client::from_conf(config));

        Ok(())
    }

    pub async fn load_snapshots(&mut self) -> Result<()> {
        if self.s3_client.is_none() {
            if let Err(e) = self.init_s3_client().await {
                return Err(e);
            }
        }

        let client = self.s3_client.as_ref().unwrap();

        let mut list_objects_builder = client.list_objects_v2()
            .bucket(&self.config.bucket);

        if !self.config.prefix.is_empty() {
            list_objects_builder = list_objects_builder.prefix(&self.config.prefix);
        }

        match list_objects_builder.send().await {
            Ok(resp) => {
                self.snapshots.clear();

                let contents = resp.contents();
                if !contents.is_empty() {
                    for obj in contents {
                        if let (Some(key), Some(size), Some(last_modified)) = (obj.key(), obj.size(), obj.last_modified()) {
                            self.snapshots.push(BackupMetadata {
                                key: key.to_string(),
                                size: size,
                                last_modified: last_modified.clone(),
                            });
                        }
                    }
                }

                // Sort by last modified, newest first
                self.snapshots.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));

                if !self.snapshots.is_empty() && self.selected_idx.is_none() {
                    self.selected_idx = Some(0);
                } else if self.snapshots.is_empty() {
                    self.selected_idx = None;
                } else if let Some(idx) = self.selected_idx {
                    if idx >= self.snapshots.len() {
                        self.selected_idx = Some(self.snapshots.len() - 1);
                    }
                }

                Ok(())
            },
            Err(e) => {
                self.set_error(Some(format!("Failed to list objects: {}", e)));
                Err(anyhow!("Failed to list objects: {}", e))
            }
        }
    }

    pub fn next(&mut self) {
        if let Some(idx) = self.selected_idx {
            if idx + 1 < self.snapshots.len() {
                self.selected_idx = Some(idx + 1);
            }
        } else if !self.snapshots.is_empty() {
            self.selected_idx = Some(0);
        }
    }

    pub fn previous(&mut self) {
        if let Some(idx) = self.selected_idx {
            if idx > 0 {
                self.selected_idx = Some(idx - 1);
            }
        } else if !self.snapshots.is_empty() {
            self.selected_idx = Some(self.snapshots.len() - 1);
        }
    }

    pub fn selected_snapshot(&self) -> Option<&BackupMetadata> {
        self.selected_idx.and_then(|idx| self.snapshots.get(idx))
    }

    pub async fn download_snapshot<B: Backend>(&mut self, snapshot: &BackupMetadata, terminal: &mut Terminal<B>, temp_path: &std::path::Path) -> Result<Option<String>> {
        // Clone the necessary data to avoid borrowing issues
        let temp_path_str = temp_path.to_string_lossy().to_string();
        let s3_client = self.s3_client.clone();
        let bucket = self.config.bucket.clone();

        // Start download
        self.popup_state = PopupState::Downloading(snapshot.clone(), 0.0, 0.0);
        self.temp_file = Some(temp_path_str.clone());

        // Track download rate
        let mut last_update = std::time::Instant::now();
        let mut last_bytes = 0u64;
        let mut current_rate = 0.0;

        // Begin downloading the file
        if let Some(client) = &s3_client {
            let get_obj = client.get_object()
                .bucket(&bucket)
                .key(&snapshot.key)
                .send()
                .await;

            match get_obj {
                Ok(resp) => {
                    if let Some(total_size) = resp.content_length() {
                        let mut file = tokio::fs::File::create(&temp_path).await?;
                        let mut stream = resp.body;
                        let mut downloaded: u64 = 0;

                        while let Some(chunk) = stream.try_next().await? {
                            file.write_all(&chunk).await?;
                            downloaded += chunk.len() as u64;
                            let progress = downloaded as f32 / total_size as f32;

                            // Calculate download rate
                            let now = std::time::Instant::now();
                            let elapsed = now.duration_since(last_update).as_secs_f64();
                            if elapsed >= 0.5 { // Update rate every 0.5 seconds
                                let bytes_since_last = downloaded - last_bytes;
                                current_rate = bytes_since_last as f64 / elapsed;
                                last_update = now;
                                last_bytes = downloaded;
                            }

                            // Check for user input (like ESC key) during download
                            if crossterm::event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
                                if let crossterm::event::Event::Key(key) = crossterm::event::read().unwrap_or(crossterm::event::Event::Key(crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Null, crossterm::event::KeyModifiers::NONE))) {
                                    if key.code == crossterm::event::KeyCode::Esc {
                                        log::debug!("User pressed ESC to cancel download during chunk processing");
                                        self.popup_state = PopupState::ConfirmCancel(snapshot.clone(), progress, current_rate);
                                        terminal.draw(|f| crate::ui::renderer::ui::<B>(f, self))?;
                                        continue;
                                    }
                                }
                            }

                            match &self.popup_state {
                                PopupState::ConfirmCancel(..) => {
                                    // Wait for user confirmation
                                    terminal.draw(|f| crate::ui::renderer::ui::<B>(f, self))?;
                                    continue;
                                },
                                PopupState::Hidden => {
                                    // Download was cancelled and confirmed
                                    log::debug!("Download cancelled by user");
                                    file.flush().await?;
                                    self.temp_file = None; // Reset temp file
                                    return Ok(None);
                                },
                                _ => {
                                    // Continue downloading
                                    self.popup_state = PopupState::Downloading(snapshot.clone(), progress, current_rate);
                                }
                            }
                            // Force a redraw to show progress
                            terminal.draw(|f| crate::ui::renderer::ui::<B>(f, self))?;
                            // Small delay to allow UI updates
                            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                        }
                        self.temp_file = Some(temp_path_str.clone());
                        log::info!("Download completed successfully: {}", temp_path_str);
                        self.popup_state = PopupState::Success("Download complete".to_string());
                        // Show success message briefly
                        terminal.draw(|f| crate::ui::renderer::ui::<B>(f, self))?;
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        return Ok(Some(temp_path_str));
                    } else {
                        log::warn!("Could not determine file size for snapshot: {}", snapshot.key);
                        self.popup_state = PopupState::Error("Could not determine file size".to_string());
                        return Ok(None);
                    }
                }
                Err(e) => {
                    log::error!("Failed to download snapshot {}: {}", snapshot.key, e);
                    self.popup_state = PopupState::Error(format!("Failed to download backup: {}", e));
                    return Ok(None);
                }
            }
        } else {
            log::warn!("Download attempted but S3 client not initialized");
            self.popup_state = PopupState::Error("S3 client not initialized".to_string());
            return Ok(None);
        }
    }
}

/// Run the TUI application
pub async fn run_tui(
    bucket: Option<String>,
    region: Option<String>,
    prefix: Option<String>,
    endpoint_url: Option<String>,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    path_style: bool,
) -> Result<Option<String>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let config = S3Config {
        bucket: bucket.unwrap_or_default(),
        region: region.unwrap_or_default(),
        prefix: prefix.unwrap_or_default(),
        endpoint_url: endpoint_url.unwrap_or_default(),
        access_key_id: access_key_id.unwrap_or_default(),
        secret_access_key: secret_access_key.unwrap_or_default(),
        path_style,
        error_message: None,
    };

    let pg_config = PostgresConfig::default();
    let browser = SnapshotBrowser::new(config, pg_config);

    // Run app
    let res = run_app(&mut terminal, browser).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

/// Run the application
pub async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut browser: SnapshotBrowser) -> Result<Option<String>> {
    // Initial load of snapshots
    if let Err(e) = browser.load_snapshots().await {
        debug!("Failed to load snapshots: {}", e);
    }

    loop {
        // Draw UI
        terminal.draw(|f| crate::ui::renderer::ui::<B>(f, &mut browser))?;

        // Handle events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match browser.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => {
                            debug!("User pressed 'q' to quit");
                            return Ok(None);
                        },
                        KeyCode::Esc => {
                            match &browser.popup_state {
                                PopupState::Downloading(snapshot, progress, rate) => {
                                    debug!("User pressed ESC to cancel download");
                                    // Show cancel confirmation
                                    browser.popup_state = PopupState::ConfirmCancel(snapshot.clone(), *progress, *rate);
                                }
                                PopupState::ConfirmRestore => {
                                    browser.popup_state = PopupState::Hidden;
                                },
                                PopupState::TestS3Result(_) | PopupState::TestPgResult(_) => {
                                    browser.popup_state = PopupState::Hidden;
                                }
                                _ => {}
                            }
                        },
                        KeyCode::Char('y') if matches!(browser.popup_state, PopupState::ConfirmCancel(..)) => {
                            debug!("User confirmed download cancel");
                            // User confirmed cancel
                            browser.popup_state = PopupState::Hidden;
                            browser.temp_file = None; // Reset temp file
                        },

                        KeyCode::Char('y') if matches!(browser.popup_state, PopupState::ConfirmRestore) => {
                            if let Some(snapshot) = browser.selected_snapshot().cloned() {
                                info!("User confirmed restore of snapshot: {}", snapshot.key);
                                // Create a temporary file
                                let temp_dir = tempfile::Builder::new().prefix("pg-backup-").tempdir()?;
                                let temp_path = temp_dir.path().join("backup.sql");

                                // Start download
                                match browser.download_snapshot(&snapshot, terminal, &temp_path).await {
                                    Ok(Some(downloaded_path)) => return Ok(Some(downloaded_path)),
                                    Ok(None) => {},  // Download was cancelled or failed
                                    Err(e) => {
                                        error!("Error during download: {}", e);
                                        browser.popup_state = PopupState::Error(format!("Download error: {}", e));
                                    }
                                }
                            }
                        },
                        KeyCode::Char('n') => match &browser.popup_state {
                            PopupState::ConfirmCancel(snapshot, progress, rate) => {
                                debug!("User denied download cancel");
                                // User denied cancel, resume download
                                browser.popup_state = PopupState::Downloading(snapshot.clone(), *progress, *rate);
                            }
                            PopupState::ConfirmRestore => {
                                browser.popup_state = PopupState::Hidden;
                            }
                            _ => {
                                browser.focus = FocusField::PgDbName;
                            }
                        },
                        KeyCode::Char('t') => {
                            debug!("User pressed 't' to test connection");
                            match browser.focus {
                                FocusField::Bucket | FocusField::Region | FocusField::Prefix |
                                FocusField::EndpointUrl | FocusField::AccessKeyId |
                                FocusField::SecretAccessKey | FocusField::PathStyle => {
                                    if let Err(e) = browser.test_s3_connection().await {
                                        browser.popup_state = PopupState::Error(format!("Error: {}", e));
                                    }
                                }
                                FocusField::PgHost | FocusField::PgPort | FocusField::PgUsername |
                                FocusField::PgPassword | FocusField::PgSsl | FocusField::PgDbName => {
                                    if let Err(e) = browser.test_pg_connection().await {
                                        browser.popup_state = PopupState::Error(format!("Error: {}", e));
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
                        },
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
                        },
                        // Edit mode
                        KeyCode::Char('e') if browser.focus != FocusField::SnapshotList => {
                            browser.input_mode = InputMode::Editing;
                            browser.input_buffer = match browser.focus {
                                FocusField::SnapshotList => String::new(),
                                FocusField::Bucket => browser.config.bucket.clone(),
                                FocusField::Region => browser.config.region.clone(),
                                FocusField::Prefix => browser.config.prefix.clone(),
                                FocusField::EndpointUrl => browser.config.endpoint_url.clone(),
                                FocusField::AccessKeyId => browser.config.access_key_id.clone(),
                                FocusField::SecretAccessKey => browser.config.secret_access_key.clone(),
                                FocusField::PathStyle => browser.config.path_style.to_string(),
                                FocusField::PgHost => browser.pg_config.host.clone().unwrap_or_default(),
                                FocusField::PgPort => browser.pg_config.port.map(|p| p.to_string()).unwrap_or_default(),
                                FocusField::PgUsername => browser.pg_config.username.clone().unwrap_or_default(),
                                FocusField::PgPassword => browser.pg_config.password.clone().unwrap_or_default(),
                                FocusField::PgSsl => browser.pg_config.use_ssl.to_string(),
                                FocusField::PgDbName => browser.pg_config.db_name.clone().unwrap_or_default()
                            };
                        },
                        // S3 Settings shortcuts
                        KeyCode::Char('b') if browser.input_mode == InputMode::Normal => browser.focus = FocusField::Bucket,
                        KeyCode::Char('R') if browser.input_mode == InputMode::Normal => browser.focus = FocusField::Region,
                        KeyCode::Char('x') if browser.input_mode == InputMode::Normal => browser.focus = FocusField::Prefix,
                        // Navigation shortcuts
                        KeyCode::Down | KeyCode::Char('j') if browser.focus == FocusField::SnapshotList => {
                            browser.next();
                        },
                        KeyCode::Up | KeyCode::Char('k') if browser.focus == FocusField::SnapshotList => {
                            browser.previous();
                        },
                        // S3 Settings shortcuts
                        KeyCode::Char('E') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::EndpointUrl;
                        },
                        KeyCode::Char('a') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::AccessKeyId;
                        },
                        KeyCode::Char('s') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::SecretAccessKey;
                        },
                        // PostgreSQL Settings shortcuts
                        KeyCode::Char('h') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::PgHost;
                        },
                        KeyCode::Char('p') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::PgPort;
                        },
                        KeyCode::Char('u') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::PgUsername;
                        },
                        KeyCode::Char('f') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::PgPassword;
                        },
                        KeyCode::Char('l') if browser.input_mode == InputMode::Normal => {
                            browser.focus = FocusField::PgSsl;
                        },
                        // State management

                        KeyCode::Char('r') => {
                            debug!("User pressed 'r' to refresh snapshots");
                            if let Err(e) = browser.load_snapshots().await {
                                browser.popup_state = PopupState::Error(format!("Error: {}", e));
                            }
                        },
                        KeyCode::Char('e') => {
                            browser.input_mode = InputMode::Editing;
                            // Pre-populate input buffer with current value based on focus
                            browser.input_buffer = match browser.focus {
                                FocusField::SnapshotList => String::new(),
                                FocusField::Bucket => browser.config.bucket.clone(),
                                FocusField::Region => browser.config.region.clone(),
                                FocusField::Prefix => browser.config.prefix.clone(),
                                FocusField::EndpointUrl => browser.config.endpoint_url.clone(),
                                FocusField::AccessKeyId => browser.config.access_key_id.clone(),
                                FocusField::SecretAccessKey => browser.config.secret_access_key.clone(),
                                FocusField::PathStyle => browser.config.path_style.to_string(),
                                FocusField::PgHost => browser.pg_config.host.clone().unwrap_or_default(),
                                FocusField::PgPort => browser.pg_config.port.map(|p| p.to_string()).unwrap_or_default(),
                                FocusField::PgUsername => browser.pg_config.username.clone().unwrap_or_default(),
                                FocusField::PgPassword => browser.pg_config.password.clone().unwrap_or_default(),
                                FocusField::PgSsl => browser.pg_config.use_ssl.to_string(),
                                FocusField::PgDbName => browser.pg_config.db_name.clone().unwrap_or_default()
                            };
                        },
                        // Handle any unmatched key
                        _ => {}
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
                                    FocusField::EndpointUrl => browser.config.endpoint_url = browser.input_buffer.clone(),
                                    FocusField::AccessKeyId => browser.config.access_key_id = browser.input_buffer.clone(),
                                    FocusField::SecretAccessKey => browser.config.secret_access_key = browser.input_buffer.clone(),
                                    FocusField::PathStyle => {
                                        browser.config.path_style = browser.input_buffer.to_lowercase() == "true";
                                    },
                                    FocusField::PgHost => browser.pg_config.host = Some(browser.input_buffer.clone()),
                                    FocusField::PgPort => {
                                        if browser.input_buffer.is_empty() {
                                            browser.pg_config.port = None;
                                        } else {
                                            match browser.input_buffer.parse::<u16>() {
                                                Ok(port) => browser.pg_config.port = Some(port),
                                                Err(_) => {
                                                    browser.popup_state = PopupState::Error("Invalid port number".to_string());
                                                }
                                            }
                                        }
                                    },
                                    FocusField::PgUsername => browser.pg_config.username = Some(browser.input_buffer.clone()),
                                    FocusField::PgPassword => browser.pg_config.password = Some(browser.input_buffer.clone()),
                                    FocusField::PgSsl => {
                                        browser.pg_config.use_ssl = browser.input_buffer.to_lowercase() == "true";
                                    },
                                    FocusField::PgDbName => browser.pg_config.db_name = Some(browser.input_buffer.clone()),
                                    _ => {}
                                }

                                // Try to initialize S3 client if all required fields are filled
                                if let Err(e) = browser.init_s3_client().await {
                                    debug!("Failed to initialize S3 client: {}", e);
                                } else {
                                    // Load snapshots if S3 client was initialized successfully
                                    if let Err(e) = browser.load_snapshots().await {
                                        debug!("Failed to load snapshots: {}", e);
                                    }
                                }
                            }
                        },
                        KeyCode::Char(c) => {
                            browser.input_buffer.push(c);
                        },
                        KeyCode::Backspace => {
                            browser.input_buffer.pop();
                        },
                        KeyCode::Esc => {
                            browser.input_mode = InputMode::Normal;
                        },
                        _ => {}
                    },
                }
            }
        }

        // Check if we need to show a success message briefly
        if let PopupState::Success(_) = &browser.popup_state {
            sleep(Duration::from_secs(1)).await;
            browser.popup_state = PopupState::Hidden;
        }
    }
}
