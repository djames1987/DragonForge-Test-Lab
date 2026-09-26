# Third-Party Notices

This repository is proprietary DragonForge source under `LicenseRef-DragonForge-Proprietary`. That licensing applies only to original DragonForge material. Third-party software and material retain their own copyright and license terms and independently granted rights.

## Rust dependencies

The workspace uses Cargo-resolved Rust dependencies and does not vendor their source in the current repository tree. Direct dependencies include general-purpose serialization/error/identifier libraries, testing and benchmarking libraries, cryptographic primitives, SQLite bindings, and TLS/certificate tooling. Binary distributions may incorporate direct and transitive crates and must satisfy the licenses of the exact resolved graph in `Cargo.lock`.

Reproducible owner-side inventory command:

```powershell
cargo metadata --locked --format-version 1 > dependency-metadata.json
```

For release qualification, inspect every package whose `source` is non-null and record its `license` or `license_file`. A package with missing or uncertain licensing, or an SPDX expression not reviewed for the intended distribution, is a publication blocker. A compatible tool such as `cargo-deny` may also be used, but its configuration and output must be retained as release evidence rather than assumed.

Common permissive Rust license families such as MIT, Apache-2.0, BSD-style, ISC, and Unicode-related licenses may occur in Cargo graphs; this notice does not assert that a family is present unless the generated metadata for the release shows it. The exact resolved graph is authoritative.

## Container base image

`containers/rust-worker/Dockerfile` uses `rust:1.96.0-bookworm`. The image is obtained from its upstream registry rather than vendored in this source tree. Anyone redistributing a built worker image must preserve and satisfy the notices and licenses of the Rust official image, Debian Bookworm packages, and any additional material actually present in the resulting image. Source-repository publication alone does not grant or replace those upstream rights.

## Bundled material

No third-party font set, icon library, artwork collection, screenshot pack, generated vendor directory, npm dependency tree, Python package tree, or copied upstream source bundle was identified in the current repository tree during the Phase 3 inspection.

## Release rule

Before a public binary or container release, regenerate the Cargo dependency metadata from the committed lockfile and inventory the final container contents where applicable. Preserve required upstream notices/license texts with the distributed artifact. Missing or uncertain redistribution rights must block the affected artifact until resolved.

The separate Phase 2 publication blocker `DF-P2-TL-002` remains in force and is not waived by this notice.
