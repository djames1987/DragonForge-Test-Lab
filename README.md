# DragonForge Test Lab

DragonForge Test Lab is a local-first, security-conscious test orchestration platform for deeper validation than conventional hosted CI can conveniently provide.

The long-term target is a reusable DragonForge engineering lab spanning Windows/Linux workers, containers, Hyper-V virtual machines, network fixtures, GUI automation, Rust fuzzing/sanitizers, hardware-in-the-loop testing, and an authenticated ChatGPT/MCP gateway.

## Current status

Phase 1 — Local Rust Worker

The lab can perform a controlled local Rust validation run against an authorized HTTPS Git repository. It creates a per-job workspace, checks out a requested revision, runs fixed Cargo validation actions, captures bounded output and artifacts, enforces a total timeout and post-step disk ceiling, writes a JSON report, and cleans up the workspace.

The executor never accepts arbitrary shell text.

## Workspace

    apps/
      dragonforge-test-lab/   operator CLI
    crates/
      df-test-protocol/       shared versioned contracts
      df-test-policy/         worker authorization policy
      df-test-agent/          worker-side trust boundary
      df-test-controller/     scheduling/controller core
      df-test-executor/       local checkout/process/artifact execution
    docs/
      ARCHITECTURE.md
      SECURITY.md
      PHASE-0.md
      PHASE-1.md
      ROADMAP.md
    scripts/
      test-phase1.ps1

## Local validation

    .\scripts\test-phase1.ps1

For a full end-to-end checkout and local worker run:

    .\scripts\test-phase1.ps1 -RepositoryUrl https://github.com/djames1987/DragonForge-Test-Lab.git -Revision main

You can also invoke the CLI directly:

    cargo run -p dragonforge-test-lab -- doctor

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision main

## Security principle

DragonForge Test Lab is not a remote shell.

Remote callers submit typed actions. Workers independently enforce repository allowlists, capability restrictions, protocol compatibility, and resource ceilings. The Phase 1 executor maps those typed actions to fixed executable/argument combinations without invoking a shell.

See docs/SECURITY.md, docs/PHASE-1.md, and docs/ROADMAP.md.
