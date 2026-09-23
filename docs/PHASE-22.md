# Phase 22 — Release Engineering

Phase 22 turns the Phase 21 platform packages into versioned, auditable release bundles with explicit promotion channels and cryptographic signing gates.

## Delivered

- workspace version `0.23.0`;
- dedicated `df-test-release` crate;
- release bundle schema v1;
- dev, beta, and stable channel policy;
- validated release-version/tag shapes;
- bounded release artifact manifests;
- SHA-256 artifact verification;
- stable package signature enforcement;
- deterministic CycloneDX 1.6 SBOM generation from `Cargo.lock`;
- `cargo audit` vulnerability gate;
- `cargo deny` license/advisory/source gate and checked-in policy;
- deterministic release notes generation;
- Windows release archive builder;
- Linux release archive builder;
- Windows Authenticode signing helper;
- detached minisign package signing helper;
- release bundle/checksum assembler;
- bundle verification CLI;
- GitHub Actions release workflow;
- Windows and Linux Phase 22 validation scripts.

## Release channels

### dev

Format:

    0.23.0-dev.1

Dev releases are intended for engineering validation. Package signatures are optional, but checksums, SBOM, audit report, and release bundle verification remain required.

### beta

Format:

    0.23.0-beta.1

Beta releases are promotion candidates for broader qualification. Package signatures remain optional at the policy layer so beta qualification can run without production signing keys.

### stable

Format:

    0.23.0

Stable bundles require package signature metadata. Windows stable binaries are Authenticode-signed before archive creation, and both Windows/Linux release archives receive detached minisign signatures before bundle assembly.

Phase 21 installer manifests continue to use stable `major.minor.patch` versions. The Phase 22 outer bundle carries dev/beta channel identity, preserving the already-qualified installer upgrade/rollback ordering.

## Release CLI

Readiness:

    cargo run -p dragonforge-test-lab -- release-doctor

Deterministic fixture:

    cargo run -p dragonforge-test-lab -- release-fixture

Resolve a release tag:

    cargo run -p dragonforge-test-lab -- release-tag --channel stable --version 0.23.0
    cargo run -p dragonforge-test-lab -- release-tag --channel beta --version 0.23.0-beta.1
    cargo run -p dragonforge-test-lab -- release-tag --channel dev --version 0.23.0-dev.1

Verify a completed bundle:

    cargo run -p dragonforge-test-lab -- release-bundle-verify \
      --manifest <release-root>/release-bundle.json \
      --root <release-root>

## SBOM

Generate a CycloneDX 1.6 JSON SBOM:

    python3 ./scripts/generate-sbom.py \
      --version 0.23.0-beta.1 \
      --commit <full-git-sha> \
      --output ./dist/dragonforge-test-lab-0.23.0-beta.1.cdx.json

The generator enumerates locked Cargo packages, records package URL identifiers, registry/source metadata, and registry SHA-256 checksums when present.

## Dependency and license audits

Linux:

    bash ./scripts/release-audit-linux.sh ./dist/audit-report.txt --install-tools

Windows:

    .\scripts\release-audit-windows.ps1 -Output .\dist\audit-report.txt -InstallTools

The release audit requires both `cargo-audit` and `cargo-deny`. `deny.toml` restricts accepted license families and external dependency sources.

## Platform release archives

Linux dev/beta:

    bash ./scripts/build-release-linux.sh dev 0.23.0-dev.1 ./dist
    bash ./scripts/build-release-linux.sh beta 0.23.0-beta.1 ./dist

Windows dev/beta:

    .\scripts\build-release-windows.ps1 -Channel dev -ReleaseVersion 0.23.0-dev.1 -OutputRoot .\dist
    .\scripts\build-release-windows.ps1 -Channel beta -ReleaseVersion 0.23.0-beta.1 -OutputRoot .\dist

## Signing

### Windows Authenticode

Stable Windows builds require:

    DRAGONFORGE_WINDOWS_SIGN_CERT_PATH
    DRAGONFORGE_WINDOWS_SIGN_CERT_PASSWORD

Optional RFC3161 timestamping uses:

    DRAGONFORGE_WINDOWS_TIMESTAMP_URL

The PFX and password are never command-line parameters and must not be committed.

### Detached package signatures

`sign-release-linux.sh` uses minisign and:

    DRAGONFORGE_MINISIGN_SECRET_KEY

The variable points to an external secret-key file. The helper writes a detached `.sig` file next to the artifact.

The GitHub release workflow accepts base64-encoded signing material through repository/environment secrets, writes it only to the ephemeral runner temp directory, and fails stable publication when required secrets are absent.

## Release bundle

`assemble-release.py` copies validated release artifacts into one directory, emits `SHA256SUMS`, and writes `release-bundle.json`.

Artifact kinds are restricted to:

- `windows_package`;
- `linux_package`;
- `sbom`;
- `checksums`;
- `audit_report`;
- `release_notes`.

Stable Windows/Linux package entries must name a detached signature file.

## GitHub release workflow

`.github/workflows/release.yml` is manually dispatched with:

- channel;
- version;
- publish flag.

The workflow performs format/Clippy/tests, validates the channel/tag, runs dependency/license audits, generates an SBOM, builds Windows and Linux archives, applies stable signing gates, assembles and verifies the bundle, uploads the bundle artifact, and optionally creates the GitHub Release/tag.

Publishing is never automatic merely because code reaches `main`.

## Validation

Windows:

    .\scripts\test-phase22.ps1 -InstallTools

Linux:

    bash ./scripts/test-phase22-linux.sh --install-tools

The qualification scripts exercise formatting, strict Clippy, workspace tests, release doctor/fixture, channel policy, dependency/license audits, SBOM generation, dev release archive creation, release bundle assembly/verification, and the Phase 22 doctor marker.

Production stable signing cannot be honestly qualified without real signing identities. The deterministic fixture verifies that stable bundle policy rejects unsigned package metadata and accepts a signed fixture.

## Security boundary

Release inputs do not become arbitrary runtime commands. Channel names are enum-constrained, versions are validated, artifact names are safe leaf names, artifact count/file size are bounded, and SHA-256 is recomputed during bundle verification.

Signing secrets remain outside the repository. Stable workflows fail closed when required signing material is absent.

## Validation status

Implementation is complete. Windows host qualification passed on 2026-09-23 using `scripts/test-phase22.ps1`, covering rustfmt, strict Clippy, full workspace tests, release doctor/fixture, dev/beta/stable channel policy, cargo-audit/cargo-deny gates, CycloneDX SBOM generation, Windows dev release packaging, release bundle assembly/verification, and the Phase 22 doctor marker. Linux qualification and production stable signing verification remain pending.
