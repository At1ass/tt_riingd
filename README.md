# tt-riingd

[![CI](https://github.com/At1ass/tt_riingd/actions/workflows/ci.yml/badge.svg)](https://github.com/At1ass/tt_riingd/actions/workflows/ci.yml)  [![License](https://img.shields.io/badge/license-MIT-green.svg)](#license)

`tt-riingd` is a lightweight Rust daemon for controlling Thermaltake Riing fans on Linux via HID and exposing a D-Bus interface.

> **Early development:** pre-alpha; full configuration support (curves, sensors) arrives in v0.4.

## Features

* **Asynchronous I/O** with [Tokio](https://tokio.rs/) for non-blocking device access.
* **HID driver** for Thermaltake Riing controllers (PID 0x232B–0x232E).
* **D-Bus interface** (bus name: `io.github.tt_riingd`, object path: `/io/github/tt_riingd`, interface: `io.github.tt_riingd1`):

  * Methods: `GetActiveCurve(y controller, y channel) → s`, `SwitchActiveCurve(y, y, s)`, `UpdateCurveData(y, y, s, s)`, `Stop()`
  * Properties: `Version (s)`
  * Signal: `Stopped()`
* **YAML configuration** (v0.4+): define polling interval, default speeds, curves, LED modes and sensor backends in `config/config.yml`.
* **CLI utility** `riingctl` (Bash script) for quick D-Bus calls.
* **Udev rule** for non-root HID access (`99-tt-riingd.rules`).
* **User & Systemd integration**: ship service units for user and system scopes.
* **Zero runtime deps** beyond core crates: `tokio`, `hidapi`, `zbus`, `serde_yaml`, `clap`.
* **GitHub Actions CI**: formatting, linting, tests on PR & push.

## Installation

```bash
git clone https://github.com/At1ass/tt_riingd.git
cd tt_riingd
cargo build --release
sudo install -Dm755 target/release/tt-riingd /usr/local/bin/tt-riingd
```

## Udev Rule

Place `99-tt-riingd.rules` in `/etc/udev/rules.d/`:

```ini
# Thermaltake Riing controllers: PID 0x232B–0x232E
SUBSYSTEM=="hidraw", SUBSYSTEMS=="usb", ATTRS{idVendor}=="264a", ATTRS{idProduct}=="232?", TAG+="uaccess", TAG+="Thermaltake_Riing"
```

```bash
sudo cp 99-tt-riingd.rules /etc/udev/rules.d/
sudo udevadm control --reload\sudo udevadm trigger
```

## Configuration (v0.4+)

Defaults work with minimal setup. To customize, create `config/config.yml`:

```yaml
tick_seconds: 2      # sensor polling interval (sec)
init_speed: 50       # default fan speed (%)

controllers:
  - id: 1
    curves:
      - name: Default
        temps: [30.0, 70.0, 90.0]
        speeds: [20, 50, 100]

sensors:
  - type: lm_sensors
```

Override location:

```bash
export TT_RIINGD_CONFIG=/etc/tt-riingd/config.yml
```

## Running

### Systemd (user)

```ini
# ~/.config/systemd/user/tt-riingd.service
[Unit]
Description=tt-riingd — Riing fan controller
after=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/tt-riingd --config $XDG_CONFIG_HOME/tt-riingd/config.yml
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal
PrivateTmp=true
ProtectSystem=full

[Install]
WantedBy=default.target
```

```bash
systemctl --user daemon-reload
systemctl --user enable --now tt-riingd
journalctl --user -u tt-riingd -f
```

### System scope

```ini
# /etc/systemd/system/tt-riingd.service
[Unit]
Description=tt-riingd — Riing fan controller
after=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/tt-riingd --config /etc/tt-riingd/config.yml
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal
PrivateTmp=true
ProtectSystem=full

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now tt-riingd
```

## D-Bus Introspection

```bash
busctl --user introspect io.github.tt_riingd /io/github/tt_riingd
```

## CLI: `riingctl`

Make executable and in your PATH:

```bash
chmod +x riingctl && mv riingctl ~/bin/
```

Usage:

```bash
riingctl <command> [args]
```

Common commands:

* `version`
* `get-active-curve <controller> <channel>`
* `switch-active-curve <controller> <channel> <curve_name>`
* `update-curve-data <controller> <channel> <curve_name> <curve_json>`
* `stop`

## Development

* Format: `cargo fmt --all`
* Lint: `cargo clippy --all-targets -- -D warnings`
* Test: `cargo test --all`

## Roadmap & Contributions

See `ROADMAP.md` for planned features (GUI, plugin API, packaging). Contributions welcome! Please open issues and PRs.

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.

---

© 2025 At1ass and contributors
