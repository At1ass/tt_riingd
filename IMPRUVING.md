Документ собирает все договорённости о дальнейших доработках проекта.
Задачи распределены по _важности_:

* **MUST** — критично для первой публичной β (работоспособность, безопасность).
* **SHOULD** — желательны в ближайших минорных релизах (v0.2 – v0.4).
* **COULD / LATER** — откладываем до стабилизации ядра.

---

## Ⅰ  MUST   («ядро, без которого нельзя»)

| # | Статус | Подзадача | Суть |
|---|--------|-----------|------|
| 1 | [x] | **Autodetect Controllers** | USB‑скан, драйвер для каждого найденного устройства. |
| 2 | [ ] | **YAML Validation** | `Config::validate()`, `config_version`, ошибка старта если невалидно. |
| 3 | [x]| **Factory‑Default Safe‑Mode** | 100 % PWM + статичный RGB при любых проблемах. |
| 4 | [ ] | **Sensor Registry** | Однократный обход `lm_sensors`, кэш friendly‑ID ↔ hwmon‑path. |
| 5 | [ ] | **CtrlState LOST/ALIVE** | Без спама ошибок, SAFE‑PWM при `Lost`. |
| 6 | [ ] | **`rescan` Command** | D‑Bus/CLI, точечный diff устройств. |
| 7 | [ ] | **UdevWatcher** | Таска слушает `add`/`remove`, debounce → `rescan()`. |
| 8 | [ ] | **Broken‑Config Test** | Hot‑reload невалидного YAML ≈ сохраняем прежнее состояние. |
| 9 | [ ] | **INIT.md** | Докфикс правил инициализации и safe‑mode. |

---

## Ⅱ  SHOULD   («следующие релизы»)

| # | Статус | Подзадача | Описание |
|---|-----|------|----------|
| 10 | [ ] | Hardware‑Effects | `supported_hardware_effects` → авто‑выбор HW/SW. |
| 11 | [ ] | Multi‑Controller SW‑Effects | Единый кадр на несколько устройств. |
| 12 | [ ] | Pipeline / Mixer | Слои, alpha/add‑blend. |
| 13 | [ ] | Reactive Effects | Сенсор‑подписка, t° / load / audio‑VU. |
| 14 | [ ] | Sample‑Config CLI | `--write-sample-config`. |
| 15 | [ ] | Fallback‑Config flag | `--fallback-config /path`. |
| 16 | [ ] | JSON‑Schema Export | `schemars` → IDE подсветка. |
| 17 | [ ] | Packaging | deb / rpm / AUR GH‑Actions. |
| 18 | [ ] | Metrics | Prometheus counters reconnect, gauge active controllers. |

---

## Ⅲ  COULD / LATER

| # | Статус | Идея | Когда |
|---|----|--|-------|
| 19 | [ ] | Hot‑plug hwmon sensors | udev add → обновить SensorRegistry. |
| 20 | [ ] | Minimal GUI | Tauri/GTK: список устройств, кривая, эффекты. |
| 21 | [ ] | Fan Groups | `front_panel`, `top_panel` логически в YAML/GUI. |
| 22 | [ ] | Preset Storage | Импорт/экспорт профилей. |
| 23 | [ ] | REST‑Bridge | Web‑клиенты через HTTPS. |
| 24 | [ ] | Plugin Sandbox | seccomp ограничения. |
| 25 | [ ] | Driver HOWTO | Док для контрибьюторов. |

---

## Ⅳ  Icebox / R&D

* HW+SW смешение в одном кадре
* WASM‑preview кривых
* Авто‑профили по времени суток
* Keyboard‑RGB = temp GPU
