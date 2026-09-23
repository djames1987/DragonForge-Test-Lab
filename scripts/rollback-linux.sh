#!/usr/bin/env bash
set -euo pipefail

[[ $EUID -eq 0 ]] || { echo "rollback-linux.sh must run as root" >&2; exit 1; }

binary_path="/opt/dragonforge/test-lab/bin/dragonforge-test-lab"
state_root="/var/lib/dragonforge/test-lab"
backup_root="$state_root/backups"
state_file="$state_root/install-state.json"

[[ -f "$state_file" ]] || { echo "install state not found" >&2; exit 1; }

readarray -t values < <(python3 - "$state_file" <<'PY'
import json,sys
s=json.load(open(sys.argv[1]))
print(s.get("current_version",""))
print(s.get("previous_version") or "")
print(s.get("previous_binary_sha256") or "")
PY
)
current="${values[0]}"
previous="${values[1]}"
expected_hash="${values[2]}"
[[ -n "$previous" && -n "$expected_hash" ]] || { echo "no rollback target recorded" >&2; exit 1; }

backup="$backup_root/dragonforge-test-lab-$previous"
[[ -f "$backup" ]] || { echo "rollback binary missing: $backup" >&2; exit 1; }
actual_hash="$(sha256sum "$backup" | awk '{print $1}')"
[[ "$actual_hash" == "$expected_hash" ]] || { echo "rollback checksum mismatch" >&2; exit 1; }

systemctl stop dragonforge-test-worker.service 2>/dev/null || true
cp -a "$binary_path" "$backup_root/dragonforge-test-lab-$current.failed" 2>/dev/null || true
install -m 0755 "$backup" "$binary_path.new"
mv -f "$binary_path.new" "$binary_path"

if [[ -f "$backup_root/install-state-$previous.json" ]]; then
  cp -a "$backup_root/install-state-$previous.json" "$state_file.tmp"
  mv -f "$state_file.tmp" "$state_file"
else
  python3 - "$state_file.tmp" "$previous" "$expected_hash" <<'PY'
import json,sys
path,version,checksum=sys.argv[1:]
json.dump({
 "schema_version":1,
 "current_version":version,
 "previous_version":None,
 "current_binary_sha256":checksum,
 "previous_binary_sha256":None
},open(path,"w"),indent=2)
PY
  mv -f "$state_file.tmp" "$state_file"
fi

echo "Rolled back DragonForge Test Lab from $current to $previous."
