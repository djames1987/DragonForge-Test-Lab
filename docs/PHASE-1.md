# Phase 1 — Local Rust Worker

Status: complete and locally validated on Windows. Hosted GitHub Actions remain unavailable until a self-hosted runner is connected.

## Goal

Phase 1 turns the Phase 0 contracts into a useful local worker while preserving the rule that DragonForge Test Lab is not an unrestricted remote shell.

## Delivered

- Per-job workspaces beneath a configured lab root.
- HTTPS Git repository checkout through fixed git argument vectors.
- Detached revision checkout after strict revision validation.
- Fixed Cargo actions for formatting, Clippy, tests, and builds.
- Total job timeout enforcement.
- Cooperative cancellation tokens.
- Bounded stdout/stderr retention while continuing to drain child pipes.
- A sanitized child-process environment.
- Per-step stdout/stderr artifact logs.
- A JSON execution report.
- Workspace disk-usage checks after completed steps.
- Automatic workspace cleanup with an opt-in retain flag.
- A doctor command that verifies Git and Cargo availability.
- A run-local command for end-to-end local validation.
- A Windows PowerShell validation script.

## Security boundary

The executor accepts TestAction values, not command strings. Executable names and argument arrays are constructed internally. It never invokes cmd.exe, PowerShell, sh, or another command shell.

Repository URLs are authorized by the Phase 0 policy before execution. The local CLI allowlists only the repository URL supplied for that run.

Repository revisions must be non-empty, no longer than 256 characters, must not begin with a dash, and may contain only ASCII letters, digits, dot, underscore, slash, and dash.

The child environment is cleared and rebuilt from a narrow set of ordinary toolchain/environment variables. Job payloads are not copied into environment variables.

## Current limits

Phase 1 intentionally does not claim strong OS sandboxing.

- Timeout cancellation terminates the directly spawned process. Full descendant process-tree containment is a Phase 3 target.
- Memory and process-count values are authorization ceilings but are not yet kernel-enforced.
- Disk usage is checked after steps rather than enforced as a live filesystem quota.
- Git and Cargo use the host's normal network access.
- Private repository checkout depends on Git authentication already configured on the worker.

## Run locally

    cargo run -p dragonforge-test-lab -- doctor

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision main

Or on Windows:

    .\scripts\test-phase1.ps1

For a full end-to-end repository checkout/test:

    .\scripts\test-phase1.ps1 -RepositoryUrl https://github.com/djames1987/DragonForge-Test-Lab.git -Revision main

Artifacts are written beneath .dragonforge-test-lab/artifacts/<job-id>/.
