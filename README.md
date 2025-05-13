# tt-riingd

`tt-riingd` is a lightweight Rust daemon for controlling Thermaltake Riing fans on Linux via HID and exposing a D-Bus interface.

> [!NOTE]
> The project is in early development (pre-alpha). Configuration support (curves, sensor integration) is planned for v0.4.

## Features

* **Asynchronous, non-blocking I/O** powered by [Tokio](https://tokio.rs/)
* **D-Bus API methods**:

  * `GetActiveCurve(y controller, y channel) → s` — get the active fan curve name
  * `Stop()` — gracefully shut down the daemon
  * `SwitchActiveCurve(y controller, y channel, s curve_name)` — switch active curve
  * `UpdateCurveData(y controller, y channel, s curve_name, s curve_json)` — update curve parameters
* **Properties & Signals**:

  * `Version (s)` — returns daemon version
  * `Stopped()` — signal emitted on shutdown
* **CLI utility** `riingctl` (Bash script) for simplified D-Bus calls
* **User-scope systemd** integration (`tt-riingd.service`)
* Zero runtime dependencies beyond essential crates: `tokio`, `hidapi`, `zbus`

> [!TIP]
> Follow logs via `journalctl --user -u tt-riingd.service -f`.

## Installation

```bash
git clone https://github.com/yourusername/tt-riingd.git
cd tt-riingd
cargo build --release
sudo install -Dm755 target/release/tt-riingd /usr/local/bin/tt-riingd
```

> [!WARNING]
> Ensure you run commands in the same user session where the service will operate.

## Configuration

> [!NOTE]
> Configuration files are not yet supported. Defaults are hardcoded until v0.4.

By default, `tt-riingd` looks for `config/config.yml`. To override:

```bash
export TT_RIINGD_CONFIG=/etc/tt-riingd/config.yml
```

Example (future support):

```yaml
system:
  tick_seconds: 2      # sensor polling interval (seconds)
  init_speed: 50       # initial fan speed (%)
curves:
  ...                   # defined in v0.4
sensors:
  ...                   # defined in v0.4
```

## Running the Daemon

### systemd unit

Create `/etc/systemd/system/tt-riingd.service`:

```ini
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

### Manual launch

```bash
tt-riingd --config /path/to/config.yml
```

## D-Bus Interface

**Bus name:** `io.github.tt_riingd`
**Object path:** `/io/github/tt_riingd`
**Interface:** `io.github.tt_riingd1`

```bash
busctl --user introspect io.github.tt_riingd /io/github/tt_riingd
```

```text
NAME                   TYPE      SIGNATURE   RESULT/VALUE   FLAGS
.GetActiveCurve        method    yy          s               -
.Stop                  method    -           -               -
.SwitchActiveCurve     method    yys         -               -
.UpdateCurveData       method    yyss        -               -
.Version               property  s           "1.0"          emits-change
.Stopped               signal    -           -               -
```

## CLI: `riingctl`

A Bash script to simplify D-Bus calls. Place it in your `PATH` and make it executable:

```bash
chmod +x ~/bin/riingctl
```

```bash
riingctl <command> [args]
```

**Commands:**

* `introspect` — show D-Bus introspection
* `version` — get `Version` property
* `get-active-curve <controller> <channel>` — call `GetActiveCurve(y y)` → s
* `stop` — call `Stop()`
* `switch-active-curve <controller> <channel> <curve_name>` — call `SwitchActiveCurve(y y s)`
* `update-curve-data <controller> <channel> <curve_name> <curve_json>` — call `UpdateCurveData(y y s s)`

**Example:**

```bash
riingctl get-active-curve 1 1
riingctl switch-active-curve 1 1 StepCurve
riingctl update-curve-data 1 1 StepCurve '{"t":"StepCurve","c":{"temps":[0.0,100.0],"speeds":[20,100]}}'
```

## Development

* Format: `cargo fmt`
* Lint: `cargo clippy`

> [!ATTENTION]
> Unit tests for stable modules (e.g., HID packet serialization, curve parsing) are recommended as the project matures.

---

*This project is under active development. Contributions welcome!*
