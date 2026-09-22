# Phase 6 — Windows Integration

Status: complete; safe and privileged Windows validation passed on both the host and Windows VM on 2026-09-22.

## Goal

Phase 6 adds a typed Windows integration layer for operating-system surfaces that ordinary Rust unit tests cannot exercise directly: Service Control Manager state, Event Log access and writes, registry mutation/cleanup, process execution, loopback networking, Windows Installer discovery, MSI signature inspection, and privilege-sensitive validation.

The phase preserves the Test Lab rule that remote jobs do not contain arbitrary shell or PowerShell command text.

## Delivered

- New `df-test-windows` crate.
- `windows-doctor` CLI command.
- `windows-fixtures` safe fixture command.
- `windows-privileged-fixtures --confirm` elevated fixture command.
- `windows-installer-info --path <installer.msi>` signature inspection.
- Read-only Windows integration readiness script.
- Phase 6 validation script with uploadable transcript.
- HKCU registry create/read/delete round-trip.
- Direct child-process fixture using a fixed Windows executable.
- TCP loopback connect/accept/payload round-trip.
- UDP loopback payload round-trip.
- Service Control Manager create/query/delete fixture with managed DragonForge-TestLab-* names.
- Application Event Log write fixture using a DragonForge-TestLab source.
- Windows Installer service and msiexec discovery.
- Authenticode inspection for supplied MSI files.
- Explicit elevation detection and privileged-lane gating.

## Safe fixture lane

The ordinary lane does not require elevation and runs:

1. HKCU registry round-trip beneath a DragonForge-owned fixture key;
2. fixed direct child-process execution;
3. TCP loopback fixture;
4. UDP loopback fixture;
5. Windows integration doctor;
6. Windows Installer subsystem discovery;
7. GitHub-aware native worker regression.

Run:

    .\scripts\test-phase6.ps1

## Privileged fixture lane

The privileged lane mutates only temporary DragonForge-managed OS objects:

- creates a uniquely named `DragonForge-TestLab-Service-*` service entry;
- queries it through the Service Control Manager;
- deletes it;
- writes one informational Application Event Log entry with source `DragonForge-TestLab`.

The service is never started. The binary path is fixed internally to a Windows system binary and cannot be supplied by a job or CLI argument.

Run from an elevated PowerShell:

    .\scripts\test-phase6.ps1 -IncludePrivileged

The CLI itself also requires explicit confirmation:

    cargo run -p dragonforge-test-lab -- windows-privileged-fixtures --confirm

## Registry boundary

The registry fixture is restricted to:

    HKCU:\Software\DragonForge\TestLab\Fixtures\<generated-id>

The key is created, checked for an exact known value, and removed in a finally block. No HKLM mutation occurs in the safe lane.

## Process and network fixtures

Process validation launches a fixed Windows executable directly without a shell.

The network fixtures bind only to 127.0.0.1 on operating-system-selected ephemeral ports. They validate local socket behavior and do not open a listener on an external interface.

## Installer validation

The doctor verifies that the Windows Installer service exists and that `msiexec.exe` is available.

A supplied MSI can be inspected without installation:

    cargo run -p dragonforge-test-lab -- windows-installer-info --path C:\path\package.msi

The path must resolve to an existing file with an .msi extension. The command reports file size, Authenticode signature status, and signer subject when available. It does not install, repair, uninstall, or execute the MSI.

## Permission model

`windows-doctor` reports whether the process is elevated.

Safe fixtures are intended to work as a normal interactive user. Service creation/deletion and Event Log writes are separated into the privileged lane and require both:

- an elevated Windows token;
- explicit `--confirm` at the CLI boundary.

No credentials, passwords, alternate-user tokens, or impersonation are accepted.

## Validation

Host mandatory lane:

    .\scripts\test-phase6.ps1

Host privileged lane:

    .\scripts\test-phase6.ps1 -IncludePrivileged

Optional MSI inspection:

    .\scripts\test-phase6.ps1 -MsiPath C:\path\package.msi

Windows VM validation uses the same script from inside the prepared Windows golden/validation VM.

Validation completed successfully on both the Windows host and the prepared Windows VM on 2026-09-22. In both environments, the safe lane passed; the elevated lane also passed the transient Service Control Manager fixture and Application Event Log write fixture.
