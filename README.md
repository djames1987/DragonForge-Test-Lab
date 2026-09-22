# DragonForge Test Lab

DragonForge Test Lab is a local-first, security-conscious test orchestration platform for deeper validation than conventional hosted CI can conveniently provide.

The long-term target is a distributed DragonForge engineering lab spanning Windows/Linux workers, Raspberry Pi and other physical nodes, containers, Hyper-V virtual machines, network fixtures, GUI automation, Rust fuzzing/sanitizers, hardware-in-the-loop testing, GitHub integration, and an authenticated ChatGPT/MCP gateway.

## Current status

Phase 11 — Durable Controller & State — Implementation complete

Phases 0-3 established the versioned protocol, controller/agent policy boundary, local Rust worker, GitHub integration, Windows Job Object containment, worker identity checks, and Docker/Podman isolation.

Phase 4 added typed Hyper-V VM orchestration and passed full Windows lifecycle validation on 2026-09-22. Phase 5 added deep Rust testing, Phase 6 added validated Windows OS integration fixtures, Phase 7 added validated GUI automation, Phase 8 added validated authenticated distributed execution, Phase 9 added the validated authenticated MCP gateway, and Phase 10 added validated deterministic Test Intelligence. Phase 11 adds SQLite-backed durable controller state, migrations, persisted jobs/attempts/workers/intelligence/artifact metadata/audit/configuration, and restart recovery that marks uncertain in-flight work interrupted.

## Workspace

    apps/
      dragonforge-test-lab/   operator CLI
    crates/
      df-test-protocol/       shared versioned contracts
      df-test-policy/         worker authorization policy
      df-test-agent/          worker-side trust boundary
      df-test-controller/     scheduling/controller core
      df-test-executor/       checkout/process/artifact execution
      df-test-github/         typed GitHub adapter
      df-test-sandbox/        native/container containment
      df-test-vm/             Hyper-V VM Lab orchestration
      df-test-windows/        Windows integration fixtures
      df-test-gui/            Windows GUI automation
      df-test-distributed/    distributed nodes, scheduling, transport, network fixtures
      df-test-mcp/            authenticated MCP HTTP gateway and typed tool surface
      df-test-intelligence/   change analysis, regression targeting, clustering, scheduling
    docs/
      ARCHITECTURE.md
      SECURITY.md
      HOST-SETUP-HYPERV.md
      PHASE-0.md
      PHASE-1.md
      PHASE-2.md
      PHASE-3.md
      PHASE-4.md
      PHASE-5.md
      PHASE-6.md
      PHASE-7.md
      PHASE-8.md
      PHASE-9.md
      PHASE-10.md
      PHASE-11.md
      ROADMAP.md
    scripts/
      check-hyperv-host.ps1
      test-phase1.ps1
      test-phase2.ps1
      test-phase3.ps1
      test-phase4.ps1
      test-phase5.ps1
      check-rust-deep-tools.ps1
      check-windows-integration.ps1
      test-phase6.ps1
      check-gui-host.ps1
      phase7-gui-fixture.ps1
      test-phase7.ps1
      check-distributed-host.ps1
      test-phase8.ps1
      check-mcp-host.ps1
      test-phase9.ps1
      test-phase10.ps1
      test-phase11.ps1

## Hyper-V host setup

Follow docs/HOST-SETUP-HYPERV.md before running VM lifecycle tests.

Read-only host readiness:

    .\scripts\check-hyperv-host.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -SwitchName "Default Switch"

VM Lab doctor:

    cargo run -p dragonforge-test-lab -- vm-doctor --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch"

## VM lifecycle

Create:

    cargo run -p dragonforge-test-lab -- vm-create --name DragonForge-Windows-Test-01 --guest-os windows --base-vhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images

Create clean baseline:

    cargo run -p dragonforge-test-lab -- vm-baseline --name DragonForge-Windows-Test-01

Restore baseline:

    cargo run -p dragonforge-test-lab -- vm-restore --name DragonForge-Windows-Test-01

Destroy:

    cargo run -p dragonforge-test-lab -- vm-destroy --name DragonForge-Windows-Test-01 --vm-root C:\DragonForge-Test-Lab-VMs --confirm

## Phase 4 validation

Core host/code validation:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs

Full VM lifecycle validation after a golden image is prepared:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -BaseVhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx -GuestOs windows

## Phase 5 deep Rust testing

Readiness:

    .\scripts\check-rust-deep-tools.ps1

Mandatory validation:

    .\scripts\test-phase5.ps1

Install/update required cargo tools and validate:

    .\scripts\test-phase5.ps1 -InstallTools

See docs/PHASE-5.md for Miri, sanitizer, fuzzing, benchmark, and coverage lanes.

## Phase 6 Windows integration

Read-only readiness:

    .\scripts\check-windows-integration.ps1

Safe host validation:

    .\scripts\test-phase6.ps1

Elevated service/Event Log validation:

    .\scripts\test-phase6.ps1 -IncludePrivileged

See docs/PHASE-6.md for fixture boundaries, installer inspection, and permission behavior.

## Phase 7 GUI automation

Readiness:

    .\scripts\check-gui-host.ps1

End-to-end validation:

    .\scripts\test-phase7.ps1

Run a typed plan:

    cargo run -p dragonforge-test-lab -- gui-run-plan --plan .\examples\phase7-plan.json

See docs/PHASE-7.md for the managed-window boundary, screenshots, deterministic plans, and crash capture.

## Phase 8 distributed/network lab

Readiness:

    .\scripts\check-distributed-host.ps1

Local Phase 8 validation:

    .\scripts\test-phase8.ps1

Distributed doctor:

    cargo run -p dragonforge-test-lab -- distributed-doctor

Authenticated scheduler/network fixtures:

    cargo run -p dragonforge-test-lab -- distributed-fixtures

For the real host-to-VM typed job probe and shared-secret setup, see docs/PHASE-8.md.

## Phase 9 ChatGPT/MCP gateway

Required environment:

    $env:DRAGONFORGE_MCP_TOKEN = "<at-least-32-random-characters>"
    $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = "https://github.com/djames1987/DragonForge-Test-Lab"

Readiness:

    .\scripts\check-mcp-host.ps1
    cargo run -p dragonforge-test-lab -- mcp-doctor

Start the loopback MCP endpoint:

    cargo run -p dragonforge-test-lab -- mcp-serve

End-to-end Phase 9 validation:

    .\scripts\test-phase9.ps1

See docs/PHASE-9.md for authentication, MCP protocol compatibility, typed tool schemas, job profiles, and artifact/result boundaries.

## Phase 10 Test Intelligence

Readiness:

    cargo run -p dragonforge-test-lab -- intelligence-doctor

Deterministic built-in fixture:

    cargo run -p dragonforge-test-lab -- intelligence-fixture

Analyze a JSON scenario:

    cargo run -p dragonforge-test-lab -- intelligence-analyze --input .\examples\phase10-intelligence.json

End-to-end Phase 10 validation:

    .\scripts\test-phase10.ps1

See docs/PHASE-10.md for profile scoring, history targeting, clustering, scheduling rules, and security boundaries.

## Phase 11 Durable Controller & State

Readiness and schema migration:

    cargo run -p dragonforge-test-lab -- controller-state-doctor

Restart-recovery fixture:

    cargo run -p dragonforge-test-lab -- controller-state-fixture

End-to-end Phase 11 validation:

    .\scripts\test-phase11.ps1

The default durable database is:

    .dragonforge-test-lab\controller.sqlite3

See docs/PHASE-11.md for schema, persistence records, restart behavior, audit metadata, and security boundaries.

## Security principle

DragonForge Test Lab is not a remote shell.

VM management is constrained to typed Hyper-V operations, managed DragonForge-* names, validated paths/resources, and explicit destructive confirmation. Phase 4 creates a disposable VM boundary but does not claim protection from hypervisor escape or provide arbitrary host-to-guest execution.

See docs/SECURITY.md, docs/HOST-SETUP-HYPERV.md, docs/PHASE-4.md, docs/PHASE-5.md, docs/PHASE-6.md, docs/PHASE-7.md, docs/PHASE-8.md, docs/PHASE-9.md, docs/PHASE-10.md, docs/PHASE-11.md, and docs/ROADMAP.md.
