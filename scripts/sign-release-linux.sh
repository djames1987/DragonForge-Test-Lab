#!/usr/bin/env bash
set -euo pipefail
artifact="${1:-}"
[[ -f "$artifact" ]] || { echo "usage: $0 <artifact>" >&2; exit 2; }
command -v minisign >/dev/null 2>&1 || { echo "minisign is required" >&2; exit 1; }
key="${DRAGONFORGE_MINISIGN_SECRET_KEY:-}"
[[ -n "$key" && -f "$key" ]] || { echo "DRAGONFORGE_MINISIGN_SECRET_KEY must point to a secret-key file" >&2; exit 1; }
minisign -S -s "$key" -m "$artifact" -x "$artifact.sig"
echo "Signature ready: $artifact.sig"
