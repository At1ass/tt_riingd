//! Configuration file management and operations.
//!
//! Provides high-level interface for loading, reloading, and managing
//! configuration files with automatic location resolution and validation.

use crate::{
    config::{
        location::locate_config,
        structures::{
            Config, CurveCfg, CurveMappingCfg, EffectCfg, EffectMappingCfg, FanTarget, MappingCfg,
        },
    },
    core::event::ConfigChangeType,
    drivers::registry::Registry,
};
use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{fs, sync::RwLock};
use tracing::{debug, info};

/// Configuration manager for loading, reloading, and managing configuration files.
///
/// Provides unified interface for configuration operations with automatic location
/// resolution and validation.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    path: PathBuf,
}

impl ConfigManager {
    /// Creates a new ConfigManager with the given config and path.
    ///
    /// This is primarily used for testing purposes.
    #[cfg(test)]
    pub fn new(config: Config, path: PathBuf) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            path,
        }
    }

    /// Loads configuration from file or standard locations.
    ///
    /// Searches for configuration in the following order:
    /// 1. Provided path parameter
    /// 2. TT_RIINGD_CONFIG environment variable
    /// 3. XDG_CONFIG_HOME/tt_riingd/config.yml or ~/.config/tt_riingd/config.yml
    /// 4. /etc/tt_riingd/config.yml
    pub async fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = match path {
            Some(p) => p,
            None => locate_config().context("No configuration file found")?,
        };

        info!("Loading config from: {}", config_path.display());
        let config = Self::load_config_from_path(&config_path).await?;

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            path: config_path,
        })
    }

    /// Gets a read-only reference to the current configuration.
    pub async fn get(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
        self.config.read().await
    }

    /// Returns the path to the configuration file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reloads configuration from the same file.
    ///
    /// This is useful for hot-reloading configuration changes.
    pub async fn reload(&self) -> Result<()> {
        info!("Reloading config from: {}", self.path.display());
        let new_config = Self::load_config_from_path(&self.path).await?;

        *self.config.write().await = new_config;
        info!("Configuration reloaded successfully");
        Ok(())
    }

    /// Analyzes configuration changes and returns the type of reload required.
    ///
    /// Compares the current configuration with a new one from file
    /// to determine if hot-reload is possible or restart is required.
    pub async fn analyze_config_changes(&self) -> Result<ConfigChangeType> {
        let current_config = self.config.read().await;
        let new_config = Self::load_config_from_path(&self.path).await?;

        Ok(current_config.analyze_changes(&new_config))
    }

    /// Ensures that fallback controllers have appropriate mappings.
    ///
    /// Creates default sensor mappings, curve mappings, and effect mappings
    /// for any controllers that start with "fallback" prefix.
    pub async fn ensure_mappings(config: &mut Config) -> Result<()> {
        // Ensure all mappings have valid targets
        let (m, cm, em) = config
            .controllers
            .iter_mut()
            .inspect(|c| {
                debug!("Processing controller: {}", c.get_id());
            })
            .filter(|controller| controller.get_id().starts_with("fallback"))
            .fold(
                (
                    MappingCfg::dummy(),
                    CurveMappingCfg::dummy(),
                    EffectMappingCfg::dummy(),
                ),
                |(mut m, mut cm, mut em), controller| {
                    for idx in 1..=controller.get_fans_count() {
                        let fan_target = FanTarget {
                            controller_id: controller.get_id(),
                            fan_idx: idx,
                        };
                        m.targets.push(fan_target.clone());
                        cm.targets.push(fan_target.clone());
                        em.targets.push(fan_target);
                    }
                    (m, cm, em)
                },
            );

        config.mappings.push(m);

        config.curves.push(CurveCfg::Constant {
            id: "dummy_curve".to_string(),
            speed: 80,
        });
        config.active_curve_mappings.push(cm);

        config.effects.push(EffectCfg::ConstantColor {
            id: "dummy_effect".to_string(),
            rgb: [255, 0, 0],
        });
        config.effect_mappings.push(em);

        Ok(())
    }

    /// Clones the current configuration.
    ///
    /// Useful when you need to work with a snapshot of the config.
    pub async fn clone_config(&self) -> Config {
        self.config.read().await.clone()
    }

    /// Loads configuration from a specific path (internal helper).
    async fn load_config_from_path(path: &Path) -> Result<Config> {
        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let mut config: Config = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML in: {}", path.display()))?;

        if config.version != 1 {
            anyhow::bail!(
                "Unsupported config version {} in file: {}",
                config.version,
                path.display()
            );
        }

        config
            .validate()
            .with_context(|| format!("Configuration validation failed for: {}", path.display()))?;

        Registry::merge_with_config(&mut config);

        Self::ensure_mappings(&mut config).await?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_config_manager_basic_operations() {
        let config = Config::default();
        let temp_path = PathBuf::from("/tmp/test_config.yml");
        let manager = ConfigManager::new(config, temp_path);

        // Test basic getters
        let config_ref = manager.get().await;
        assert_eq!(config_ref.version, 1);
        assert_eq!(config_ref.tick_seconds, 2);
        drop(config_ref);

        // Test cloning
        let cloned = manager.clone_config().await;
        assert_eq!(cloned.version, 1);
    }

    #[tokio::test]
    async fn test_load_config_from_path() {
        let temp_file = NamedTempFile::new().unwrap();
        let yaml_content = r#"
version: 1
tick_seconds: 5
enable_broadcast: true
controllers: []
curves: []
sensors: []
mappings: []
active_curve_mappings: []
effects: []
effect_mappings: []
"#;

        tokio::fs::write(temp_file.path(), yaml_content)
            .await
            .unwrap();

        let config = ConfigManager::load_config_from_path(temp_file.path())
            .await
            .unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.tick_seconds, 5);
        assert!(config.enable_broadcast);
    }

    #[tokio::test]
    async fn test_invalid_config_version() {
        let temp_file = NamedTempFile::new().unwrap();
        let yaml_content = r#"
version: 2
tick_seconds: 5
"#;

        tokio::fs::write(temp_file.path(), yaml_content)
            .await
            .unwrap();

        let result = ConfigManager::load_config_from_path(temp_file.path()).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported config version")
        );
    }
}
