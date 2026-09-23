# Phase 21 — Installer / Upgrades

Phase 21 makes DragonForge Test Lab installable as a managed Windows or Linux application and establishes a bounded upgrade/rollback contract.

## Delivered

- dedicated `df-test-install` crate;
- workspace version 0.22.0;
- fixed Windows and Linux install layouts;
- SHA-256 verified release manifests;
- bounded manifest/config/state parsing;
- installer-owned configuration schema v1 with migration from the legacy unversioned form;
- versioned install state with immediate-previous rollback metadata;
- stable semantic-version upgrade ordering;
- Linux and Windows release packagers;
- Linux and Windows install/upgrade scripts;
- Linux and Windows immediate rollback scripts;
- Linux and Windows uninstall scripts;
- state-preserving uninstall by default with explicit purge;
- `install-doctor`, `install-fixture`, `install-layout`, `release-verify`, and `upgrade-plan` CLI commands;
- Phase 21 validation scripts.

## Stable filesystem layout

### Linux

- binary: `/opt/dragonforge/test-lab/bin/dragonforge-test-lab`
- configuration: `/etc/dragonforge/test-lab`
- durable state: `/var/lib/dragonforge/test-lab`
- logs: `/var/log/dragonforge/test-lab`
- backups: `/var/lib/dragonforge/test-lab/backups`
- worker service: `dragonforge-test-worker.service`

### Windows

- binary: `%ProgramFiles%\DragonForge\Test Lab\dragonforge-test-lab.exe`
- configuration: `%ProgramData%\DragonForge\Test Lab\config`
- durable state: `%ProgramData%\DragonForge\Test Lab\state`
- logs: `%ProgramData%\DragonForge\Test Lab\logs`
- backups: `%ProgramData%\DragonForge\Test Lab\backups`
- worker service: `DragonForgeTestWorker`

## Package and manifest

Packages contain one fixed binary plus `release-manifest.json`. The manifest binds:

- schema version;
- release version;
- target OS;
- target architecture;
- binary leaf filename;
- SHA-256 digest.

Installers reject path traversal, target mismatches, malformed versions, oversized inputs, and checksum mismatches before replacing an installed binary.

## Upgrade model

An upgrade must target a strictly newer stable `major.minor.patch` version. Before replacement, the installer stops the worker service if present and stores the previous binary plus install-state metadata in the fixed backup directory. Only one immediate rollback generation is authoritative in install state.

Phase 21 does not change the controller database schema. Existing controller SQLite migrations remain owned by `DurableController`, while installer-owned configuration gains schema v1 and a bounded legacy-to-v1 migration. Because 0.21.0 and 0.22.0 share the same controller schema, the Phase 21 rollback path is database-compatible.

Future releases that add irreversible database migrations must explicitly block binary rollback or add database backup/restore support before changing this guarantee.

## Linux

Build a package:

    bash ./scripts/package-release-linux.sh 0.22.0

Install or upgrade from an elevated shell:

    sudo bash ./scripts/install-linux.sh \
      --package-root ./dist/dragonforge-test-lab-0.22.0-linux-x86_64

Optionally install/update worker configuration at the same time:

    sudo bash ./scripts/install-linux.sh \
      --package-root <package-dir> \
      --worker-config ./worker.json

Rollback:

    sudo bash ./scripts/rollback-linux.sh

Uninstall while preserving config/state/logs/backups:

    sudo bash ./scripts/uninstall-linux.sh

Purge:

    sudo bash ./scripts/uninstall-linux.sh --purge

## Windows

Build a package:

    .\scripts\package-release-windows.ps1 -Version 0.22.0

From elevated PowerShell, install or upgrade:

    .\scripts\install-windows.ps1 -PackageRoot <package-dir>

Optionally install/update the native worker service configuration:

    .\scripts\install-windows.ps1 -PackageRoot <package-dir> -WorkerConfig .\worker.json

Rollback:

    .\scripts\rollback-windows.ps1

Uninstall preserving data:

    .\scripts\uninstall-windows.ps1

Purge:

    .\scripts\uninstall-windows.ps1 -Purge

## Service behavior

Installers may create/update the worker service definition when worker configuration is supplied, but they do not automatically start the service. This avoids accidentally connecting a newly upgraded worker before its operator has reviewed configuration and certificates.

The installed service continues to use the existing outbound-only mTLS worker runtime. Phase 21 adds no inbound administration endpoint or remote shell.

## Validation

Cross-platform deterministic installer fixture:

    cargo run -p dragonforge-test-lab -- install-fixture

Installer layout doctor:

    cargo run -p dragonforge-test-lab -- install-doctor

Windows validation:

    .\scripts\test-phase21.ps1

Linux validation:

    bash ./scripts/test-phase21-linux.sh

## Security boundary

Installer inputs do not become executable command text. Release binary names must be safe leaf names, package binaries are SHA-256 verified, target OS/architecture are checked, installer metadata is bounded, and production paths are fixed.

Uninstall preserves configuration, durable state, logs, and backups unless purge is explicitly requested.

## Validation status

Implementation is complete. Windows host validation passed on 2026-09-23 using `scripts/test-phase21.ps1`, covering rustfmt, strict Clippy, full workspace tests, installer doctor/fixture, Windows release packaging, release verification, and the Phase 21 doctor marker. Windows VM lifecycle qualification also passed on 2026-09-23: fresh managed 0.21.0 install, 0.21.0 → 0.22.0 upgrade, verified rollback metadata/backups, rollback to 0.21.0, repeat upgrade to 0.22.0, state-preserving uninstall, and explicit purge. Linux host validation remains pending before Phase 21 is fully platform-qualified.
