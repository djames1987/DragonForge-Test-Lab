# DragonForge Test Lab

DragonForge Test Lab is a local-first, security-conscious test orchestration platform for deeper validation than conventional hosted CI can conveniently provide.

The long-term target is a distributed DragonForge engineering lab spanning Windows/Linux workers, Raspberry Pi and other physical nodes, containers, Hyper-V virtual machines, network fixtures, GUI automation, Rust fuzzing/sanitizers, hardware-in-the-loop testing, GitHub integration, and an authenticated ChatGPT/MCP gateway.

## Current status

Phase 5 — Deep Rust Testing — Implementation complete

Phases 0-3 established the versioned protocol, controller/agent policy boundary, local Rust worker, GitHub integration, Windows Job Object containment, worker identity checks, and Docker/Podman isolation.

Phase 4 added typed Hyper-V VM orchestration for disposable Windows/Linux test machines and passed full Windows lifecycle validation on 2026-09-22. Phase 5 adds nextest, LLVM coverage, property testing, Criterion benchmarks, Miri, sanitizers, and bounded cargo-fuzz support with explicit platform-aware validation lanes.

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
      ROADMAP.md
    scripts/
      check-hyperv-host.ps1
      test-phase1.ps1
      test-phase2.ps1
      test-phase3.ps1
      test-phase4.ps1
      test-phase5.ps1
      check-rust-deep-tools.ps1

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

## Security principle

DragonForge Test Lab is not a remote shell.

VM management is constrained to typed Hyper-V operations, managed DragonForge-* names, validated paths/resources, and explicit destructive confirmation. Phase 4 creates a disposable VM boundary but does not claim protection from hypervisor escape or provide arbitrary host-to-guest execution.

See docs/SECURITY.md, docs/HOST-SETUP-HYPERV.md, docs/PHASE-4.md, docs/PHASE-5.md, and docs/ROADMAP.md.
