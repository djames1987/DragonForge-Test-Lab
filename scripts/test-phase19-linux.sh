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
  echo "Phase 19 qualification must run on Linux." >&2
  exit 1
fi

mkdir -p "$log_directory"
timestamp="$(date +%Y%m%d-%H%M%S)"
log_path="$log_directory/phase19-linux-validation-$timestamp.log"
exec > >(tee "$log_path") 2>&1

echo "=== DragonForge Test Lab - Phase 19 Linux Qualification ==="
echo "Started: $(date --iso-8601=seconds)"
echo "Revision: $revision"
echo "Kernel: $(uname -a)"

if [[ $install_tools -eq 1 ]]; then
  cargo install --locked cargo-nextest
  cargo install --locked cargo-llvm-cov
fi

if [[ -z "$container_runtime" ]]; then
  if command -v docker >/dev/null 2>&1; then
    container_runtime="docker"
  elif command -v podman >/dev/null 2>&1; then
    container_runtime="podman"
  else
    echo "Docker or Podman is required for Phase 19 container qualification." >&2
    exit 1
  fi
fi

echo "[0/15] Environment"
git --version
cargo --version
rustc --version
rustup show active-toolchain
gh --version
"$container_runtime" --version

echo "[1/15] cargo fmt"
cargo fmt --all -- --check

echo "[2/15] strict cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/15] full workspace tests"
cargo test --workspace --all-features

echo "[4/15] Linux doctor"
cargo run -p dragonforge-test-lab -- linux-doctor

echo "[5/15] Linux containment/service/mTLS fixture"
cargo run -p dragonforge-test-lab -- linux-fixture

echo "[6/15] Native sandbox doctor"
cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox native

echo "[7/15] Build container sandbox image"
bash ./scripts/build-sandbox-image.sh "$container_runtime"

echo "[8/15] Container sandbox doctor"
cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox "$container_runtime"

echo "[9/15] Worker service regression"
cargo run -p dragonforge-test-lab -- worker-service-fixture

echo "[10/15] Recovery/lifecycle and observability regressions"
cargo run -p dragonforge-test-lab -- lifecycle-fixture
cargo run -p dragonforge-test-lab -- observability-fixture

echo "[11/15] Advanced Rust - nextest"
cargo nextest run --workspace --all-features --no-fail-fast

echo "[12/15] Advanced Rust - coverage/property/benchmark"
cargo llvm-cov --workspace --all-features --summary-only
cargo test -p df-test-protocol property_
cargo bench -p df-test-protocol --bench capability_derivation --no-run

echo "[13/15] GitHub-aware native worker"
cargo run -p dragonforge-test-lab -- run-github --repo "$repository_url" --revision "$revision" --sandbox native --no-status

echo "[14/15] GitHub-aware container worker"
cargo run -p dragonforge-test-lab -- run-github --repo "$repository_url" --revision "$revision" --sandbox "$container_runtime" --no-status

echo "[15/15] General doctor reports Phase 19"
general="$(cargo run -q -p dragonforge-test-lab -- doctor)"
echo "$general"
grep -q "phase=19" <<<"$general"

echo
echo "Phase 19 Linux Qualification validation passed."
echo "Log file: $log_path"
