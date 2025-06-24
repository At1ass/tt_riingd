use anyhow::Result;
use clap::Parser;

use tt_riingd::bootstrap::{cli, daemon, runtime};

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    // Initialize daemon mode first
    daemon::init_daemon(cli.daemonize)?;

    // Run the main application (tracing is initialized inside runtime)
    let result = runtime::run(cli.config, cli.daemonize);

    // Log shutdown - this will work because tracing is initialized inside runtime
    match &result {
        Ok(_) => tracing::info!("tt_riingd daemon shutting down normally"),
        Err(e) => tracing::error!("tt_riingd daemon shutting down due to error: {}", e),
    }

    // Guard will be dropped here, ensuring logs are flushed
    result.map(|_| ())
}
