use insta::assert_debug_snapshot;
use postgres_manager::ui::renderer::centered_rect;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[test]
fn test_centered_rect() {
    // Test the centered_rect function with various inputs
    let base_rect = Rect::new(0, 0, 100, 50);
    
    // Test with different percentages and heights
    let rect1 = centered_rect(60, 10, base_rect);
    let rect2 = centered_rect(80, 20, base_rect);
    let rect3 = centered_rect(40, 30, base_rect);
    
    assert_debug_snapshot!("centered_rect_60_10", rect1);
    assert_debug_snapshot!("centered_rect_80_20", rect2);
    assert_debug_snapshot!("centered_rect_40_30", rect3);
}

#[test]
fn test_layout_constraints() {
    // Test various layout configurations used in the renderer
    let base_rect = Rect::new(0, 0, 100, 50);
    
    // Test vertical layout with fixed constraints
    let vertical_layout = Layout::default()
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
        .split(base_rect);
    
    assert_debug_snapshot!("vertical_layout", vertical_layout);
    
    // Test horizontal layout with percentage constraints
    let horizontal_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ]
            .as_ref(),
        )
        .split(base_rect);
    
    assert_debug_snapshot!("horizontal_layout", horizontal_layout);
}

#[test]
fn test_nested_layouts() {
    // Test nested layouts similar to those used in the renderer
    let base_rect = Rect::new(0, 0, 100, 50);
    
    // Create a main vertical layout
    let main_chunks = Layout::default()
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
        .split(base_rect);
    
    // Create a nested layout for S3 settings
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
        .split(main_chunks[1]);
    
    assert_debug_snapshot!("s3_settings_chunks", s3_settings_chunks);
    
    // Create a nested layout for PostgreSQL settings
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
        .split(main_chunks[2]);
    
    assert_debug_snapshot!("pg_settings_chunks", pg_settings_chunks);
}
