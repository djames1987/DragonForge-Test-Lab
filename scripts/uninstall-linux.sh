#!/usr/bin/env bash
set -euo pipefail

purge=0
[[ "${1:-}" == "--purge" ]] && purge=1
[[ $EUID -eq 0 ]] || { echo "uninstall-linux.sh must run as root" >&2; exit 1; }

systemctl disable --now dragonforge-test-worker.service 2>/dev/null || true
rm -f /etc/systemd/system/dragonforge-test-worker.service
systemctl daemon-reload
rm -rf /opt/dragonforge/test-lab

if [[ $purge -eq 1 ]]; then
  rm -rf /etc/dragonforge/test-lab /var/lib/dragonforge/test-lab /var/log/dragonforge/test-lab
  echo "DragonForge Test Lab uninstalled and state purged."
else
  echo "DragonForge Test Lab uninstalled. Configuration, state, logs, and backups were preserved."
fi
