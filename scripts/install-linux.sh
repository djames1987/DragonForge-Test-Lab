#!/usr/bin/env bash
set -euo pipefail

package_root=""
manifest=""
worker_config=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package-root) package_root="$2"; shift 2 ;;
    --manifest) manifest="$2"; shift 2 ;;
    --worker-config) worker_config="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ $EUID -eq 0 ]] || { echo "install-linux.sh must run as root" >&2; exit 1; }
[[ -n "$package_root" ]] || { echo "missing --package-root" >&2; exit 2; }
[[ -n "$manifest" ]] || manifest="$package_root/release-manifest.json"
[[ -f "$manifest" ]] || { echo "manifest not found: $manifest" >&2; exit 1; }

mapfile -t verified < <(python3 - "$manifest" "$package_root" <<'PY'
import hashlib, json, pathlib, platform, re, sys
manifest_path = pathlib.Path(sys.argv[1])
root = pathlib.Path(sys.argv[2]).resolve()
raw = manifest_path.read_bytes()
if len(raw) > 1024 * 1024:
    raise SystemExit("manifest exceeds 1 MiB")
m = json.loads(raw)
if m.get("schema_version") != 1:
    raise SystemExit("unsupported manifest schema")
if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", str(m.get("version",""))):
    raise SystemExit("manifest version must be stable x.y.z")
if m.get("target_os") != "linux":
    raise SystemExit("manifest is not for Linux")
arch = platform.machine().lower()
expected = "aarch64" if arch in ("aarch64","arm64") else "arm" if arch in ("armv7l","armv8l") else "x86_64" if arch == "x86_64" else arch
if m.get("target_arch") != expected:
    raise SystemExit(f"architecture mismatch: manifest={m.get('target_arch')} host={expected}")
name = m.get("binary_file","")
if not re.fullmatch(r"[A-Za-z0-9._-]{1,255}", name) or name in (".",".."):
    raise SystemExit("unsafe binary filename")
binary = (root / name).resolve()
if binary.parent != root or not binary.is_file():
    raise SystemExit("release binary missing or escaped package root")
h = hashlib.sha256()
total = 0
with binary.open("rb") as stream:
    while True:
        chunk = stream.read(65536)
        if not chunk:
            break
        total += len(chunk)
        if total > 512 * 1024 * 1024:
            raise SystemExit("binary exceeds 512 MiB")
        h.update(chunk)
checksum = h.hexdigest()
if checksum.lower() != str(m.get("binary_sha256","")).lower():
    raise SystemExit("binary checksum mismatch")
print(m["version"])
print(binary)
print(checksum)
PY
)
version="${verified[0]}"
source_binary="${verified[1]}"
new_hash="${verified[2]}"

binary_dir="/opt/dragonforge/test-lab/bin"
binary_path="$binary_dir/dragonforge-test-lab"
config_root="/etc/dragonforge/test-lab"
state_root="/var/lib/dragonforge/test-lab"
log_root="/var/log/dragonforge/test-lab"
backup_root="$state_root/backups"
state_file="$state_root/install-state.json"
managed_config="$config_root/install-config.json"
unit_path="/etc/systemd/system/dragonforge-test-worker.service"

install -d -m 0755 "$binary_dir" "$config_root" "$state_root" "$log_root" "$backup_root"

previous_version=""
previous_hash=""
if [[ -f "$state_file" ]]; then
  previous_version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("current_version",""))' "$state_file")"
  previous_hash="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("current_binary_sha256",""))' "$state_file")"
fi

if [[ -n "$previous_version" && "$previous_version" != "unknown" ]]; then
  python3 - "$previous_version" "$version" <<'PY'
import sys
def parse(value):
    parts=value.split(".")
    if len(parts)!=3 or any(not p.isdigit() for p in parts):
        raise SystemExit(f"invalid installed version: {value}")
    return tuple(map(int, parts))
current,target=sys.argv[1:]
if parse(target) <= parse(current):
    raise SystemExit(f"upgrade target {target} must be newer than installed version {current}")
PY
fi

if [[ -x "$binary_path" ]]; then
  systemctl stop dragonforge-test-worker.service 2>/dev/null || true
  if [[ -z "$previous_version" ]]; then
    previous_version="unknown"
    previous_hash="$(sha256sum "$binary_path" | awk '{print $1}')"
  fi
  cp -a "$binary_path" "$backup_root/dragonforge-test-lab-$previous_version"
  [[ -f "$state_file" ]] && cp -a "$state_file" "$backup_root/install-state-$previous_version.json"
fi

install -m 0755 "$source_binary" "$binary_path.new"
mv -f "$binary_path.new" "$binary_path"

cat > "$managed_config.tmp" <<JSON
{
  "schema_version": 1,
  "preserve_state_on_uninstall": true,
  "config_root": "$config_root",
  "state_root": "$state_root",
  "log_root": "$log_root"
}
JSON
mv -f "$managed_config.tmp" "$managed_config"

python3 - "$state_file.tmp" "$version" "$new_hash" "$previous_version" "$previous_hash" <<'PY'
import json, sys
path, current, current_hash, previous, previous_hash = sys.argv[1:]
value = {
  "schema_version": 1,
  "current_version": current,
  "previous_version": previous or None,
  "current_binary_sha256": current_hash,
  "previous_binary_sha256": previous_hash or None,
}
with open(path, "w") as f:
    json.dump(value, f, indent=2)
    f.write("\n")
PY
mv -f "$state_file.tmp" "$state_file"

if [[ -n "$worker_config" ]]; then
  [[ -f "$worker_config" ]] || { echo "worker config not found: $worker_config" >&2; exit 1; }
  install -m 0600 "$worker_config" "$config_root/worker.json"
fi

cat > "$unit_path.tmp" <<UNIT
[Unit]
Description=DragonForge Test Worker
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=$binary_path worker-service-run --config $config_root/worker.json
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$state_root $log_root

[Install]
WantedBy=multi-user.target
UNIT
mv -f "$unit_path.tmp" "$unit_path"
systemctl daemon-reload

echo "DragonForge Test Lab $version installed."
echo "Binary: $binary_path"
echo "State: $state_root"
echo "Worker service is installed but is not enabled automatically."
