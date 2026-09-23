#!/usr/bin/env bash
set -euo pipefail
channel="${1:-}"
release_version="${2:-}"
output_root="${3:-./dist}"
case "$channel" in
  dev|beta|stable) ;;
  *) echo "usage: $0 dev|beta|stable <release-version> [output-root]" >&2; exit 2 ;;
esac
base_version="${release_version%%-*}"
[[ "$base_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "invalid base version" >&2; exit 2; }

bash ./scripts/package-release-linux.sh "$base_version" "$output_root/packages"
arch="$(uname -m)"
case "$arch" in
  x86_64) rust_arch=x86_64 ;;
  aarch64|arm64) rust_arch=aarch64 ;;
  armv7l|armv8l) rust_arch=arm ;;
  *) echo "unsupported architecture" >&2; exit 1 ;;
esac
package_dir="$output_root/packages/dragonforge-test-lab-$base_version-linux-$rust_arch"
archive="$output_root/dragonforge-test-lab-$release_version-linux-$rust_arch.tar.gz"
mkdir -p "$output_root"
tar -C "$package_dir" -czf "$archive" .
if [[ "$channel" == stable && -n "${DRAGONFORGE_MINISIGN_SECRET_KEY:-}" ]]; then
  bash ./scripts/sign-release-linux.sh "$archive"
fi
echo "Release archive: $archive"
