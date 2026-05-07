#!/usr/bin/env bash
# Install / reinstall the LaunchAgent that keeps an SSH tunnel to the
# remote cpu-monitord (default: wallabot:9124 → 127.0.0.1:9124).
# Usage:
#   ./scripts/install-tunnel.sh           # install + load
#   ./scripts/install-tunnel.sh uninstall # unload + remove
set -euo pipefail

LABEL="com.maximofn.cpu-monitor-tunnel"
SRC="$(cd "$(dirname "$0")" && pwd)/${LABEL}.plist"
DST="$HOME/Library/LaunchAgents/${LABEL}.plist"

uid="$(id -u)"
domain="gui/${uid}"
target="${domain}/${LABEL}"

cmd="${1:-install}"

case "$cmd" in
    install)
        if launchctl print "$target" >/dev/null 2>&1; then
            echo "==> bootout existing $LABEL"
            launchctl bootout "$target" || true
        fi

        echo "==> install $DST"
        mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
        cp "$SRC" "$DST"

        echo "==> bootstrap $target"
        launchctl bootstrap "$domain" "$DST"
        launchctl enable "$target"
        launchctl kickstart -k "$target"

        echo
        echo "Loaded. The SSH tunnel will autostart at login."
        echo "Logs: ~/Library/Logs/cpu-monitor-tunnel.{out,err}.log"
        ;;
    uninstall)
        if launchctl print "$target" >/dev/null 2>&1; then
            echo "==> bootout $target"
            launchctl bootout "$target" || true
        fi
        if [[ -f "$DST" ]]; then
            echo "==> remove $DST"
            rm -f "$DST"
        fi
        echo "Uninstalled."
        ;;
    *)
        echo "usage: $0 [install|uninstall]" >&2
        exit 2
        ;;
esac
