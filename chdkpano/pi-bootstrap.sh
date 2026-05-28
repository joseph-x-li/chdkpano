#!/usr/bin/env bash
# One-time Pi-side setup. Run via ssh from the Mac:
#
#   ssh pi@raspberrypi.local 'bash -s' < pi-bootstrap.sh
#
# Sets up:
#   1. udev rule giving the `pi` user direct access to Canon USB devices
#      (avoids running the server as root)
#   2. user-level systemd unit so `systemctl --user restart chdkpano` works
#      from deploy.sh without sudo
#   3. lingering for the `pi` user so the service runs at boot, before login
#
# Idempotent — safe to re-run.

set -euo pipefail

DEPLOY_DIR="${DEPLOY_DIR:-$HOME/chdkpano}"
CANON_VENDOR_ID="04a9"

mkdir -p "$DEPLOY_DIR" "$DEPLOY_DIR/dist"

# ----- 1. udev rule -----
# nusb on Linux opens /dev/bus/usb/* directly. Default permissions are
# 0664 root:root, so we need either root or a rule. Mode 0666 is the
# pragmatic choice for a personal/hobby Pi; tighten with GROUP="plugdev"
# + adduser if you care.
echo "==> Installing udev rule for Canon vendor 0x$CANON_VENDOR_ID"
sudo tee /etc/udev/rules.d/99-canon-chdk.rules >/dev/null <<EOF
# chdkpano: let any local user open Canon PTP cameras directly via nusb
SUBSYSTEM=="usb", ATTR{idVendor}=="$CANON_VENDOR_ID", MODE="0666"
EOF
sudo udevadm control --reload-rules
sudo udevadm trigger

# ----- 2. systemd user unit -----
echo "==> Writing ~/.config/systemd/user/chdkpano.service"
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/chdkpano.service" <<EOF
[Unit]
Description=chdkpano server (Canon CHDK web UI)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=$DEPLOY_DIR
Environment=CHDKPANO_STATIC_DIR=$DEPLOY_DIR/dist
Environment=CHDKPANO_ADDR=0.0.0.0:3030
Environment=RUST_LOG=chdkpano_server=info,tower_http=info
ExecStart=$DEPLOY_DIR/chdkpano-server
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
EOF

# ----- 3. lingering so the service stays up across logouts / reboot -----
# loginctl enable-linger needs root.
echo "==> Enabling user lingering for $USER"
sudo loginctl enable-linger "$USER"

# Reload + enable. Won't start yet because the binary isn't here on a fresh
# bootstrap — deploy.sh handles the initial copy + start.
systemctl --user daemon-reload
systemctl --user enable chdkpano.service

echo
echo "==> Bootstrap done."
echo "    Now back on the Mac:  ./deploy.sh"
echo "    Then browse:           http://$(hostname).local:3030"
