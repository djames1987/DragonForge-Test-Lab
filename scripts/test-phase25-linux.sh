#!/usr/bin/env bash
set -euo pipefail
run_external=0
if [[ "${1-}" == "--run-external-campaign" ]]; then run_external=1; fi

mkdir -p ./test-logs
stamp="$(date +%Y%m%d-%H%M%S)"
log="./test-logs/phase25-dogfooding-$stamp.log"
exec > >(tee "$log") 2>&1
sha="$(git rev-parse HEAD)"
[[ "$sha" =~ ^[0-9a-fA-F]{40}$ ]]

echo "[0/11] Environment"
git --version
cargo --version
rustc --version
gh --version
echo "Current commit: $sha"

echo "[1/11] cargo fmt"
cargo fmt --all -- --check

echo "[2/11] strict cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/11] workspace tests"
cargo test --workspace --all-features

echo "[4/11] dogfood doctor and fixture"
cargo run -p dragonforge-test-lab -- dogfood-doctor
cargo run -p dragonforge-test-lab -- dogfood-fixture

echo "[5/11] checked-in dogfood profiles"
cargo run -p dragonforge-test-lab -- dogfood-profile-validate --profile ./dogfood/dragonforge-test-lab.json
cargo run -p dragonforge-test-lab -- dogfood-profile-validate --profile ./dogfood/dragonforge-security-suite.json
cargo run -p dragonforge-test-lab -- dogfood-profile-validate --profile ./dogfood/dragonforge-security-test-lab.json
cargo run -p dragonforge-test-lab -- dogfood-campaign-validate --campaign ./dogfood/phase25-campaign.json

echo "[6/11] immutable self-host profile compilation"
cargo run -p dragonforge-test-lab -- dogfood-profile-compile --profile ./dogfood/dragonforge-test-lab.json --sha "$sha" --depth 1

echo "[7/11] self-hosted Test Lab dogfood run"
cargo run -p dragonforge-test-lab -- dogfood-run --profile ./dogfood/dragonforge-test-lab.json --revision "$sha" --depth 1 --lab-root ./test-logs/phase25-self-host

echo "[8/11] optional external DragonForge campaign"
if (( run_external == 1 )); then
  cargo run -p dragonforge-test-lab -- dogfood-campaign-run --campaign ./dogfood/phase25-campaign.json --lab-root ./test-logs/phase25-campaign
else
  echo "External campaign execution skipped; profiles and campaign were validated."
fi

echo "[9/11] Phase 24 reliability regression"
cargo run -p dragonforge-test-lab -- chaos-fixture --stress-jobs 250

echo "[10/11] focused dogfood tests"
cargo test -p df-test-dogfood

echo "[11/11] general doctor reports Phase 25"
general="$(cargo run -q -p dragonforge-test-lab -- doctor)"
echo "$general"
grep -q '^phase=25$' <<<"$general"

echo "Phase 25 Dogfooding validation passed."
echo "Log file: $log"
