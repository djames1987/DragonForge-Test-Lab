#!/usr/bin/env python3
import argparse
import hashlib
import json
import pathlib
import tomllib
import uuid

def purl(name, version):
    return f"pkg:cargo/{name}@{version}"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--lockfile", default="Cargo.lock")
    parser.add_argument("--output", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()

    lock = pathlib.Path(args.lockfile).read_bytes()
    data = tomllib.loads(lock.decode("utf-8"))
    components = []
    for package in sorted(data.get("package", []), key=lambda p: (p["name"], p["version"])):
        item = {
            "type": "library",
            "name": package["name"],
            "version": package["version"],
            "bom-ref": purl(package["name"], package["version"]),
            "purl": purl(package["name"], package["version"]),
        }
        checksum = package.get("checksum")
        if checksum:
            item["hashes"] = [{"alg": "SHA-256", "content": checksum}]
        source = package.get("source")
        if source:
            item["properties"] = [{"name": "cargo:source", "value": source}]
        components.append(item)

    serial_seed = hashlib.sha256(lock + args.version.encode() + args.commit.encode()).hexdigest()
    serial = uuid.UUID(serial_seed[:32])
    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": "dragonforge-test-lab",
                "version": args.version,
                "properties": [
                    {"name": "dragonforge:git_commit", "value": args.commit}
                ],
            }
        },
        "components": components,
    }
    output = pathlib.Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(bom, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"SBOM ready: {output}")
    print(f"Components: {len(components)}")

if __name__ == "__main__":
    main()
