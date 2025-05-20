use clap::Parser;
use std::path::PathBuf;

/// tt-riingd — daemon for TT Riing Quad fan control
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// YAML config file path (default: /etc/config.yml)
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,
}
