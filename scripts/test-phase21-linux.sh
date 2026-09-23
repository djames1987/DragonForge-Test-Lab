#!/usr/bin/env bash
set -euo pipefail

log_directory="./test-logs"
mkdir -p "$log_directory"
timestamp="$(date +%Y%m%d-%H%M%S)"
log_path="$log_directory/phase21-linux-validation-$timestamp.log"
exec > >(tee "$log_path") 2>&1

echo "=== DragonForge Test Lab - Phase 21 Installer / Upgrades ==="

echo "[0/7] Environment"
git --version
cargo --version
rustc --version

echo "[1/7] cargo fmt"
cargo fmt --all -- --check

echo "[2/7] strict cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/7] workspace tests"
cargo test --workspace --all-features

echo "[4/7] installer doctor"
cargo run -p dragonforge-test-lab -- install-doctor

echo "[5/7] installer fixture"
cargo run -p dragonforge-test-lab -- install-fixture

echo "[6/7] package build and verification"
rm -rf ./test-packages
bash ./scripts/package-release-linux.sh 0.22.0 ./test-packages
arch="$(uname -m)"
case "$arch" in
  x86_64) rust_arch="x86_64" ;;
  aarch64|arm64) rust_arch="aarch64" ;;
  armv7l|armv8l) rust_arch="arm" ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac
package="./test-packages/dragonforge-test-lab-0.22.0-linux-$rust_arch"
cargo run -p dragonforge-test-lab -- release-verify --manifest "$package/release-manifest.json" --package-root "$package"

echo "[7/7] general doctor reports Phase 21"
general="$(cargo run -q -p dragonforge-test-lab -- doctor)"
echo "$general"
grep -q "^phase=21$" <<<"$general"

echo "Phase 21 Installer / Upgrades validation passed."
echo "Log file: $log_path"
