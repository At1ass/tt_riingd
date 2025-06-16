# tt-riingd

[![CI](https://github.com/At1ass/tt_riingd/actions/workflows/ci.yml/badge.svg)](https://github.com/At1ass/tt_riingd/actions/workflows/ci.yml)  [![License](https://img.shields.io/badge/license-MIT-green.svg)](#license)

A high-performance, asynchronous Rust daemon for controlling Thermaltake Riing fans on Linux. It provides comprehensive fan speed control, RGB lighting management, and temperature monitoring through a modern, modular architecture.

> **Status:** Active development - Core functionality stable, advanced features in progress.

## Table of Contents

- [Features](#features)
- [System Requirements](#system-requirements)
- [Installation](#installation)
- [Configuration](#configuration)
- [Usage](#usage)
- [Architecture](#architecture)
- [Development](#development)
- [Performance](#performance)
- [Contributing](#contributing)
- [Roadmap](#roadmap)
- [Troubleshooting](#troubleshooting)
- [License](#license)

## Features

### Core Functionality
* **Asynchronous Architecture** - Built with [Tokio](https://tokio.rs/) for high-performance, non-blocking operations
* **HID Driver Support** - Native support for Thermaltake Riing controllers (PID 0x232B–0x232E)
* **Temperature Monitoring** - Integrated lm-sensors and NVIDIA GPU support with configurable polling
* **Advanced Fan Curves** - Support for constant, step-based, and smooth Bézier curves
* **RGB Control** - Full RGB lighting control with temperature-based color mapping
* **Hot Configuration Reload** - Dynamic configuration updates without daemon restart

### Integration & APIs
* **D-Bus Interface** - Complete D-Bus API for external integration:
  - Bus name: `io.github.tt_riingd`
  - Object path: `/io/github/tt_riingd`
  - Interface: `io.github.tt_riingd1`
  - Methods: `GetActiveCurve`, `SwitchActiveCurve`, `UpdateCurveData`, `Stop`
  - Properties: `Version`
  - Signals: `Stopped`, `TemperatureChanged`

* **YAML Configuration** - Human-readable configuration with validation
* **CLI Utility** - `riingctl` for quick D-Bus operations
* **Security** - Udev rules for non-root access, systemd hardening

### Architecture Highlights
* **Modular Service Architecture** - Plugin-based service providers
* **Event-Driven Design** - Async event bus for inter-service communication
* **Comprehensive Testing** - 162+ unit tests with edge case coverage
* **Performance Monitoring** - Built-in metrics and health checks
* **Zero Runtime Dependencies** - Minimal dependency footprint

## System Requirements

- **OS**: Linux (kernel 3.0+)
- **Hardware**: Thermaltake Riing controllers
- **Rust**: 1.70+ (for building from source)
- **Dependencies**: `libudev-dev`, `libhidapi-dev`
- **Optional**: NVIDIA drivers (for GPU temperature monitoring)

## Installation

### From Source (Recommended)

```bash
# Install system dependencies
sudo apt update && sudo apt install libudev-dev libhidapi-dev  # Debian/Ubuntu
sudo dnf install systemd-devel hidapi-devel                    # Fedora
sudo pacman -S systemd hidapi                                  # Arch Linux

# Clone and build
git clone https://github.com/At1ass/tt_riingd.git
cd tt_riingd
cargo build --release

# Install binary
sudo install -Dm755 target/release/tt-riingd /usr/local/bin/tt-riingd
sudo install -Dm755 riingctl /usr/local/bin/riingctl
```

### Package Managers (Coming Soon)
- AUR package
- Debian/Ubuntu packages
- Fedora RPM packages

## Configuration

### Udev Rules (Required)

Enable non-root access to Thermaltake devices:

```bash
sudo tee /etc/udev/rules.d/99-tt-riingd.rules << 'EOF'
# Thermaltake Riing controllers: PID 0x232B–0x232E
SUBSYSTEM=="hidraw", SUBSYSTEMS=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="232?", TAG+="uaccess", TAG+="Thermaltake_Riing"
EOF

sudo udevadm control --reload
sudo udevadm trigger
```

### Configuration File

Create `~/.config/tt-riingd/config.yml`:

```yaml
# Global settings
version: 1
tick_seconds: 2              # Temperature polling interval
enable_broadcast: true       # Enable temperature broadcasts
broadcast_interval: 5        # Broadcast interval (seconds)

# Hardware controllers
controllers:
  - kind: riing-quad
    id: "main_controller"
    usb:
      vid: 0x264a
      pid: 0x2330
      # serial: "ABC123"       # Optional: specific device serial
    fans:
      - idx: 1
        name: "CPU Intake"
      - idx: 2
        name: "CPU Exhaust"

# Temperature sensors
sensors:
  - kind: lm-sensors
    id: "cpu_temp"
    chip: "k10temp-pci-00c3"
    feature: "Tctl"
  
  # NVIDIA GPU temperature monitoring (optional, requires NVIDIA drivers)
  - kind: nvidia
    id: "gpu_temp"
    gpu_index: 0          # First GPU (0-based indexing)

# Fan speed curves
curves:
  - kind: constant
    id: "silent"
    speed: 30

  - kind: step-curve
    id: "performance"
    tmps: [30.0, 50.0, 70.0, 85.0]
    spds: [25, 40, 70, 100]

  - kind: bezier
    id: "smooth"
    points:
      - {x: 30.0, y: 20.0}    # Control point 1
      - {x: 45.0, y: 30.0}    # Control point 2
      - {x: 65.0, y: 70.0}    # Control point 3
      - {x: 80.0, y: 95.0}    # Control point 4

# Sensor-to-fan mappings
mappings:
  - sensor: "cpu_temp"
    targets:
      - controller: 1
        fan_idx: 1
      - controller: 1
        fan_idx: 2

# Active curve assignments
active_curve_mappings:
  - curve: "performance"
    targets:
      - controller: 1
        fan_idx: 1

# RGB color definitions
colors:
  - color: "cool_blue"
    rgb: [0, 100, 255]
  - color: "warm_red"
    rgb: [255, 50, 0]

# Temperature-based color mappings
color_mappings:
  - color: "cool_blue"
    targets:
      - controller: 1
        fan_idx: 1
```

### Environment Variables

```bash
export TT_RIINGD_CONFIG=/path/to/config.yml    # Custom config location
export TT_RIINGD_LOG_LEVEL=debug               # Logging level
export RUST_LOG=tt_riingd=debug                # Rust logging
```

## Usage

### Running the Daemon

#### Systemd User Service (Recommended)

```bash
# Create service file
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/tt-riingd.service << 'EOF'
[Unit]
Description=tt-riingd — Thermaltake Riing Fan Controller
Documentation=https://github.com/At1ass/tt_riingd
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/local/bin/tt-riingd
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal

# Security hardening
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
NoNewPrivileges=true
MemoryDenyWriteExecute=true

[Install]
WantedBy=default.target
EOF

# Enable and start
systemctl --user daemon-reload
systemctl --user enable --now tt-riingd

# Monitor logs
journalctl --user -u tt-riingd -f
```

#### System Service

```bash
sudo tee /etc/systemd/system/tt-riingd.service << 'EOF'
[Unit]
Description=tt-riingd — Thermaltake Riing Fan Controller
Documentation=https://github.com/At1ass/tt_riingd
After=multi-user.target

[Service]
Type=simple
ExecStart=/usr/local/bin/tt-riingd --config /etc/tt-riingd/config.yml
User=tt-riingd
Group=tt-riingd
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal

# Security hardening
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
NoNewPrivileges=true
MemoryDenyWriteExecute=true
SystemCallFilter=@system-service
SystemCallErrorNumber=EPERM

[Install]
WantedBy=multi-user.target
EOF

# Create user and enable service
sudo useradd -r -s /bin/false tt-riingd
sudo systemctl daemon-reload
sudo systemctl enable --now tt-riingd
```

#### Manual Execution

```bash
# Foreground with debug logging
RUST_LOG=debug tt-riingd --config config.yml

# Background daemon
tt-riingd --config config.yml --daemon
```

### CLI Tool: `riingctl`

```bash
# Get daemon version
riingctl version

# Check active curve for controller 1, fan 1
riingctl get-active-curve 1 1

# Switch to performance curve
riingctl switch-active-curve 1 1 performance

# Update curve data (JSON format)
riingctl update-curve-data 1 1 custom '{"kind":"constant","speed":75}'

# Stop daemon gracefully
riingctl stop
```

### D-Bus Integration

```bash
# Introspect the interface
busctl --user introspect io.github.tt_riingd /io/github/tt_riingd

# Call methods directly
busctl --user call io.github.tt_riingd /io/github/tt_riingd io.github.tt_riingd1 GetActiveCurve yy 1 1

# Monitor signals
busctl --user monitor io.github.tt_riingd
```

### Configuration Hot Reload

```bash
# Send SIGHUP to reload configuration
sudo systemctl reload tt-riingd

# Or via D-Bus
busctl --user call io.github.tt_riingd /io/github/tt_riingd io.github.tt_riingd1 ReloadConfig
```

## Architecture

### Service Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                        │
├─────────────────────────────────────────────────────────────┤
│  SystemCoordinator │ TaskManager │ EventBus │ ConfigManager │
├─────────────────────────────────────────────────────────────┤
│                    Service Providers                        │
│  • MonitoringService    • BroadcastService                  │
│  • FanColorService      • DBusService                       │
│  • ConfigWatcherService                                     │
├─────────────────────────────────────────────────────────────┤
│                    Hardware Abstraction                     │
│  • FanController       • TemperatureSensors                 │
│  • TTRiingQuad Driver   • LmSensors Integration             │
│  • NVIDIA GPU Support                                       │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

- **SystemCoordinator**: Orchestrates service lifecycle and dependencies
- **TaskManager**: Manages async tasks with graceful shutdown
- **EventBus**: Pub/sub system for inter-service communication
- **ConfigManager**: Hot-reloadable configuration with validation
- **Service Providers**: Modular, pluggable service architecture

## Development

### Building

```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Build with all features
cargo build --all-features
```

### Testing

```bash
# Run all tests
cargo test

# Run with coverage
cargo test --all-features
cargo tarpaulin --out html

# Run specific test module
cargo test config::tests

# Run integration tests
cargo test --test integration
```

### Code Quality

```bash
# Format code
cargo fmt --all

# Lint with clippy
cargo clippy --all-targets --all-features -- -D warnings

# Security audit
cargo audit

# Check documentation
cargo doc --no-deps --open
```

### Debugging

```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Run with specific module logging
RUST_LOG=tt_riingd::drivers=trace cargo run

# Use debugger
rust-gdb target/debug/tt-riingd
```

## Performance

### Benchmarks

- **Memory Usage**: ~2-5MB RSS
- **CPU Usage**: <1% on modern systems
- **Response Time**: <1ms for D-Bus calls
- **Startup Time**: <100ms cold start

### Monitoring

```bash
# System resource usage
systemctl --user status tt-riingd

# Detailed metrics
journalctl --user -u tt-riingd | grep "METRICS"

# D-Bus performance
busctl --user monitor io.github.tt_riingd
```

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Quick Start

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes with tests
4. Run quality checks: `cargo fmt && cargo clippy && cargo test`
5. Commit with conventional commits: `git commit -m "feat: add amazing feature"`
6. Push and create a Pull Request

### Development Setup

```bash
# Install development dependencies
cargo install cargo-tarpaulin cargo-audit cargo-outdated

# Set up pre-commit hooks
cp scripts/pre-commit .git/hooks/
chmod +x .git/hooks/pre-commit
```

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features and milestones.

### Upcoming Features

- [ ] **GUI Application** - GTK4/Libadwaita interface
- [ ] **Plugin System** - Extensible architecture for custom sensors/controllers
- [ ] **Advanced Curves** - PID controllers, machine learning optimization
- [ ] **Packaging** - Distribution packages for major Linux distros
- [ ] **Documentation** - Comprehensive user and developer guides

## Troubleshooting

### Common Issues

**Permission Denied**
```bash
# Check udev rules are installed
ls -la /etc/udev/rules.d/99-tt-riingd.rules

# Verify device permissions
ls -la /dev/hidraw*

# Reload udev rules
sudo udevadm control --reload && sudo udevadm trigger
```

**Configuration Errors**
```bash
# Validate configuration
tt-riingd --config config.yml --validate

# Check logs for details
journalctl --user -u tt-riingd -n 50
```

**D-Bus Connection Issues**
```bash
# Check if daemon is running
systemctl --user status tt-riingd

# Test D-Bus connectivity
busctl --user list | grep tt_riingd
```

### Getting Help

- [Documentation](https://github.com/At1ass/tt_riingd/wiki)
- [Issue Tracker](https://github.com/At1ass/tt_riingd/issues)
- [Discussions](https://github.com/At1ass/tt_riingd/discussions)

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Acknowledgments

- Thermaltake for hardware documentation
- The Rust community for excellent crates
- Linux kernel developers for HID subsystem
- Contributors and testers

---

**Made with care by [At1ass](https://github.com/At1ass) and [contributors](https://github.com/At1ass/tt_riingd/graphs/contributors)**
