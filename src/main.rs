mod app_context;
mod application;
mod cli;
mod config;
mod coordinator;
mod drivers;
mod event;
mod fan_curve;
mod interface;
mod mappings;
mod providers;
mod task_manager;
mod temperature_sensors;

use std::{fs::File, io::Write, path::PathBuf};

use anyhow::{Result, anyhow};
use application::Application;
use clap::Parser;
use daemonize::Daemonize;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Custom syslog writer that implements std::io::Write
///
/// This writer integrates with the system syslog daemon, which is the standard
/// approach for system daemons like tt_riingd.
struct SyslogWriter {
    logger: syslog::Logger<syslog::LoggerBackend, syslog::Formatter3164>,
}

impl SyslogWriter {
    fn new() -> Result<Self> {
        let formatter = syslog::Formatter3164 {
            facility: syslog::Facility::LOG_DAEMON,
            hostname: None,
            process: "tt_riingd".into(),
            pid: std::process::id(),
        };

        let logger =
            syslog::unix(formatter).map_err(|e| anyhow!("Failed to connect to syslog: {}", e))?;

        Ok(SyslogWriter { logger })
    }
}

impl Write for SyslogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let msg = String::from_utf8_lossy(buf);
        let msg = msg.trim_end(); // Remove trailing newline

        // Parse log level from tracing format and map to syslog methods
        let result = if msg.contains("ERROR") {
            self.logger.err(msg)
        } else if msg.contains("WARN") {
            self.logger.warning(msg)
        } else if msg.contains("INFO") {
            self.logger.info(msg)
        } else if msg.contains("DEBUG") {
            self.logger.debug(msg)
        } else {
            self.logger.info(msg)
        };

        result.map_err(|e| std::io::Error::other(e.to_string()))?;

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Syslog doesn't need explicit flushing
        Ok(())
    }
}

/// Initialize tracing with production-ready configuration following daemon best practices.
///
/// This implementation follows the tracing ecosystem best practices:
/// - Uses non-blocking appenders for performance (critical for daemons)
/// - Proper syslog integration for system daemons
/// - Environment-based configuration for flexibility
/// - Structured logging support
///
/// Environment variables:
/// - RUST_LOG: Controls log levels (e.g., "tt_riingd=info,tt_riingd::coordinator=debug")
/// - TT_RIINGD_LOG_FORMAT: "json", "pretty", or "compact" (default: "compact")
/// - TT_RIINGD_LOG_TARGET: "stdout", "syslog", or "file" (default: "syslog" for daemon)
/// - TT_RIINGD_LOG_DIR: Directory for file logging (default: "/var/log")
///
/// Returns a WorkerGuard that must be kept alive to ensure log flushing
fn init_tracing(is_daemon: bool) -> Result<Option<WorkerGuard>> {
    let log_format =
        std::env::var("TT_RIINGD_LOG_FORMAT").unwrap_or_else(|_| "compact".to_string());
    let log_target = std::env::var("TT_RIINGD_LOG_TARGET").unwrap_or_else(|_| {
        if is_daemon {
            "syslog".to_string()
        } else {
            "stdout".to_string()
        }
    });

    // Create environment filter with sensible defaults for production
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("tt_riingd=info,warn"))
        .map_err(|e| anyhow!("Invalid RUST_LOG filter: {}", e))?;

    let registry = tracing_subscriber::registry().with(env_filter);

    match log_target.as_str() {
        "syslog" => {
            // Syslog is the standard for daemons - use non-blocking for performance
            let syslog_writer = SyslogWriter::new()?;
            let (non_blocking, guard) = tracing_appender::non_blocking(syslog_writer);

            registry
                .with(
                    fmt::layer()
                        .compact()
                        .with_ansi(false) // No ANSI colors for syslog
                        .with_target(false) // Syslog handles facility/target
                        .with_thread_ids(false) // Not needed for syslog
                        .with_writer(non_blocking),
                )
                .init();

            Ok(Some(guard))
        }
        "file" => {
            let log_dir =
                std::env::var("TT_RIINGD_LOG_DIR").unwrap_or_else(|_| "/var/log".to_string());
            let file_appender = tracing_appender::rolling::daily(&log_dir, "tt_riingd.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            match log_format.as_str() {
                "json" => {
                    registry
                        .with(
                            fmt::layer()
                                .json()
                                .with_ansi(false)
                                .with_writer(non_blocking),
                        )
                        .init();
                }
                _ => {
                    registry
                        .with(
                            fmt::layer()
                                .compact()
                                .with_ansi(false)
                                .with_writer(non_blocking),
                        )
                        .init();
                }
            }

            Ok(Some(guard))
        }
        "stdout" => {
            let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());

            match log_format.as_str() {
                "json" => {
                    registry
                        .with(
                            fmt::layer()
                                .json()
                                .with_target(true)
                                .with_thread_ids(true)
                                .with_writer(non_blocking),
                        )
                        .init();
                }
                "pretty" => {
                    registry
                        .with(
                            fmt::layer()
                                .pretty()
                                .with_target(true)
                                .with_thread_ids(true)
                                .with_writer(non_blocking),
                        )
                        .init();
                }
                _ => {
                    registry
                        .with(
                            fmt::layer()
                                .compact()
                                .with_target(true)
                                .with_writer(non_blocking),
                        )
                        .init();
                }
            }

            Ok(Some(guard))
        }
        _ => Err(anyhow!(
            "Invalid log target: {}. Supported: stdout, syslog, file",
            log_target
        )),
    }
}

fn into_daemon(daemonize: bool) -> Result<()> {
    daemonize
        .then(|| {
            File::create("/var/tmp/tt_riingd.log")
                .and_then(|out| Ok((out.try_clone()?, out)))
                .map_err(|e| anyhow!("{e}"))
                .and_then(|(stderr, stdout)| {
                    Daemonize::new()
                        .pid_file("/tmp/tt_riingd.pid")
                        .stdout(stdout)
                        .stderr(stderr)
                        .start()
                        .map_err(|e| anyhow!("{e}"))
                })
        })
        .map_or(Ok(()), |res| res)
}

#[tokio::main]
async fn tokio_main(config_path: Option<PathBuf>) -> Result<()> {
    #[cfg(feature = "tokio-console")]
    {
        console_subscriber::init();
    }
    let config_manager = config::ConfigManager::load(config_path).await?;
    Application::builder()
        .with_config_manager(config_manager)
        .build()
        .await?
        .run()
        .await?;

    Ok(())
}

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    // Initialize daemon mode first
    into_daemon(cli.daemonize)?;

    // Initialize tracing and keep the guard alive
    let _guard = init_tracing(cli.daemonize)?;

    // Log successful initialization
    tracing::info!("tt_riingd daemon starting up");
    if cli.daemonize {
        tracing::info!("Running in daemon mode with syslog logging");
    }

    // Run the main application
    let result = tokio_main(cli.config);

    // Log shutdown
    match &result {
        Ok(_) => tracing::info!("tt_riingd daemon shutting down normally"),
        Err(e) => tracing::error!("tt_riingd daemon shutting down due to error: {}", e),
    }

    // Guard will be dropped here, ensuring logs are flushed
    result
}
