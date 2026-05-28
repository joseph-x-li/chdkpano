#!/usr/bin/env bash
# Mac-side deploy: cross-compile the server, build the wasm client, rsync
# both to the Pi, restart the systemd service.
#
# First time on a new Pi: run pi-bootstrap.sh once over ssh (see top of that
# file). After that, just `./deploy.sh` whenever you want to push a build.
#
# Overridable env vars:
#   PI_HOST    — ssh target (default: pi@raspberrypi.local)
#   DEPLOY_DIR — where the binary + dist live on the Pi (default: ~/chdkpano)
#   TARGET     — Rust target triple (default: aarch64-unknown-linux-gnu)

set -euo pipefail

PI_HOST="${PI_HOST:-pi@raspberrypi.local}"
DEPLOY_DIR="${DEPLOY_DIR:-chdkpano}"
TARGET="${TARGET:-aarch64-unknown-linux-gnu}"

cd "$(dirname "$0")"

command -v cross >/dev/null || { echo "missing: cargo install cross"; exit 1; }
command -v trunk >/dev/null || { echo "missing: cargo install trunk"; exit 1; }
command -v rsync >/dev/null || { echo "missing: brew install rsync"; exit 1; }

echo "==> Building wasm client (release)"
(cd client && trunk build --release)

echo "==> Cross-compiling chdkpano-server for $TARGET"
# Auto-mount the out-of-workspace path dep if Cross.toml's volume line is
# enabled (no-op otherwise — cross 0.2.5+ usually finds it automatically).
export CHDKPTP_DIR
CHDKPTP_DIR="$(cd ../../chdkptp_rs 2>/dev/null && pwd || true)"
cross build --release --target "$TARGET" -p chdkpano-server

echo "==> Syncing to $PI_HOST:~/$DEPLOY_DIR"
ssh "$PI_HOST" "mkdir -p ~/$DEPLOY_DIR"
rsync -avz --progress \
    "target/$TARGET/release/chdkpano-server" \
    "$PI_HOST:~/$DEPLOY_DIR/chdkpano-server"
rsync -avz --delete \
    "client/dist/" \
    "$PI_HOST:~/$DEPLOY_DIR/dist/"

echo "==> Restarting service"
# `|| true` so a fresh Pi that hasn't run pi-bootstrap.sh yet doesn't block
# the deploy — the next message tells the user what to do.
if ssh "$PI_HOST" "systemctl --user is-enabled chdkpano >/dev/null 2>&1"; then
    ssh "$PI_HOST" "systemctl --user restart chdkpano"
    echo "==> Done. http://$(echo "$PI_HOST" | cut -d@ -f2):3030"
else
    echo
    echo "==> chdkpano service isn't set up yet on the Pi."
    echo "    Run the one-time bootstrap:  ssh $PI_HOST 'bash -s' < pi-bootstrap.sh"
    echo "    Then re-run ./deploy.sh"
fi
