#!/usr/bin/env bash
set -euo pipefail

USER_SYSTEMD_DIR="$HOME/.config/systemd/user"
LOCAL_BIN="$HOME/.local/bin"

SERVICE_NAME="tt_riingd.service"
BINARY_NAME="tt_riingd"
UDEV_RULE="99-tt-riingd.rules"

echo "==> Stopping and disabling user service…"
systemctl --user disable --now "$SERVICE_NAME" || true

echo "==> Removing user-unit…"
rm -f "$USER_SYSTEMD_DIR/$SERVICE_NAME"
systemctl --user daemon-reload

echo "==> Removing binary…"
rm -f "$LOCAL_BIN/$BINARY_NAME"

echo "==> (Optional) Removing udev rule (requires sudo)…"
sudo rm -f "/etc/udev/rules.d/$UDEV_RULE"
sudo udevadm control --reload
sudo udevadm trigger

echo "Uninstallation complete."
