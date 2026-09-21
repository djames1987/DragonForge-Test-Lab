# DragonForge Test Lab

DragonForge Test Lab is a local-first, security-conscious test orchestration platform for deeper validation than conventional hosted CI can conveniently provide.

The long-term target is a distributed DragonForge engineering lab spanning Windows/Linux workers, Raspberry Pi and other physical nodes, containers, Hyper-V virtual machines, network fixtures, GUI automation, Rust fuzzing/sanitizers, hardware-in-the-loop testing, GitHub integration, and an authenticated ChatGPT/MCP gateway.

## Current status

Phase 3 — Sandboxing

Phases 0-2 established the versioned protocol, capability-aware controller/agent boundary, secured local Rust worker, artifacts, and GitHub-aware immutable-commit validation. Phase 3 adds process-tree containment and resource enforcement.

On Windows native mode, each Test Lab command is created suspended, assigned to a Job Object with kill-on-close plus memory/process ceilings, then resumed. Timeout and cancellation terminate the whole contained tree rather than only the direct child.

Docker and Podman are also supported as explicit Cargo project-action sandbox modes. Test Lab supplies fixed container arguments and never accepts arbitrary shell text.

## Workspace

    apps/
      dragonforge-test-lab/   operator CLI
    crates/
      df-test-protocol/       shared versioned contracts
      df-test-policy/         worker authorization policy
      df-test-agent/          worker-side trust boundary
      df-test-controller/     scheduling/controller core
      df-test-executor/       checkout/process/artifact execution
      df-test-github/         typed GitHub CLI adapter and commit statuses
      df-test-sandbox/        Job Objects, identity checks, Docker/Podman wrapping
    docs/
      ARCHITECTURE.md
      SECURITY.md
      PHASE-0.md
      PHASE-1.md
      PHASE-2.md
      PHASE-3.md
      ROADMAP.md
    scripts/
      test-phase1.ps1
      test-phase2.ps1
      test-phase3.ps1

## Doctors

    cargo run -p dragonforge-test-lab -- doctor

    cargo run -p dragonforge-test-lab -- github-doctor

    cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox native

To require a dedicated worker account:

    cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox native --worker-user DragonForgeTestLab

## Sandboxed local validation

Native Windows Job Object:

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-3-sandboxing --sandbox native

Docker:

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-3-sandboxing --sandbox docker

Podman:

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-3-sandboxing --sandbox podman

For the complete Phase 3 Windows validation with an uploadable transcript:

    .\scripts\test-phase3.ps1 -RepositoryUrl https://github.com/djames1987/DragonForge-Test-Lab.git -Revision phase-3-sandboxing

## Security principle

DragonForge Test Lab is not a remote shell.

Callers submit typed operations. Workers independently enforce repository allowlists, capability restrictions, protocol compatibility, resource ceilings, sandbox mode, and optional worker identity. Phase 3 improves containment substantially, but disposable VMs remain the intended hostile-code boundary in Phase 4.

See docs/SECURITY.md, docs/PHASE-3.md, and docs/ROADMAP.md.
