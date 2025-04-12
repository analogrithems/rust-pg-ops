use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use chrono::{DateTime, Utc};

use crate::ui::models::{FocusField, PopupState};
use crate::ui::browser::SnapshotBrowser;

/// Helper function to create a centered rect
pub fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - height) / 2),
                Constraint::Length(height),
                Constraint::Percentage((100 - height) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

/// Render the UI
pub fn ui<B: Backend>(f: &mut Frame, browser: &mut SnapshotBrowser) {
    // Create the layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),  // Title
                Constraint::Length(9),  // S3 Settings
                Constraint::Length(8),  // PostgreSQL Settings
                Constraint::Min(10),    // Snapshot List
            ]
            .as_ref(),
        )
        .split(f.size());

    // Title
    let title = Paragraph::new("PostgreSQL S3 Backup Manager")
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // S3 Settings
    let s3_settings_block = Block::default()
        .title("S3 Settings")
        .borders(Borders::ALL);
    f.render_widget(s3_settings_block, chunks[1]);

    // S3 Settings Content
    let s3_settings_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(chunks[1]);

    // Bucket
    let bucket_style = if browser.focus == FocusField::Bucket {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let bucket_text = format!("Bucket: {}", browser.config.bucket);
    let bucket = Paragraph::new(bucket_text)
        .style(bucket_style);
    f.render_widget(bucket, s3_settings_chunks[0]);

    // Region
    let region_style = if browser.focus == FocusField::Region {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let region_text = format!("Region: {}", browser.config.region);
    let region = Paragraph::new(region_text)
        .style(region_style);
    f.render_widget(region, s3_settings_chunks[1]);

    // Prefix
    let prefix_style = if browser.focus == FocusField::Prefix {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let prefix_text = format!("Prefix: {}", browser.config.prefix);
    let prefix = Paragraph::new(prefix_text)
        .style(prefix_style);
    f.render_widget(prefix, s3_settings_chunks[2]);

    // Endpoint URL
    let endpoint_style = if browser.focus == FocusField::EndpointUrl {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let endpoint_text = format!("Endpoint URL: {}", browser.config.endpoint_url);
    let endpoint = Paragraph::new(endpoint_text)
        .style(endpoint_style);
    f.render_widget(endpoint, s3_settings_chunks[3]);

    // Access Key ID
    let access_key_style = if browser.focus == FocusField::AccessKeyId {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let access_key_text = format!("Access Key ID: {}", browser.config.masked_access_key());
    let access_key = Paragraph::new(access_key_text)
        .style(access_key_style);
    f.render_widget(access_key, s3_settings_chunks[4]);

    // Secret Access Key
    let secret_key_style = if browser.focus == FocusField::SecretAccessKey {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let secret_key_text = format!("Secret Access Key: {}", browser.config.masked_secret_key());
    let secret_key = Paragraph::new(secret_key_text)
        .style(secret_key_style);
    f.render_widget(secret_key, s3_settings_chunks[5]);

    // Path Style
    let path_style_style = if browser.focus == FocusField::PathStyle {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let path_style_text = format!("Path Style: {}", browser.config.path_style);
    let path_style = Paragraph::new(path_style_text)
        .style(path_style_style);
    f.render_widget(path_style, s3_settings_chunks[6]);

    // PostgreSQL Settings
    let pg_settings_block = Block::default()
        .title("PostgreSQL Settings")
        .borders(Borders::ALL);
    f.render_widget(pg_settings_block, chunks[2]);

    // PostgreSQL Settings Content
    let pg_settings_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ]
            .as_ref(),
        )
        .split(chunks[2]);

    // Host
    let host_style = if browser.focus == FocusField::PgHost {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let host_text = format!("Host: {}", browser.pg_config.host.as_ref().unwrap_or(&String::new()));
    let host = Paragraph::new(host_text)
        .style(host_style);
    f.render_widget(host, pg_settings_chunks[0]);

    // Port
    let port_style = if browser.focus == FocusField::PgPort {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let port_text = format!("Port: {}", browser.pg_config.port.map(|p| p.to_string()).unwrap_or_default());
    let port = Paragraph::new(port_text)
        .style(port_style);
    f.render_widget(port, pg_settings_chunks[1]);

    // Username
    let username_style = if browser.focus == FocusField::PgUsername {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let username_text = format!("Username: {}", browser.pg_config.username.as_ref().unwrap_or(&String::new()));
    let username = Paragraph::new(username_text)
        .style(username_style);
    f.render_widget(username, pg_settings_chunks[2]);

    // Password
    let password_style = if browser.focus == FocusField::PgPassword {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let password_text = format!("Password: {}", if browser.pg_config.password.is_some() { "********" } else { "" });
    let password = Paragraph::new(password_text)
        .style(password_style);
    f.render_widget(password, pg_settings_chunks[3]);

    // SSL
    let ssl_style = if browser.focus == FocusField::PgSsl {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let ssl_text = format!("SSL: {}", browser.pg_config.use_ssl);
    let ssl = Paragraph::new(ssl_text)
        .style(ssl_style);
    f.render_widget(ssl, pg_settings_chunks[4]);

    // Database
    let db_style = if browser.focus == FocusField::PgDbName {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let db_text = format!("Database: {}", browser.pg_config.db_name.as_ref().unwrap_or(&String::new()));
    let db = Paragraph::new(db_text)
        .style(db_style);
    f.render_widget(db, pg_settings_chunks[5]);

    // Snapshot List
    let snapshot_style = if browser.focus == FocusField::SnapshotList {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let snapshot_block = Block::default()
        .title("Snapshots")
        .borders(Borders::ALL)
        .style(snapshot_style);

    let snapshot_items: Vec<ListItem> = browser.snapshots
        .iter()
        .enumerate()
        .map(|(i, snapshot)| {
            // Convert AWS DateTime to chrono DateTime
            let timestamp = snapshot.last_modified.as_secs_f64();
            let dt: DateTime<Utc> = DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_default();
            let formatted_date = dt.format("%Y-%m-%d %H:%M:%S").to_string();
            let size_mb = snapshot.size as f64 / 1024.0 / 1024.0;
            let content = format!("{} - {:.2} MB - {}", snapshot.key, size_mb, formatted_date);
            let style = if Some(i) == browser.selected_idx {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![Span::styled(content, style)]))
        })
        .collect();

    let snapshot_list = List::new(snapshot_items)
        .block(snapshot_block);
    f.render_widget(snapshot_list, chunks[3]);

    // Show help text at the bottom
    let help_text = match browser.input_mode {
        crate::ui::models::InputMode::Normal => "Press 'q' to quit, 'e' to edit, 't' to test connection, 'r' to refresh, Enter to select",
        crate::ui::models::InputMode::Editing => "Press Esc to cancel, Enter to save",
    };
    let help_paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);

    let help_rect = Rect::new(
        chunks[3].x,
        chunks[3].y + chunks[3].height - 1,
        chunks[3].width,
        1,
    );
    f.render_widget(help_paragraph, help_rect);

    // We'll handle popups at the end to ensure they're on top

    // Show popup if needed - render last to ensure they're on top
    match &browser.popup_state {
        PopupState::ConfirmRestore(snapshot) => {
            let area = centered_rect(60, 5, f.size());
            // Clear the area where the popup will be rendered
            f.render_widget(ratatui::widgets::Clear, area);
            let popup = Paragraph::new(vec![
                Line::from(vec![Span::raw(format!("Are you sure you want to restore this backup '{}'?", snapshot.key))]),
                Line::from(vec![]),
                Line::from(vec![Span::raw("Press 'y' to confirm, 'n' to cancel")]),
            ])
            .block(Block::default().title("Confirm Restore").borders(Borders::ALL))
            .alignment(Alignment::Center);
            f.render_widget(popup, area);
        }
        PopupState::Downloading(snapshot, progress, rate) => {
            let area = centered_rect(60, 5, f.size());
            // Clear the area where the popup will be rendered
            f.render_widget(ratatui::widgets::Clear, area);
            let rate_mb = *rate / 1024.0 / 1024.0;
            let popup = Paragraph::new(vec![
                Line::from(vec![Span::raw(format!("Downloading: {}", snapshot.key))]),
                Line::from(vec![Span::raw(format!("Progress: {:.1}% ({:.2} MB/s)", *progress * 100.0, rate_mb))]),
                Line::from(vec![]),
                Line::from(vec![Span::raw("Press Esc to cancel")]),
            ])
            .block(Block::default().title("Downloading").borders(Borders::ALL))
            .alignment(Alignment::Center);
            f.render_widget(popup, area);
        }
        PopupState::ConfirmCancel(snapshot, progress, rate) => {
            let area = centered_rect(60, 5, f.size());
            // Clear the area where the popup will be rendered
            f.render_widget(ratatui::widgets::Clear, area);
            let rate_mb = *rate / 1024.0 / 1024.0;
            let popup = Paragraph::new(vec![
                Line::from(vec![Span::raw(format!("Cancel download of: {}", snapshot.key))]),
                Line::from(vec![Span::raw(format!("Progress: {:.1}% ({:.2} MB/s)", *progress * 100.0, rate_mb))]),
                Line::from(vec![]),
                Line::from(vec![Span::raw("Press 'y' to confirm cancel, 'n' to continue downloading")]),
            ])
            .block(Block::default().title("Confirm Cancel").borders(Borders::ALL))
            .alignment(Alignment::Center);
            f.render_widget(popup, area);
        }
        PopupState::Error(message) => {
            let area = centered_rect(60, 5, f.size());
            // Clear the area where the popup will be rendered
            f.render_widget(ratatui::widgets::Clear, area);
            let popup = Paragraph::new(vec![
                Line::from(vec![Span::styled(
                    message,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![]),
                Line::from(vec![Span::raw("Press Esc to dismiss")]),
            ])
            .block(Block::default().title("Error").borders(Borders::ALL))
            .alignment(Alignment::Center);
            f.render_widget(popup, area);
        }
        PopupState::Success(message) => {
            let area = centered_rect(60, 5, f.size());
            // Clear the area where the popup will be rendered
            f.render_widget(ratatui::widgets::Clear, area);
            let popup = Paragraph::new(vec![
                Line::from(vec![Span::styled(
                    message,
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                )]),
            ])
            .block(Block::default().title("Success").borders(Borders::ALL))
            .alignment(Alignment::Center);
            f.render_widget(popup, area);
        }
        PopupState::TestS3Result(result) => {
            let area = centered_rect(60, 7, f.size());
            // Clear the area where the popup will be rendered
            f.render_widget(ratatui::widgets::Clear, area);
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
            let area = centered_rect(60, 7, f.size());
            // Clear the area where the popup will be rendered
            f.render_widget(ratatui::widgets::Clear, area);
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
        _ => {}
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
