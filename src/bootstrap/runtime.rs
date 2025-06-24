use anyhow::Result;
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;

use crate::{bootstrap::logging, config::cfg, core::application::Application};

/// Main application runtime entry point
///
/// This function handles:
/// - Tracing initialization
/// - Configuration loading
/// - Application startup
/// - Returns the tracing guard to ensure proper log flushing
#[tokio::main]
pub async fn run(config_path: Option<PathBuf>, is_daemon: bool) -> Result<Option<WorkerGuard>> {
    // Initialize tracing INSIDE the tokio runtime
    let guard = logging::init_tracing(is_daemon)?;

    // Log successful initialization
    tracing::info!("tt_riingd daemon starting up");
    if is_daemon {
        tracing::info!("Running in daemon mode with syslog logging");
    }

    let config_manager = cfg::ConfigManager::load(config_path).await?;
    Application::builder()
        .with_config_manager(config_manager)
        .build()
        .await?
        .run()
        .await?;

    Ok(guard)
}
