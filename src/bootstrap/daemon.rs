use anyhow::{Result, anyhow};
use daemonize::Daemonize;
use std::fs::File;

/// Configure and initialize daemon mode if requested
///
/// This function handles the transition to daemon mode, including:
/// - PID file creation
/// - stdout/stderr redirection
/// - Process forking
pub fn init_daemon(daemonize: bool) -> Result<()> {
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
