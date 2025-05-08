**tt-riingd** is a lightweight Rust daemon for controlling Thermaltake Riing fans on Linux via D-Bus.

> [!NOTE]
> tt-riingd is in early development. Configuration support (curves, sensor mapping) will be added in v0.4.

## Features

- Asynchronous, non-blocking I/O powered by [Tokio](https://tokio.rs/)  
- D-Bus API methods:
  - **SetPwm(u32 level)** — set fan speed (0–100)  
  - **GetRpm() → u32** — read current RPM  
  - **Stop()** — gracefully shut down the daemon  
- User-scope `systemd` integration (`tt-riingd.service`)  
- Zero runtime dependencies on external crates  
- Graceful shutdown and structured error logging

> [!TIP]
> Use `journalctl --user -u tt-riingd.service -f` to follow live logs.

## Installation

```bash
git clone https://github.com/At1ass/tt_riingd.git
cd tt_riingd
cargo build --release

sudo install -Dm755 target/release/tt-riingd /usr/local/bin/tt-riingd
```

> [!WARNING]
> All commands must be run in the same user session where the service is enabled.

## Usage

Control your fans via `busctl` on the session bus:

```bash
# Set fan to 50% PWM
busctl --user call io.github.tt_riingd \
  /io/github/tt_riingd io.github.tt_riingd SetSpeed y 50

# Stop the daemon
busctl --user call io.github.tt_riingd \
  /io/github/tt_riingd io.github.tt_riingd Stop
```

## Configuration

> [!NOTE]
> Configuration files not yet supported. Defaults are hardcoded until v0.4.

## Contributing

Contributions are welcome!

1. Fork the repo  
2. Create a branch: `git checkout -b feature/my-feature`  
3. Commit changes: `git commit -m "Add awesome feature"`  
4. Push and open a PR  

Please run `cargo fmt`, `cargo clippy` and existing tests before submitting.

## Acknowledgments

| Project     | Purpose                           |
|------------ |---------------------------------- |
| [Tokio]     | Async runtime                    |
| [zbus]      | D-Bus interface                  |
| [anyhow]    | Error handling                   |
| [systemd]   | Service integration (systemd)    |

[Tokio]: https://tokio.rs/  
[zbus]: https://docs.rs/zbus  
[anyhow]: https://docs.rs/anyhow  
[systemd]: https://www.freedesktop.org/wiki/Software/systemd/

---
