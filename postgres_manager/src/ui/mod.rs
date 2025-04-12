// UI module for the postgres manager
pub mod models;
pub mod browser;
pub mod renderer;

// Re-export the main entry point
pub use browser::run_tui;
