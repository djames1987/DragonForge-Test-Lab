#!/usr/bin/env python3
import argparse, hashlib, json, pathlib, shutil

KINDS = {
    "windows_package", "linux_package", "sbom", "checksums",
    "audit_report", "release_notes"
}

def sha256(path):
    h=hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()

def safe_leaf(name):
    return name and name not in (".","..") and "/" not in name and "\\" not in name

def parse_artifact(spec):
    parts=spec.split("::")
    if len(parts) not in (2,3):
        raise SystemExit(f"invalid --artifact: {spec}")
    kind=parts[0]
    if kind not in KINDS:
        raise SystemExit(f"unsupported artifact kind: {kind}")
    path=pathlib.Path(parts[1]).resolve()
    signature=pathlib.Path(parts[2]).resolve() if len(parts)==3 and parts[2] else None
    if not path.is_file():
        raise SystemExit(f"artifact missing: {path}")
    if signature and not signature.is_file():
        raise SystemExit(f"signature missing: {signature}")
    return kind,path,signature

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--channel", choices=["dev","beta","stable"], required=True)
    ap.add_argument("--version", required=True)
    ap.add_argument("--git-commit", required=True)
    ap.add_argument("--output-dir", required=True)
    ap.add_argument("--artifact", action="append", default=[], required=True)
    args=ap.parse_args()

    if len(args.git_commit)!=40 or any(c not in "0123456789abcdefABCDEF" for c in args.git_commit):
        raise SystemExit("git commit must be a full 40-character SHA")
    out=pathlib.Path(args.output_dir).resolve()
    out.mkdir(parents=True, exist_ok=True)

    records=[]
    for spec in args.artifact:
        kind,path,signature=parse_artifact(spec)
        if args.channel=="stable" and kind in ("windows_package","linux_package") and not signature:
            raise SystemExit(f"stable package requires signature: {path.name}")
        if not safe_leaf(path.name):
            raise SystemExit(f"unsafe artifact filename: {path.name}")
        dest=out/path.name
        if path != dest:
            shutil.copy2(path,dest)
        sig_name=None
        if signature:
            if not safe_leaf(signature.name):
                raise SystemExit(f"unsafe signature filename: {signature.name}")
            sigdest=out/signature.name
            if signature != sigdest:
                shutil.copy2(signature,sigdest)
            sig_name=signature.name
        records.append({
            "kind":kind,
            "file":path.name,
            "sha256":sha256(dest),
            "signature_file":sig_name,
        })

    sums=out/"SHA256SUMS"
    with sums.open("w",encoding="utf-8",newline="\n") as f:
        for r in sorted(records,key=lambda r:r["file"]):
            f.write(f'{r["sha256"]}  {r["file"]}\n')
    records.append({
        "kind":"checksums",
        "file":"SHA256SUMS",
        "sha256":sha256(sums),
        "signature_file":None,
    })

    manifest={
        "schema_version":1,
        "version":args.version,
        "channel":args.channel,
        "git_commit":args.git_commit.lower(),
        "artifacts":records,
    }
    (out/"release-bundle.json").write_text(json.dumps(manifest,indent=2)+"\n",encoding="utf-8")
    print(f"Release bundle ready: {out}")

if __name__=="__main__":
    main()
