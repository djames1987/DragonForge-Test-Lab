#!/usr/bin/env bash
set -euo pipefail

runtime="${1:-docker}"
image="dragonforge/test-lab-rust:0.4.0"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dockerfile_dir="$root/containers/rust-worker"

case "$runtime" in
  docker|podman) ;;
  *) echo "runtime must be docker or podman" >&2; exit 2 ;;
esac

echo "Building DragonForge Test Lab sandbox image..."
echo "Runtime: $runtime"
echo "Image: $image"
"$runtime" build --pull -t "$image" "$dockerfile_dir"
"$runtime" image inspect "$image" >/dev/null
echo "Sandbox image ready: $image"
