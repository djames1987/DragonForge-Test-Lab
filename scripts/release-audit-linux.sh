#!/usr/bin/env bash
set -euo pipefail
install_tools=0
output="${1:-./dist/audit-report.txt}"
[[ "${2:-}" == "--install-tools" ]] && install_tools=1
if [[ $install_tools -eq 1 ]]; then
  command -v cargo-audit >/dev/null 2>&1 || cargo install cargo-audit --locked
  command -v cargo-deny >/dev/null 2>&1 || cargo install cargo-deny --locked
fi
command -v cargo-audit >/dev/null 2>&1 || { echo "cargo-audit is required" >&2; exit 1; }
command -v cargo-deny >/dev/null 2>&1 || { echo "cargo-deny is required" >&2; exit 1; }
mkdir -p "$(dirname "$output")"
{
  echo "DragonForge Test Lab release audit"
  echo "commit=$(git rev-parse HEAD)"
  echo
  echo "== cargo audit =="
  cargo audit
  echo
  echo "== cargo deny licenses advisories sources =="
  cargo deny check licenses advisories sources
} | tee "$output"
