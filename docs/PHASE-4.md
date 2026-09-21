# Phase 4 — VM Lab

Status: implementation complete; host and lifecycle validation pending.

## Goal

Phase 4 introduces a typed Hyper-V VM orchestration boundary so DragonForge Test Lab can create disposable Windows/Linux test machines from golden VHDX images, establish clean baselines, restore them before tests, and destroy managed VM instances without exposing arbitrary PowerShell.

## Delivered

- New df-test-vm crate.
- Hyper-V host doctor.
- Managed-VM namespace: only DragonForge-* names.
- Generation 2 VM creation.
- Windows and Linux guest profiles.
- Golden VHDX containment beneath a configured image root.
- Per-VM differencing disks created with New-VHD -Differencing.
- Configurable memory, vCPU, and virtual-switch selection.
- Standard Hyper-V checkpoints for deterministic lab rollback.
- Fixed DragonForge-Baseline clean checkpoint workflow.
- Typed start, stop, baseline, restore, and destroy operations.
- Partial-creation cleanup.
- Guarded destroy operation requiring --confirm.
- VM listing limited to managed names.
- Read-only host readiness script.
- Detailed Hyper-V host setup guide.
- Phase 4 validation script with optional full VM lifecycle test.

## Security boundary

The VM adapter invokes powershell.exe only with internally generated Hyper-V command templates.

Inputs that become PowerShell values are validated before use:

- VM names must begin with DragonForge- and contain a restricted character set.
- checkpoint names have a restricted character set.
- switch names have a restricted character set.
- memory and processor counts are bounded.
- base VHDX files must exist beneath the configured image root.
- VM destruction is namespace-limited and CLI-gated by --confirm.

The API does not accept arbitrary PowerShell scripts, cmdlet names, or PowerShell arguments.

## Disk model

Golden image:

    vm-lab/images/windows-base.vhdx

Managed instance:

    vm-lab/vms/DragonForge-Windows-Test-01/Virtual Hard Disks/os.vhdx

The instance disk is a differencing child whose parent is the golden image.

## Clean rollback model

    golden VHDX
       |
       v
    differencing VM
       |
       v
    per-instance configuration
       |
       v
    power off
       |
       v
    DragonForge-Baseline checkpoint
       |
       +--> destructive test
       |
       +--> restore baseline
       |
       +--> next destructive test

Microsoft documents Checkpoint-VM, Get-VMCheckpoint, and Restore-VMCheckpoint as the PowerShell checkpoint workflow. Managed VMs are explicitly set to Standard checkpoint type for deterministic test-state rollback.

## Guest execution

Phase 4 establishes the VM lifecycle and isolation boundary. It intentionally does not create a general-purpose host-to-guest remote shell.

Guest test execution should use a future authenticated Test Lab guest/worker transport, PowerShell Direct for narrowly typed Windows operations, or SSH through an explicit typed adapter. That boundary must preserve the no-arbitrary-shell policy.

## Host setup

Follow docs/HOST-SETUP-HYPERV.md before expecting VM lifecycle validation to succeed.

## Validation

Core:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs

Full lifecycle:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -BaseVhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx -GuestOs windows

Full lifecycle validation creates only DragonForge-Phase4-Validation and destroys it at the end.
