#!/usr/bin/env bash
# 0verwatch installer
#
# Builds the release binary and installs it plus a systemd service + timer that
# runs a host health survey periodically and (optionally) posts it to EventFlow.
#
# Usage:
#   sudo ./deploy/install.sh                 # install with defaults
#   sudo INTERVAL=1min ./deploy/install.sh   # change timer interval
#   sudo NO_TIMER=1 ./deploy/install.sh      # install binary only
#   sudo UNINSTALL=1 ./deploy/install.sh     # remove units + binary
set -euo pipefail

PREFIX="${PREFIX:-/usr/local/bin}"
CONFIG_DIR="/etc/0verwatch"
UNIT_DIR="/etc/systemd/system"
INTERVAL="${INTERVAL:-5min}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "${SCRIPT_DIR}")"

require_root() {
    if [ "$(id -u)" -ne 0 ]; then
        echo "error: must run as root (try: sudo $0)" >&2
        exit 1
    fi
}

uninstall() {
    echo "==> Uninstalling 0verwatch"
    systemctl disable --now 0verwatch.timer 2>/dev/null || true
    rm -f "${UNIT_DIR}/0verwatch.timer" "${UNIT_DIR}/0verwatch.service"
    systemctl daemon-reload || true
    rm -f "${PREFIX}/0verwatch"
    echo "    removed units, timer, and ${PREFIX}/0verwatch"
    echo "    (left ${CONFIG_DIR} in place; delete manually if desired)"
}

install_units() {
    echo "==> Installing binary to ${PREFIX}/0verwatch"
    install -Dm755 "${PROJECT_DIR}/target/release/0verwatch" "${PREFIX}/0verwatch"

    echo "==> Installing systemd units"
    install -Dm644 "${SCRIPT_DIR}/0verwatch.service" "${UNIT_DIR}/0verwatch.service"
    install -Dm644 "${SCRIPT_DIR}/0verwatch.timer" "${UNIT_DIR}/0verwatch.timer"

    # Apply the requested interval to the timer
    sed -i "s/^OnUnitActiveSec=.*/OnUnitActiveSec=${INTERVAL}/" "${UNIT_DIR}/0verwatch.timer"

    echo "==> Installing config"
    install -d -m755 "${CONFIG_DIR}"
    if [ ! -f "${CONFIG_DIR}/eventflow.env" ]; then
        install -m640 "${SCRIPT_DIR}/eventflow.env.example" "${CONFIG_DIR}/eventflow.env"
        echo "    created ${CONFIG_DIR}/eventflow.env (set EVENTFLOW_URL to enable telemetry)"
    else
        echo "    kept existing ${CONFIG_DIR}/eventflow.env"
    fi

    systemctl daemon-reload
}

require_root

if [ "${UNINSTALL:-0}" = "1" ]; then
    uninstall
    exit 0
fi

echo "==> Building release binary"
if command -v cargo >/dev/null 2>&1; then
    (cd "${PROJECT_DIR}" && cargo build --release)
else
    echo "error: cargo not found; install Rust (https://rustup.rs) or build elsewhere" >&2
    exit 1
fi

install_units

if [ "${NO_TIMER:-0}" = "1" ]; then
    echo "==> NO_TIMER set — skipping timer enable"
else
    echo "==> Enabling and starting timer (interval: ${INTERVAL})"
    systemctl enable --now 0verwatch.timer
    echo "    last/next run:"
    systemctl list-timers 0verwatch.timer --no-pager || true
fi

echo "==> Done!"
echo "    Run once now:  ${PREFIX}/0verwatch --json-pretty"
echo "    Timer logs:    journalctl -u 0verwatch.service -f"
echo "    Configure:     \$EDITOR ${CONFIG_DIR}/eventflow.env"
