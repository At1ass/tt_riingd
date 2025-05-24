#!/usr/bin/env bash
set -euo pipefail

# пути в домашней папке
USER_SYSTEMD_DIR="$HOME/.config/systemd/user"
LOCAL_BIN="$HOME/.local/bin"

REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"
BINARY_NAME="tt_riingd"
SERVICE_NAME="tt_riingd.service"
UDEV_RULE="99-tt-riingd.rules"

echo "==> Building release…"
cd "$REPO_ROOT"
cargo build --release

echo "==> Installing binary to $LOCAL_BIN/$BINARY_NAME…"
mkdir -p "$LOCAL_BIN"
install -Dm755 "target/release/$BINARY_NAME" "$LOCAL_BIN/$BINARY_NAME"

echo "==> Installing systemd user-unit to $USER_SYSTEMD_DIR/$SERVICE_NAME…"
mkdir -p "$USER_SYSTEMD_DIR"
install -Dm644 "resources/$SERVICE_NAME" "$USER_SYSTEMD_DIR/$SERVICE_NAME"

echo "==> (Optional) Installing udev rule (requires sudo)…"
sudo install -Dm644 "resources/$UDEV_RULE" "/etc/udev/rules.d/$UDEV_RULE"
sudo udevadm control --reload
sudo udevadm trigger

echo "==> Reloading and starting user service…"
systemctl --user daemon-reload
systemctl --user enable --now "$SERVICE_NAME"

echo
echo "Done!
 • Проверьте статус: systemctl --user status $SERVICE_NAME
 • Убедитесь, что $LOCAL_BIN в вашем \$PATH
 • Для автозапуска без активной сессии: sudo loginctl enable-linger $USER"
