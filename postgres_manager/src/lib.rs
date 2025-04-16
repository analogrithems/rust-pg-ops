// Export modules for testing and usage
pub mod ui;
pub mod config;
pub mod backup;
pub mod postgres;

// Re-export other modules as needed
pub use crate::ui::browser::run_tui;


