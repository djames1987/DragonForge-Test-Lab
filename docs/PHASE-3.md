# Phase 3 — Sandboxing

Status: complete and locally validated on Windows with native Job Object containment.

## Goal

Phase 3 moves DragonForge Test Lab from bounded direct-child execution to explicit process-tree containment and resource enforcement while preserving the rule that Test Lab is not a remote shell.

## Delivered

- New `df-test-sandbox` crate.
- Windows-native Job Object containment.
- Race-resistant Windows launch sequence:
  1. create a per-command Job Object;
  2. configure memory/process ceilings and kill-on-close;
  3. spawn the child suspended;
  4. assign it to the Job Object;
  5. resume the child thread.
- Whole-tree termination on timeout/cancellation, including fixed-name Docker/Podman cleanup so runtime-client termination cannot leave an orphaned Test Lab container.
- Job Object close after leader exit so leftover descendants are reaped before output readers join.
- Aggregate Windows Job Object memory ceiling using the job's `max_memory_mib`.
- Windows active-process ceiling using the job's `max_processes`.
- Explicit Docker and Podman sandbox modes for Cargo project actions.
- Fixed project-owned container image: `dragonforge/test-lab-rust:0.4.0`, built from `rust:1.96.0-bookworm` with `rustfmt` and `clippy` explicitly installed.
- Container hardening:
  - `--cap-drop=ALL`;
  - `--security-opt=no-new-privileges`;
  - memory limit;
  - PID limit;
  - only the checked-out repository bind-mounted at `/workspace`.
- Dedicated worker-identity enforcement through `--worker-user <name>`.
- `sandbox-doctor` preflight command.
- Execution reports now record the selected sandbox mode.
- Phase 3 PowerShell validation with an uploadable timestamped transcript.

## Native Windows flow

    typed TestAction
      -> fixed git/cargo CommandSpec
      -> create Job Object
      -> apply memory/process/kill-on-close limits
      -> spawn command suspended
      -> assign process to Job Object
      -> resume child
      -> bounded stdout/stderr capture
      -> timeout/cancel => terminate Job Object tree
      -> leader exit => close Job Object and reap remaining descendants
      -> artifact/report

Each command receives a fresh Job Object. Descendants inherit Job Object membership unless Windows explicitly permits a nested/breakaway behavior; Test Lab does not set breakaway flags.

## Dedicated worker identity

Test Lab deliberately does not create Windows accounts or store account passwords.

A production worker can be run under a dedicated, non-administrator local account and started with:

    --worker-user DragonForgeTestLab

Before any repository execution, On Windows, Test Lab reads the actual process account through the operating system rather than trusting the `USERNAME` environment variable. It compares that identity with the required worker identity and fails closed on mismatch.

For local development, omitting `--worker-user` keeps the identity check optional. The Phase 3 validation script supplies the current account by default so the enforcement path itself is exercised.

## Container modes

Build the fixed sandbox image first:

    .\scripts\build-sandbox-image.ps1 -Runtime docker

Select Docker:

    --sandbox docker

For Podman, build the same Dockerfile with:

    .\scripts\build-sandbox-image.ps1 -Runtime podman

then select:

    --sandbox podman

Git checkout remains a fixed host operation. Cargo project actions are translated into a fixed container invocation; callers cannot provide an arbitrary runtime command or container image.

The container receives the same job memory and process ceilings as the native policy. The repository is mounted at `/workspace`; other host directories are not mounted by Test Lab.

## Network boundary

Phase 3 does not claim outbound network isolation. Native jobs retain the worker's normal network access. Docker/Podman use the runtime's normal networking because Rust dependency resolution may require registry access.

Network policy, isolated fixtures, and fault injection remain later-phase work.

## Platform behavior

- Windows + `native`: enforced with Job Objects.
- Windows + `docker`/`podman`: Job Object containment for the runtime client plus container memory/PID restrictions for project code.
- Non-Windows + `docker`/`podman`: container runtime is the project-code containment boundary.
- Non-Windows + `native`: fails closed until a native cgroup/process-group implementation is added.

This prevents Test Lab from silently presenting an unenforced native sandbox on platforms where it has not implemented one.

## Commands

Preflight native Windows sandboxing:

    cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox native

Require a specific worker identity:

    cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox native --worker-user DragonForgeTestLab

Run a repository with native containment:

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-3-sandboxing --sandbox native

Run Cargo actions in Docker:

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-3-sandboxing --sandbox docker

## Validation

On Windows:

    .\scripts\test-phase3.ps1 -RepositoryUrl https://github.com/djames1987/DragonForge-Test-Lab.git -Revision phase-3-sandboxing

The script writes a timestamped transcript beneath `test-logs` and exercises formatting, Clippy, unit/doc tests, normal/GitHub/sandbox doctors, worker-identity enforcement, native Job Object execution, and the GitHub-aware sandbox path.

## Security status

Phase 3 materially improves containment for DragonForge-owned/test code, but it is not the final hostile-code boundary. Phase 4 disposable VMs remain the preferred boundary for truly untrusted third-party code, kernel exploits, installer testing, privilege-boundary testing, or code intended to attack the host.
