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
echo "  [6.1] identity fixture"
cargo run -p dragonforge-test-lab -- identity-fixture

echo "  [6.2] MCP fixture"
old_mcp_token="${DRAGONFORGE_MCP_TOKEN-}"
old_mcp_allowlist="${DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES-}"
had_mcp_token=0
had_mcp_allowlist=0
[[ -v DRAGONFORGE_MCP_TOKEN ]] && had_mcp_token=1
[[ -v DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES ]] && had_mcp_allowlist=1
export DRAGONFORGE_MCP_TOKEN="phase23-fixture-token-0123456789abcdef"
export DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES="https://github.com/djames1987/"
cargo run -p dragonforge-test-lab -- mcp-fixture
if [[ $had_mcp_token -eq 1 ]]; then export DRAGONFORGE_MCP_TOKEN="$old_mcp_token"; else unset DRAGONFORGE_MCP_TOKEN; fi
if [[ $had_mcp_allowlist -eq 1 ]]; then export DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES="$old_mcp_allowlist"; else unset DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES; fi

echo "  [6.3] observability fixture"
cargo run -p dragonforge-test-lab -- observability-fixture
echo "  [6.4] installer fixture"
cargo run -p dragonforge-test-lab -- install-fixture
echo "  [6.5] release fixture"
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
