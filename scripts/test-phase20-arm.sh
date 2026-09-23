#!/usr/bin/env bash
set -euo pipefail

repository_url="https://github.com/djames1987/DragonForge-Test-Lab.git"
revision="main"
log_directory="./test-logs"
install_tools=0
container_runtime=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repository-url) repository_url="$2"; shift 2 ;;
    --revision) revision="$2"; shift 2 ;;
    --log-directory) log_directory="$2"; shift 2 ;;
    --install-tools) install_tools=1; shift ;;
    --container-runtime) container_runtime="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Phase 20 qualification must run on Linux." >&2
  exit 1
fi

machine="$(uname -m)"
case "$machine" in
  aarch64|arm64|armv7l|armv8l) ;;
  *)
    echo "Phase 20 qualification requires physical/native ARM hardware; detected: $machine" >&2
    exit 1
    ;;
esac

mkdir -p "$log_directory"
timestamp="$(date +%Y%m%d-%H%M%S)"
log_path="$log_directory/phase20-arm-validation-$timestamp.log"
exec > >(tee "$log_path") 2>&1

echo "=== DragonForge Test Lab - Phase 20 ARM / Raspberry Pi Qualification ==="
echo "Started: $(date --iso-8601=seconds)"
echo "Revision: $revision"
echo "Kernel: $(uname -a)"
echo "Machine: $machine"

if [[ $install_tools -eq 1 ]]; then
  cargo install --locked cargo-nextest
  cargo install --locked cargo-llvm-cov
fi

echo "[0/13] Environment"
git --version
cargo --version
rustc --version
rustup show active-toolchain
gh --version
gh auth status -h github.com

echo "[1/13] cargo fmt"
cargo fmt --all -- --check

echo "[2/13] strict cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/13] full workspace tests"
cargo test --workspace --all-features

echo "[4/13] ARM/Raspberry Pi doctor"
cargo run -p dragonforge-test-lab -- arm-doctor

echo "[5/13] Deterministic ARM/HIL fixture"
cargo run -p dragonforge-test-lab -- arm-fixture

echo "[6/13] Typed read-only hardware probes"
for probe in board-model cpu-temperature gpio-controllers i2c-buses spi-devices serial-devices; do
  cargo run -p dragonforge-test-lab -- arm-probe --probe "$probe"
done

echo "[7/13] Linux containment/service/mTLS regression"
cargo run -p dragonforge-test-lab -- linux-fixture
cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox native

echo "[8/13] Worker service regression"
cargo run -p dragonforge-test-lab -- worker-service-fixture

echo "[9/13] Advanced Rust - nextest"
cargo nextest run --workspace --all-features --no-fail-fast

echo "[10/13] Advanced Rust - coverage/property"
cargo llvm-cov --workspace --all-features --summary-only
cargo test -p df-test-protocol property_

echo "[11/13] GitHub-aware native ARM worker"
cargo run -p dragonforge-test-lab -- run-github --repo "$repository_url" --revision "$revision" --sandbox native --no-status

if [[ -n "$container_runtime" ]]; then
  case "$container_runtime" in
    docker|podman) ;;
    *) echo "container runtime must be docker or podman" >&2; exit 2 ;;
  esac
  echo "[optional] ARM container sandbox"
  bash ./scripts/build-sandbox-image.sh "$container_runtime"
  cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox "$container_runtime"
  cargo run -p dragonforge-test-lab -- run-github --repo "$repository_url" --revision "$revision" --sandbox "$container_runtime" --no-status
fi

echo "[12/13] General doctor reports Phase 20"
general="$(cargo run -q -p dragonforge-test-lab -- doctor)"
echo "$general"
reported_phase="$(sed -n 's/^phase=//p' <<<"$general")"
if [[ -z "$reported_phase" || "$reported_phase" -lt 20 ]]; then
  echo "general doctor reported phase '$reported_phase'; expected Phase 20 or later" >&2
  exit 1
fi

echo "[13/13] ARM doctor still reports ready"
arm="$(cargo run -q -p dragonforge-test-lab -- arm-doctor)"
echo "$arm"
grep -q "status=arm_worker_ready" <<<"$arm"

echo
echo "Phase 20 ARM / Raspberry Pi Qualification validation passed."
echo "Log file: $log_path"
