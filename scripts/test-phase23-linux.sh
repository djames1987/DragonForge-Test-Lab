#!/usr/bin/env bash
set -euo pipefail
install_tools=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-tools) install_tools=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
mkdir -p ./test-logs
stamp="$(date +%Y%m%d-%H%M%S)"
log="./test-logs/phase23-security-review-$stamp.log"
report="./test-logs/phase23-security-report-$stamp.json"
exec > >(tee "$log") 2>&1

echo "=== DragonForge Test Lab - Phase 23 Security Review ==="
echo "[0/8] Environment"
git --version
cargo --version
rustc --version

echo "[1/8] cargo fmt"
cargo fmt --all -- --check

echo "[2/8] strict cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/8] workspace tests"
cargo test --workspace --all-features

echo "[4/8] security doctor, fixture, and repository review"
cargo run -p dragonforge-test-lab -- security-doctor
cargo run -p dragonforge-test-lab -- security-fixture
cargo run -p dragonforge-test-lab -- security-review --root . --output "$report"

echo "[5/8] dependency/advisory/license/source audit"
if [[ $install_tools -eq 1 ]]; then
  bash ./scripts/release-audit-linux.sh "./test-logs/phase23-dependency-audit-$stamp.txt" --install-tools
else
  bash ./scripts/release-audit-linux.sh "./test-logs/phase23-dependency-audit-$stamp.txt"
fi

echo "[6/8] security-boundary regressions"
cargo run -p dragonforge-test-lab -- identity-fixture
cargo run -p dragonforge-test-lab -- mcp-fixture
cargo run -p dragonforge-test-lab -- observability-fixture
cargo run -p dragonforge-test-lab -- install-fixture
cargo run -p dragonforge-test-lab -- release-fixture

echo "[7/8] focused policy and security-review tests"
cargo test -p df-test-policy
cargo test -p df-test-security-review

echo "[8/8] general doctor reports Phase 23"
general="$(cargo run -q -p dragonforge-test-lab -- doctor)"
echo "$general"
grep -q '^phase=23$' <<<"$general"

echo "Phase 23 Security Review validation passed."
echo "Security report: $report"
echo "Log file: $log"
