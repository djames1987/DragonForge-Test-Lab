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
log="./test-logs/phase22-linux-validation-$stamp.log"
exec > >(tee "$log") 2>&1

echo "=== DragonForge Test Lab - Phase 22 Release Engineering ==="
echo "[0/9] Environment"
git --version
cargo --version
rustc --version
python3 --version

echo "[1/9] cargo fmt"
cargo fmt --all -- --check

echo "[2/9] strict cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/9] workspace tests"
cargo test --workspace --all-features

echo "[4/9] release doctor and fixture"
cargo run -p dragonforge-test-lab -- release-doctor
cargo run -p dragonforge-test-lab -- release-fixture

echo "[5/9] channel policy"
[[ "$(cargo run -q -p dragonforge-test-lab -- release-tag --channel stable --version 0.23.0)" == "v0.23.0" ]]
[[ "$(cargo run -q -p dragonforge-test-lab -- release-tag --channel beta --version 0.23.0-beta.1)" == "v0.23.0-beta.1" ]]
[[ "$(cargo run -q -p dragonforge-test-lab -- release-tag --channel dev --version 0.23.0-dev.1)" == "v0.23.0-dev.1" ]]

echo "[6/9] dependency/license audit"
if [[ $install_tools -eq 1 ]]; then
  bash ./scripts/release-audit-linux.sh ./test-release/audit-report.txt --install-tools
else
  bash ./scripts/release-audit-linux.sh ./test-release/audit-report.txt
fi

echo "[7/9] SBOM and dev release archive"
commit="$(git rev-parse HEAD)"
rm -rf ./test-release
mkdir -p ./test-release
bash ./scripts/release-audit-linux.sh ./test-release/audit-report.txt
python3 ./scripts/generate-sbom.py --version 0.23.0-dev.1 --commit "$commit" --output ./test-release/dragonforge-test-lab-0.23.0-dev.1.cdx.json
python3 ./scripts/generate-release-notes.py --version 0.23.0-dev.1 --channel dev --commit "$commit" --output ./test-release/RELEASE-NOTES.md
bash ./scripts/build-release-linux.sh dev 0.23.0-dev.1 ./test-release

echo "[8/9] bundle assembly and verification"
archive="$(find ./test-release -maxdepth 1 -name 'dragonforge-test-lab-0.23.0-dev.1-linux-*.tar.gz' -print -quit)"
python3 ./scripts/assemble-release.py   --channel dev   --version 0.23.0-dev.1   --git-commit "$commit"   --output-dir ./test-release/bundle   --artifact "linux_package::$archive"   --artifact "sbom::./test-release/dragonforge-test-lab-0.23.0-dev.1.cdx.json"   --artifact "audit_report::./test-release/audit-report.txt"   --artifact "release_notes::./test-release/RELEASE-NOTES.md"
cargo run -p dragonforge-test-lab -- release-bundle-verify --manifest ./test-release/bundle/release-bundle.json --root ./test-release/bundle

echo "[9/9] general doctor reports Phase 22"
general="$(cargo run -q -p dragonforge-test-lab -- doctor)"
echo "$general"
grep -q '^phase=22$' <<<"$general"

echo "Phase 22 Release Engineering validation passed."
echo "Log file: $log"
