//! Configuration file location resolution.
//!
//! Handles finding configuration files in standard system locations
//! following XDG specification and common Unix practices.

use anyhow::Result;
use std::{
    env,
    path::{Path, PathBuf},
};

/// Locates the configuration file in standard system locations.
///
/// Searches for configuration files in the following priority order:
/// 1. TT_RIINGD_CONFIG environment variable
/// 2. XDG_CONFIG_HOME/tt_riingd/config.yml or ~/.config/tt_riingd/config.yml  
/// 3. /etc/tt_riingd/config.yml
///
/// # Returns
///
/// PathBuf to the configuration file if found, or an error if no
/// configuration file exists in any standard location.
///
/// # Examples
///
/// ```no_run
/// use tt_riingd::config::location::locate_config;
///
/// # fn example() -> anyhow::Result<()> {
/// let config_path = locate_config()?;
/// println!("Found config at: {}", config_path.display());
/// # Ok(())
/// # }
/// ```
pub fn locate_config() -> Result<PathBuf> {
    // 1) Environment variable override
    if let Ok(env_path) = env::var("TT_RIINGD_CONFIG") {
        return Ok(PathBuf::from(env_path));
    }

    // 2) XDG_CONFIG_HOME or $HOME/.config
    if let Some(mut cfg_dir) = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
    {
        cfg_dir.push("tt_riingd/config.yml");
        if cfg_dir.exists() {
            return Ok(cfg_dir);
        }
    }

    // 3) System-wide configuration
    let etc = Path::new("/etc/tt_riingd/config.yml");
    if etc.exists() {
        return Ok(etc.to_path_buf());
    }

    anyhow::bail!("Configuration file not found in any standard location")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    // Use a mutex to prevent tests from running in parallel and interfering with each other
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_locate_config_env_var() {
        let _guard = TEST_MUTEX.lock().unwrap();

        let test_path = "/tmp/test_config.yml";
        unsafe {
            env::set_var("TT_RIINGD_CONFIG", test_path);
        }

        let result = locate_config();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str().unwrap(), test_path);

        unsafe {
            env::remove_var("TT_RIINGD_CONFIG");
        }
    }

    #[test]
    fn test_locate_config_nonexistent_env_path() {
        let _guard = TEST_MUTEX.lock().unwrap();

        let test_path = "/nonexistent/path/config.yml";
        unsafe {
            env::set_var("TT_RIINGD_CONFIG", test_path);
        }

        // Should succeed with the env var path (even if file doesn't exist)
        let result = locate_config();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str().unwrap(), test_path);

        unsafe {
            env::remove_var("TT_RIINGD_CONFIG");
        }
    }
}
