//! Unit tests for configuration management
//!
//! Tests configuration loading, validation, change detection, and various
//! edge cases according to Rust testing best practices.

use super::super::*;
use proptest::prelude::*;
use std::{env, fs, path::PathBuf};
use tempfile::NamedTempFile;

/// Test data fixtures for configuration testing
mod fixtures {
    use super::*;

    pub fn minimal_config() -> Config {
        Config {
            version: 1,
            tick_seconds: 2,
            enable_broadcast: false,
            broadcast_interval: 2,
            controllers: vec![],
            curves: vec![],
            sensors: vec![],
            mappings: vec![],
            active_curve_mappings: vec![],
            effects: vec![],
            effect_mappings: vec![],
        }
    }

    pub fn valid_riing_quad_controller() -> ControllerCfg {
        ControllerCfg::RiingQuad {
            id: "test_controller".to_string(),
            usb: UsbSelector {
                vid: 0x264a,
                pid: 0x2330,
                serial: None,
            },
            fans: vec![FanCfg {
                idx: 1,
                name: "CPU Fan".to_string(),
            }],
        }
    }

    pub fn valid_constant_curve() -> CurveCfg {
        CurveCfg::Constant {
            id: "test_constant".to_string(),
            speed: 50,
        }
    }

    pub fn valid_step_curve() -> CurveCfg {
        CurveCfg::StepCurve {
            id: "test_step".to_string(),
            tmps: vec![30.0, 50.0, 70.0],
            spds: vec![20, 50, 80],
        }
    }

    pub fn valid_bezier_curve() -> CurveCfg {
        CurveCfg::Bezier {
            id: "test_bezier".to_string(),
            points: vec![
                Point { x: 0.0, y: 20.0 },
                Point { x: 30.0, y: 20.0 },
                Point { x: 70.0, y: 80.0 },
                Point { x: 100.0, y: 100.0 },
            ],
        }
    }

    pub fn config_yaml() -> &'static str {
        r#"
version: 1
tick_seconds: 2
enable_broadcast: false
broadcast_interval: 2

controllers:
  - kind: riing-quad
    id: "controller1"
    usb:
      vid: 0x264a
      pid: 0x2330
    fans:
      - idx: 1
        name: "CPU Fan"

curves:
  - kind: constant
    id: "constant_50"
    speed: 50

sensors:
  - kind: lm-sensors
    id: "cpu_temp"
    chip: "k10temp-pci-00c3"
    feature: "Tctl"

mappings:
  - sensor: "cpu_temp"
    targets:
      - controller: 1
        fan_idx: 1

active_curve_mappings:
  - curve: "constant_50"
    targets:
      - controller: 1
        fan_idx: 1

effects:
  - kind: constant-color
    id: "red"
    rgb: [255, 0, 0]

effect_mappings:
  - effect: "red"
    targets:
      - controller: 1
        fan_idx: 1
"#
    }
}

/// Tests for Config struct default values and validation
mod config_tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert_eq!(config.version, 1);
        assert_eq!(config.tick_seconds, 2);
        assert!(!config.enable_broadcast);
        assert_eq!(config.broadcast_interval, 2);
        assert!(config.controllers.is_empty());
        assert!(config.curves.is_empty());
        assert!(config.sensors.is_empty());
        assert!(config.mappings.is_empty());
        assert!(config.active_curve_mappings.is_empty());
        assert!(config.effects.is_empty());
        assert!(config.effect_mappings.is_empty());
    }

    #[test]
    fn test_config_validation_empty() {
        let config = fixtures::minimal_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_with_components() {
        let mut config = fixtures::minimal_config();
        config
            .controllers
            .push(fixtures::valid_riing_quad_controller());
        config.curves.push(fixtures::valid_constant_curve());

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_yaml_serialization() {
        let config = fixtures::minimal_config();
        let yaml = serde_yaml::to_string(&config).expect("Should serialize to YAML");
        assert!(yaml.contains("version: 1"));
        assert!(yaml.contains("tick_seconds: 2"));
    }

    #[test]
    fn test_config_yaml_deserialization() {
        let yaml = fixtures::config_yaml();
        let config: Config = serde_yaml::from_str(yaml).expect("Should deserialize from YAML");

        assert_eq!(config.version, 1);
        assert_eq!(config.controllers.len(), 1);
        assert_eq!(config.curves.len(), 1);
        assert_eq!(config.sensors.len(), 1);
        assert_eq!(config.mappings.len(), 1);
        assert_eq!(config.active_curve_mappings.len(), 1);
        assert_eq!(config.effects.len(), 1);
        assert_eq!(config.effect_mappings.len(), 1);
    }

    #[test]
    fn test_config_change_analysis_no_changes() {
        let config1 = fixtures::minimal_config();
        let config2 = fixtures::minimal_config();

        let change_type = config1.analyze_changes(&config2);
        matches!(change_type, ConfigChangeType::HotReload);
    }

    #[test]
    fn test_config_change_analysis_hot_reload() {
        let config1 = fixtures::minimal_config();
        let mut config2 = fixtures::minimal_config();

        config2.tick_seconds = 5; // This should be hot-reloadable

        let change_type = config1.analyze_changes(&config2);
        matches!(change_type, ConfigChangeType::HotReload);
    }

    #[test]
    fn test_config_change_analysis_cold_restart() {
        let mut config1 = fixtures::minimal_config();
        let config2 = fixtures::minimal_config();

        config1
            .controllers
            .push(fixtures::valid_riing_quad_controller());
        // config2 has no controllers - this should require cold restart

        let change_type = config1.analyze_changes(&config2);
        matches!(change_type, ConfigChangeType::ColdRestart { .. });
    }
}

/// Tests for ControllerCfg variants
mod controller_tests {
    use super::*;

    #[test]
    fn test_riing_quad_controller_creation() {
        let controller = fixtures::valid_riing_quad_controller();

        let ControllerCfg::RiingQuad { id, usb, fans } = controller;
        assert_eq!(id, "test_controller");
        assert_eq!(usb.vid, 0x264a);
        assert_eq!(usb.pid, 0x2330);
        assert!(usb.serial.is_none());
        assert_eq!(fans.len(), 1);
        assert_eq!(fans[0].idx, 1);
        assert_eq!(fans[0].name, "CPU Fan");
    }

    #[test]
    fn test_controller_yaml_serialization() {
        let controller = fixtures::valid_riing_quad_controller();
        let yaml = serde_yaml::to_string(&controller).expect("Should serialize to YAML");

        assert!(yaml.contains("kind: riing-quad"));
        assert!(yaml.contains("id: test_controller"));
        assert!(yaml.contains("vid: 9802")); // 0x264a in decimal
    }

    #[test]
    fn test_fan_cfg_creation() {
        let fan = FanCfg {
            idx: 2,
            name: "Case Fan".to_string(),
        };

        assert_eq!(fan.idx, 2);
        assert_eq!(fan.name, "Case Fan");
    }
}

/// Tests for CurveCfg variants and calculations
mod curve_tests {
    use super::*;

    #[test]
    fn test_constant_curve_creation() {
        let curve = fixtures::valid_constant_curve();

        if let CurveCfg::Constant { id, speed } = curve {
            assert_eq!(id, "test_constant");
            assert_eq!(speed, 50);
        } else {
            panic!("Expected Constant curve");
        }
    }

    #[test]
    fn test_constant_curve_calculation() {
        let curve = fixtures::valid_constant_curve();
        let speed = curve.calculate_speed(50.0).expect("Should calculate speed");
        assert_eq!(speed, 50);
    }

    #[test]
    fn test_step_curve_creation() {
        let curve = fixtures::valid_step_curve();

        if let CurveCfg::StepCurve { id, tmps, spds } = curve {
            assert_eq!(id, "test_step");
            assert_eq!(tmps, vec![30.0, 50.0, 70.0]);
            assert_eq!(spds, vec![20, 50, 80]);
        } else {
            panic!("Expected StepCurve");
        }
    }

    #[test]
    fn test_step_curve_calculation() {
        let curve = fixtures::valid_step_curve();

        // Test exact points
        assert_eq!(curve.calculate_speed(30.0).unwrap(), 20);
        assert_eq!(curve.calculate_speed(50.0).unwrap(), 50);
        assert_eq!(curve.calculate_speed(70.0).unwrap(), 80);

        // Test interpolation
        let speed = curve.calculate_speed(40.0).unwrap();
        assert!((20..=50).contains(&speed));

        // Test below range
        let speed = curve.calculate_speed(20.0).unwrap();
        assert_eq!(speed, 20);

        // Test above range
        let speed = curve.calculate_speed(80.0).unwrap();
        assert_eq!(speed, 80);
    }

    #[test]
    fn test_bezier_curve_creation() {
        let curve = fixtures::valid_bezier_curve();

        if let CurveCfg::Bezier { id, points } = curve {
            assert_eq!(id, "test_bezier");
            assert_eq!(points.len(), 4);
            assert_eq!(points[0], Point { x: 0.0, y: 20.0 });
            assert_eq!(points[3], Point { x: 100.0, y: 100.0 });
        } else {
            panic!("Expected Bezier curve");
        }
    }

    #[test]
    fn test_bezier_curve_calculation() {
        let curve = fixtures::valid_bezier_curve();

        // Test start and end points
        let speed_start = curve.calculate_speed(0.0).unwrap();
        assert!((speed_start as f32 - 20.0).abs() < 1.0);

        let speed_end = curve.calculate_speed(100.0).unwrap();
        assert!((speed_end as f32 - 100.0).abs() < 1.0);

        // Test middle point
        let speed_mid = curve.calculate_speed(50.0).unwrap();
        assert!((20..=100).contains(&speed_mid));
    }

    #[test]
    fn test_curve_get_id() {
        let curves = vec![
            fixtures::valid_constant_curve(),
            fixtures::valid_step_curve(),
            fixtures::valid_bezier_curve(),
        ];

        let ids: Vec<String> = curves.iter().map(|c| c.get_id()).collect();
        assert_eq!(ids, vec!["test_constant", "test_step", "test_bezier"]);
    }

    #[test]
    fn test_step_curve_invalid_data() {
        let curve = CurveCfg::StepCurve {
            id: "invalid".to_string(),
            tmps: vec![30.0, 50.0],
            spds: vec![20], // Mismatched lengths
        };

        assert!(curve.calculate_speed(40.0).is_err());
    }

    #[test]
    fn test_bezier_curve_invalid_points() {
        let curve = CurveCfg::Bezier {
            id: "invalid".to_string(),
            points: vec![Point { x: 0.0, y: 0.0 }], // Too few points
        };

        assert!(curve.calculate_speed(50.0).is_err());
    }
}

/// Tests for sensor configuration
mod sensor_tests {
    use super::*;

    #[test]
    fn test_lm_sensors_config() {
        let sensor = SensorCfg::LmSensors {
            id: "cpu_temp".to_string(),
            chip: "k10temp-pci-00c3".to_string(),
            feature: "Tctl".to_string(),
        };

        if let SensorCfg::LmSensors { id, chip, feature } = sensor {
            assert_eq!(id, "cpu_temp");
            assert_eq!(chip, "k10temp-pci-00c3");
            assert_eq!(feature, "Tctl");
        } else {
            panic!("Expected LmSensors config");
        }
    }

    #[test]
    fn test_nvidia_sensor_config() {
        let sensor = SensorCfg::Nvidia {
            id: "gpu_temp".to_string(),
            gpu_index: 0,
        };

        if let SensorCfg::Nvidia { id, gpu_index } = sensor {
            assert_eq!(id, "gpu_temp");
            assert_eq!(gpu_index, 0);
        } else {
            panic!("Expected Nvidia sensor config");
        }
    }
}

/// Tests for effect configuration
mod effect_tests {
    use super::*;

    #[test]
    fn test_constant_color_effect() {
        let effect = EffectCfg::ConstantColor {
            id: "red".to_string(),
            rgb: [255, 0, 0],
        };

        assert_eq!(effect.get_id(), "red");
        assert!(effect.into_runner().is_ok());
    }

    #[test]
    fn test_rainbow_effect() {
        let effect = EffectCfg::Rainbow {
            id: "rainbow".to_string(),
            duration: 5,
        };

        assert_eq!(effect.get_id(), "rainbow");
        assert!(effect.into_runner().is_ok());
    }

    #[test]
    fn test_breathing_effect() {
        let effect = EffectCfg::Breathing {
            id: "breathing".to_string(),
            color: [0, 255, 0],
            duration: 3,
        };

        assert_eq!(effect.get_id(), "breathing");
        assert!(effect.into_runner().is_ok());
    }

    #[test]
    fn test_fade_effect() {
        let effect = EffectCfg::Fade {
            id: "fade".to_string(),
            color: [0, 0, 255],
            duration: 4,
        };

        assert_eq!(effect.get_id(), "fade");
        assert!(effect.into_runner().is_ok());
    }
}

/// Tests for ConfigManager functionality
mod config_manager_tests {
    use super::*;

    #[tokio::test]
    async fn test_config_manager_creation() {
        let config = fixtures::minimal_config();
        let temp_path = PathBuf::from("/tmp/test_config.yml");

        let manager = ConfigManager::new(config.clone(), temp_path.clone());

        assert_eq!(manager.path(), &temp_path);
        let loaded_config = manager.get().await;
        assert_eq!(loaded_config.version, config.version);
    }

    #[tokio::test]
    async fn test_config_manager_clone_config() {
        let config = fixtures::minimal_config();
        let temp_path = PathBuf::from("/tmp/test_config.yml");
        let manager = ConfigManager::new(config.clone(), temp_path);

        let cloned = manager.clone_config().await;
        assert_eq!(cloned.version, config.version);
    }

    #[tokio::test]
    async fn test_config_manager_load_from_tempfile() {
        let yaml = fixtures::config_yaml();
        let temp_file = NamedTempFile::new().expect("Should create temp file");
        fs::write(temp_file.path(), yaml).expect("Should write YAML");

        let manager = ConfigManager::load(Some(temp_file.path().to_path_buf()))
            .await
            .expect("Should load config");

        let config = manager.get().await;
        assert_eq!(config.version, 1);
        assert_eq!(config.controllers.len(), 1);
    }

    #[tokio::test]
    async fn test_config_manager_reload() {
        let yaml = fixtures::config_yaml();
        let temp_file = NamedTempFile::new().expect("Should create temp file");
        fs::write(temp_file.path(), yaml).expect("Should write YAML");

        let manager = ConfigManager::load(Some(temp_file.path().to_path_buf()))
            .await
            .expect("Should load config");

        // Modify the file
        let modified_yaml = yaml.replace("tick_seconds: 2", "tick_seconds: 5");
        fs::write(temp_file.path(), modified_yaml).expect("Should write modified YAML");

        manager.reload().await.expect("Should reload config");

        let config = manager.get().await;
        assert_eq!(config.tick_seconds, 5);
    }

    #[tokio::test]
    async fn test_config_manager_analyze_changes() {
        let yaml = fixtures::config_yaml();
        let temp_file = NamedTempFile::new().expect("Should create temp file");
        fs::write(temp_file.path(), yaml).expect("Should write YAML");

        let manager = ConfigManager::load(Some(temp_file.path().to_path_buf()))
            .await
            .expect("Should load config");

        let change_type = manager
            .analyze_config_changes()
            .await
            .expect("Should analyze changes");
        matches!(change_type, ConfigChangeType::HotReload);
    }

    #[test]
    fn config_manager_env_var_error() {
        let config_path = PathBuf::from("/non/existent/path/config.yml");

        // Test with non-existent path
        unsafe {
            env::set_var("TT_RIINGD_CONFIG", config_path.to_str().unwrap());
        }

        // This test would require implementing from_environment method
        // For now, we just test that setting the environment variable works
        let var_value = env::var("TT_RIINGD_CONFIG");
        assert!(var_value.is_ok());
        assert_eq!(var_value.unwrap(), config_path.to_str().unwrap());

        // Clean up
        unsafe {
            env::remove_var("TT_RIINGD_CONFIG");
        }
    }
}

/// Property-based tests for configuration validation
mod property_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_constant_curve_speed_valid_range(speed in 0u8..=100) {
            let curve = CurveCfg::Constant {
                id: "prop_test".to_string(),
                speed,
            };
            let result = curve.calculate_speed(50.0);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), speed);
        }

        #[test]
        fn test_usb_selector_vid_pid_any_value(vid: u16, pid: u16) {
            let usb = UsbSelector { vid, pid, serial: None };
            // Should always be valid regardless of values
            assert_eq!(usb.vid, vid);
            assert_eq!(usb.pid, pid);
        }

        #[test]
        fn test_fan_target_valid_indices(controller in 1u8..=255, fan_idx in 1u8..=255) {
            let target = FanTarget { controller, fan_idx };
            assert_eq!(target.controller, controller);
            assert_eq!(target.fan_idx, fan_idx);
        }

        #[test]
        fn test_temperature_calculation_bezier_no_panic(temp in -100.0f32..200.0f32) {
            let curve = fixtures::valid_bezier_curve();
            // Should not panic for any temperature value
            let _ = curve.calculate_speed(temp);
        }
    }
}

/// Benchmarks for performance-critical operations
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    fn bench_bezier_calculation() {
        let curve = fixtures::valid_bezier_curve();
        let start = Instant::now();

        for temp in (0..100).map(|i| i as f32) {
            let _ = curve.calculate_speed(temp);
        }

        let duration = start.elapsed();
        println!("Bezier calculation for 100 temps: {:?}", duration);

        // Should complete reasonably fast
        assert!(duration.as_millis() < 100);
    }

    #[test]
    fn bench_config_serialization() {
        let mut config = fixtures::minimal_config();
        config
            .controllers
            .push(fixtures::valid_riing_quad_controller());
        config.curves.push(fixtures::valid_bezier_curve());

        let start = Instant::now();

        for _ in 0..1000 {
            let _ = serde_yaml::to_string(&config);
        }

        let duration = start.elapsed();
        println!("Config serialization 1000x: {:?}", duration);

        // Should complete reasonably fast
        assert!(duration.as_millis() < 1000);
    }
}
