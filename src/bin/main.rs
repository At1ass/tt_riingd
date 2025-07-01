use anyhow::Result;
use clap::Parser;

use tt_riingd::bootstrap::{cli, daemon, runtime};

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    daemon::init_daemon(cli.daemonize)?;

    let result = runtime::run(cli.config, cli.daemonize);

    match &result {
        Ok(_) => tracing::info!("tt_riingd daemon shutting down normally"),
        Err(e) => tracing::error!("tt_riingd daemon shutting down due to error: {}", e),
    }

    result.map(|_| ())
}
