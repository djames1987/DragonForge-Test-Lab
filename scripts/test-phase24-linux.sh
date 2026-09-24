#!/usr/bin/env bash
set -euo pipefail
stress_jobs=1000
repeat_iterations=3
while [[ $# -gt 0 ]]; do
  case "$1" in
    --stress-jobs) stress_jobs="$2"; shift 2 ;;
    --repeat-iterations) repeat_iterations="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
if (( stress_jobs < 1 || stress_jobs > 2000 )); then echo "stress jobs must be 1..2000" >&2; exit 2; fi
if (( repeat_iterations < 1 || repeat_iterations > 20 )); then echo "repeat iterations must be 1..20" >&2; exit 2; fi

mkdir -p ./test-logs
stamp="$(date +%Y%m%d-%H%M%S)"
log="./test-logs/phase24-reliability-chaos-$stamp.log"
exec > >(tee "$log") 2>&1

echo "[0/9] Environment"
git --version
cargo --version
rustc --version

echo "[1/9] cargo fmt"
cargo fmt --all -- --check

echo "[2/9] strict cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/9] workspace tests"
cargo test --workspace --all-features

echo "[4/9] chaos doctor"
cargo run -p dragonforge-test-lab -- chaos-doctor

echo "[5/9] full chaos fixture ($stress_jobs stress jobs)"
cargo run -p dragonforge-test-lab -- chaos-fixture --stress-jobs "$stress_jobs"

echo "[6/9] repeated recovery/stress cycles"
for ((i=1; i<=repeat_iterations; i++)); do
  echo "  chaos iteration $i/$repeat_iterations"
  cargo run -q -p dragonforge-test-lab -- chaos-fixture --stress-jobs 250
done

echo "[7/9] lifecycle, worker-service, distributed, identity regressions"
cargo run -p dragonforge-test-lab -- lifecycle-fixture
cargo run -p dragonforge-test-lab -- worker-service-fixture
cargo run -p dragonforge-test-lab -- distributed-fixtures
cargo run -p dragonforge-test-lab -- identity-fixture
cargo run -p dragonforge-test-lab -- observability-fixture

echo "[8/9] focused chaos tests"
cargo test -p df-test-chaos

echo "[9/9] general doctor reports Phase 24"
general="$(cargo run -q -p dragonforge-test-lab -- doctor)"
echo "$general"
grep -q '^phase=24$' <<<"$general"

echo "Phase 24 Reliability / Chaos validation passed."
echo "Log file: $log"
