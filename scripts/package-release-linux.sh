#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
output_root="${2:-./dist}"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: $0 <version> [output-root]" >&2
  exit 2
fi

arch="$(uname -m)"
case "$arch" in
  x86_64) rust_arch="x86_64" ;;
  aarch64|arm64) rust_arch="aarch64" ;;
  armv7l|armv8l) rust_arch="arm" ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

cargo build --release -p dragonforge-test-lab
binary="target/release/dragonforge-test-lab"
[[ -f "$binary" ]] || { echo "release binary missing" >&2; exit 1; }

package="$output_root/dragonforge-test-lab-$version-linux-$rust_arch"
rm -rf "$package"
mkdir -p "$package"
install -m 0755 "$binary" "$package/dragonforge-test-lab"
sha="$(sha256sum "$package/dragonforge-test-lab" | awk '{print $1}')"

cat > "$package/release-manifest.json" <<JSON
{
  "schema_version": 1,
  "version": "$version",
  "target_os": "linux",
  "target_arch": "$rust_arch",
  "binary_file": "dragonforge-test-lab",
  "binary_sha256": "$sha"
}
JSON

echo "Package ready: $package"
echo "SHA-256: $sha"
