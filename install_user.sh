#!/bin/bash

set -e

echo "Installing tt_riingd as user service..."

# Create directories
mkdir -p ~/.local/bin
mkdir -p ~/.config/tt_riingd
mkdir -p ~/.config/systemd/user
mkdir -p ~/.local/share/dbus-1/services

# Copy binary
echo "Installing binary..."
cp target/release/tt_riingd ~/.local/bin/
chmod +x ~/.local/bin/tt_riingd

# Copy config
echo "Installing configuration..."
if [ ! -f ~/.config/tt_riingd/config.yml ]; then
    cp config/config.yml ~/.config/tt_riingd/
    echo "Configuration installed to ~/.config/tt_riingd/config.yml"
else
    echo "Configuration already exists at ~/.config/tt_riingd/config.yml"
fi

# Install systemd user service
echo "Installing systemd user service..."
cp resources/tt_riingd.service ~/.config/systemd/user/
systemctl --user daemon-reload

# Install udev rules (requires sudo for HID access)
echo "Installing udev rules (requires sudo)..."
if [ -f resources/99-tt-riingd.rules ]; then
    sudo cp resources/99-tt-riingd.rules /etc/udev/rules.d/
    sudo udevadm control --reload
    sudo udevadm trigger
    echo "Udev rules installed successfully"
else
    echo "Warning: udev rules file not found, manual installation may be required"
fi

# Enable and start service
echo "Enabling and starting service..."
systemctl --user enable tt_riingd.service
systemctl --user start tt_riingd.service

echo "Installation complete!"
echo ""
echo "Service status:"
systemctl --user status tt_riingd.service --no-pager
echo ""
echo "D-Bus connection:"
busctl --user list | grep tt_riingd || echo "D-Bus service not yet active"
echo ""
echo "You can now use: ./riingctl version" 