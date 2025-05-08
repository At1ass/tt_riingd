# tt-riing Quad Control — Roadmap

## Goal

- Step-by-step convert home MVP into a production-ready, extensible project  
- Each stage ends with a release that can be built and installed from AUR

## Legend

- **done** — completion criteria  
- **package** — packaging / distribution  
- **code** — code / architecture  
- **docs** — documentation / CI / lint  

---

## Stages

### v0.1 MVP (manual configuration)

- **code:** Rust prototype, single controller, manual `SetPwm` via HID  
- **done:** Fan responds; program exits without crashing  

### v0.2 Rust port

- **code:** crate `tt-riing-core`, error handling `anyhow`, unit tests  
- **done:** `cargo test` is green; CLI still works  

### v0.3 D-Bus + systemd

- **code:** skeleton with `zbus` (`SetPwm`, `GetRpm`)  
- **code:** systemd unit `tt-riingd.service` (user-scope)  
- **done:** `busctl call` changes speed; service restarts  

### v0.4 Unified `config.toml`

- **code:** parse/serde table Temps→PWM, auto-generate default in `$XDG_CONFIG_HOME`  
- **done:** daemon starts without parameters; file appears in `~/.config`  

### v0.5 GTK tray GUI

- **code:** gtkmm + libappindicator-gtk3: window, slider, Save button  
- **done:** Icon in Waybar; slider controls fan  

### v1.0 Stable 1

- **package:** split-package PKGBUILD (`tt-riingd`, `ctl`, `gtk`)  
- **code:** poll RPM, signal `RpmChanged`  
- **done:** `yay -S tt-riing-gtk` → works out of the box  

### v1.1 Config splitting

- **code:** switch to `curves.toml` + `topology.toml`, v0→v1 migrator  
- **done:** old config converts automatically  

### v1.2 Bezier and Points curves

- **code:** support `format="bezier" | "points"`, lookup table generator  
- **done:** GUI renders smooth curve; daemon interpolates  

### v1.3 Sensor selection

- **code:** crate `sensors` (lm-sensors + NVML)  
- **code:** `sensor` field per fan  
- **done:** GPU load only affects related fans  

### v1.4 Defaults cascade

- **code:** overlay loader `$CONFIG` → `/etc/xdg` → `/opt/.../defaults`  
- **done:** daemon starts even without user files  

### v1.5 Multi-controller mode

- **code:** USB probing, `HashMap<uid, Driver>`  
- **done:** two units controlled independently  

### v1.6 Native plugin API

- **code:** crate `plugin_api`, trait `FanController`, `libloading` dlopen  
- **done:** `driver_nzxt.so` loads; RPM reads  

### v2.0 LTS release

- **package:** Flatpak, updated AUR, tar.gz release  
- **code:** freeze ABI, CI (clippy/fmt/tests) green  
- **docs:** full documentation: config, D-Bus, SDK  
- **done:** tag `v2.0.0`, all packages available  

---

## Cross-cutting tasks

- **code:** `rustfmt`, `clippy --deny warnings`, pre-commit  
- **code:** structured logging `tracing`  
- **docs:** expand wiki/docs at each stage  
- **code:** unit tests for new logic  

## Risks and mitigations

- ABI break → `plugin_api::ABI_VERSION`, SemVer discipline  

## Final state

- daemon **tt-riingd**: multi-controller, sensor selection  
- CLI **tt-riingctl** and GUI **tt-riing-gtk** (AUR / Flatpak)  
- two-layer configs with overlay defaults  
- curve library (bezier/points/table) + editor in GUI  
- native SDK for driver plugins (.so, Rust/C)  
- full CI, documentation and LTS support branch  
