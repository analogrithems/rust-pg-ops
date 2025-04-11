use anyhow::Result;
use aws_sdk_s3::{Client as S3Client, config::Credentials};
use log::debug;
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



#[derive(Debug, Default)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub endpoint_url: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub path_style: bool,
}

#[derive(PartialEq)]
enum InputMode {
    Normal,
    Editing,
}

#[derive(PartialEq)]
enum FocusField {
    Bucket,
    Region,
    EndpointUrl,
    AccessKeyId,
    SecretAccessKey,
    PathStyle,
    SnapshotList,
}

pub struct SnapshotBrowser {
    snapshots: Vec<String>,
    state: ListState,
    s3_client: Option<S3Client>,
    config: S3Config,
    input_mode: InputMode,
    focus: FocusField,
    input_buffer: String,
}

impl SnapshotBrowser {
    pub fn new(
        bucket: Option<String>,
        region: Option<String>,
        endpoint_url: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        path_style: bool,
    ) -> Self {
        Self {
            snapshots: Vec::new(),
            state: ListState::default(),
            s3_client: None,
            config: S3Config {
                bucket: bucket.unwrap_or_default(),
                region: region.unwrap_or_else(|| "us-west-2".to_string()),
                endpoint_url,
                access_key_id,
                secret_access_key,
                path_style,
            },
            input_mode: InputMode::Normal,
            focus: FocusField::Bucket,
            input_buffer: String::new(),
        }
    }

    async fn init_s3_client(&mut self) -> Result<()> {
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
            self.init_s3_client().await?
        }

        if self.config.bucket.is_empty() {
            return Ok(());
        }

        let client = self.s3_client.as_ref().unwrap();
        debug!("Listing objects in bucket {}", self.config.bucket);
        let objects = client.list_objects_v2()
            .bucket(&self.config.bucket)
            .send()
            .await?;

        if let Some(contents) = objects.contents {
            self.snapshots = contents
                .into_iter()
                .filter_map(|obj| obj.key)
                .map(String::from)
                .collect();
            debug!("Found {} snapshots in bucket", self.snapshots.len());
            self.snapshots.sort();
        }

        if !self.snapshots.is_empty() {
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

    fn selected_snapshot(&self) -> Option<&str> {
        self.state
            .selected()
            .and_then(|i| self.snapshots.get(i))
            .map(|s| s.as_str())
    }
}

pub async fn run_tui(
    bucket: Option<String>,
    region: Option<String>,
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

    let mut browser = SnapshotBrowser::new(bucket, region, endpoint_url, access_key_id, secret_access_key, path_style);
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

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
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

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut browser: SnapshotBrowser) -> Result<Option<String>> {
    loop {
        terminal.draw(|f| ui(f, &mut browser))?;

        if let Event::Key(key) = event::read()? {
            match browser.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') => return Ok(None),
                    KeyCode::Enter => {
                        if browser.focus == FocusField::SnapshotList {
                            return Ok(browser.selected_snapshot().map(String::from));
                        }
                    }
                    KeyCode::Tab => {
                        browser.focus = match browser.focus {
                            FocusField::Bucket => FocusField::Region,
                            FocusField::Region => FocusField::EndpointUrl,
                            FocusField::EndpointUrl => FocusField::AccessKeyId,
                            FocusField::AccessKeyId => FocusField::SecretAccessKey,
                            FocusField::SecretAccessKey => FocusField::PathStyle,
                            FocusField::PathStyle => FocusField::SnapshotList,
                            FocusField::SnapshotList => FocusField::Bucket,
                        };
                    }
                    KeyCode::Char('e') => {
                        if browser.focus != FocusField::SnapshotList {
                            browser.input_mode = InputMode::Editing;
                            browser.input_buffer = match browser.focus {
                                FocusField::Bucket => browser.config.bucket.clone(),
                                FocusField::Region => browser.config.region.clone(),
                                FocusField::EndpointUrl => browser.config.endpoint_url.clone().unwrap_or_default(),
                                FocusField::AccessKeyId => browser.config.access_key_id.clone().unwrap_or_default(),
                                FocusField::SecretAccessKey => browser.config.secret_access_key.clone().unwrap_or_default(),
                                FocusField::PathStyle => browser.config.path_style.to_string(),
                                _ => String::new(),
                            };
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') if browser.focus == FocusField::SnapshotList => browser.next(),
                    KeyCode::Up | KeyCode::Char('k') if browser.focus == FocusField::SnapshotList => browser.previous(),
                    KeyCode::Char('r') => {
                        browser.load_snapshots().await?
                    }
                    _ => {}
                },
                InputMode::Editing => match key.code {
                    KeyCode::Enter => {
                        match browser.focus {
                            FocusField::Bucket => browser.config.bucket = browser.input_buffer.clone(),
                            FocusField::Region => browser.config.region = browser.input_buffer.clone(),
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
                        browser.input_mode = InputMode::Normal;
                        browser.load_snapshots().await?
                    }
                    KeyCode::Esc => {
                        browser.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char(c) => {
                        browser.input_buffer.push(c);
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
            Constraint::Length(3),   // Title
            Constraint::Length(15),  // Config
            Constraint::Min(1),      // Snapshots
            Constraint::Length(3),   // Help
        ])
        .split(f.size());

    let title = Paragraph::new(Line::from(vec![
        Span::styled("PostgreSQL S3 Snapshots", Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Config section
    let config_block = Block::default()
        .title("Configuration")
        .borders(Borders::ALL);
    let config_inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),  // Bucket
            Constraint::Length(1),  // Region
            Constraint::Length(1),  // Endpoint
            Constraint::Length(1),  // Access Key
            Constraint::Length(1),  // Secret Key
            Constraint::Length(1),  // Path Style
        ])
        .split(chunks[1]);

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
        Span::raw("Bucket: "),
        Span::styled(&browser.config.bucket, bucket_style),
    ]);
    let region_line = Line::from(vec![
        Span::raw("Region: "),
        Span::styled(&browser.config.region, region_style),
    ]);
    let endpoint_line = Line::from(vec![
        Span::raw("Endpoint: "),
        Span::styled(
            browser.config.endpoint_url.as_deref().unwrap_or(""),
            endpoint_style,
        ),
    ]);

    let access_key_line = Line::from(vec![
        Span::raw("Access Key ID: "),
        Span::styled(
            browser.config.access_key_id.as_deref().unwrap_or(""),
            access_key_style,
        ),
    ]);

    let secret_key_line = Line::from(vec![
        Span::raw("Secret Key: "),
        Span::styled(
            browser.config.secret_access_key.as_deref().unwrap_or(""),
            secret_key_style,
        ),
    ]);

    let path_style_line = Line::from(vec![
        Span::raw("Path Style: "),
        Span::styled(
            browser.config.path_style.to_string(),
            path_style_style,
        ),
    ]);

    f.render_widget(Paragraph::new(bucket_line), config_inner[0]);
    f.render_widget(Paragraph::new(region_line), config_inner[1]);
    f.render_widget(Paragraph::new(endpoint_line), config_inner[2]);
    f.render_widget(Paragraph::new(access_key_line), config_inner[3]);
    f.render_widget(Paragraph::new(secret_key_line), config_inner[4]);
    f.render_widget(Paragraph::new(path_style_line), config_inner[5]);
    f.render_widget(config_block, chunks[1]);

    // Input mode
    if browser.input_mode == InputMode::Editing {
        let input_style = Style::default().fg(Color::Yellow);
        let input = Paragraph::new(browser.input_buffer.as_str())
            .style(input_style)
            .block(Block::default().borders(Borders::ALL).title("Input"));
        let area = centered_rect(60, 3, f.size());
        f.render_widget(Clear, area);
        f.render_widget(input, area);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(f.size());

    let title = Paragraph::new(Line::from(vec![
        Span::styled("PostgreSQL Snapshots", Style::default().add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = browser
        .snapshots
        .iter()
        .map(|s| ListItem::new(s.as_str()))
        .collect();

    let list = List::new(items)
        .block(Block::default().title("Snapshots").borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol(">");

    f.render_stateful_widget(list, chunks[1], &mut browser.state);

    let help = Paragraph::new("↑/↓: Navigate • Enter: Select • q: Quit")
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}
