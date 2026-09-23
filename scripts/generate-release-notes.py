#!/usr/bin/env python3
import argparse, pathlib, subprocess

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--channel", choices=["dev","beta","stable"], required=True)
    ap.add_argument("--commit", required=True)
    ap.add_argument("--output", required=True)
    args=ap.parse_args()
    log=subprocess.run(
        ["git","log","--no-merges","--pretty=format:%h %s","-20",args.commit],
        check=True,capture_output=True,text=True
    ).stdout.strip()
    lines=[
        f"# DragonForge Test Lab {args.version}",
        "",
        f"Channel: **{args.channel}**",
        f"Commit: `{args.commit}`",
        "",
        "## Changes",
        "",
    ]
    lines += [f"- {line}" for line in log.splitlines() if line]
    lines += [
        "",
        "## Verification",
        "",
        "- Verify `SHA256SUMS` before installation.",
        "- Verify detached package signatures for stable releases.",
        "- Windows stable binaries additionally require Authenticode verification.",
        "- Review `audit-report.txt` and the CycloneDX SBOM before promotion.",
        "",
    ]
    out=pathlib.Path(args.output)
    out.parent.mkdir(parents=True,exist_ok=True)
    out.write_text("\n".join(lines),encoding="utf-8")
    print(f"Release notes ready: {out}")

if __name__=="__main__":
    main()
