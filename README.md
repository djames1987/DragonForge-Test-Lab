# DragonForge Test Lab

DragonForge Test Lab is a local-first, security-conscious test orchestration platform for deeper validation than conventional hosted CI can conveniently provide.

The long-term target is a distributed DragonForge engineering lab spanning Windows/Linux workers, Raspberry Pi and other physical nodes, containers, Hyper-V virtual machines, network fixtures, GUI automation, Rust fuzzing/sanitizers, hardware-in-the-loop testing, GitHub integration, and an authenticated ChatGPT/MCP gateway.

## Current status

Phase 2 — GitHub Integration

Phase 1 established the secured local Rust worker. Phase 2 adds a typed GitHub adapter that uses the authenticated GitHub CLI to resolve mutable refs to exact commit SHAs and report Test Lab commit statuses while keeping GitHub Actions optional.

The executor and GitHub adapter never accept arbitrary shell text.

## Workspace

    apps/
      dragonforge-test-lab/   operator CLI
    crates/
      df-test-protocol/       shared versioned contracts
      df-test-policy/         worker authorization policy
      df-test-agent/          worker-side trust boundary
      df-test-controller/     scheduling/controller core
      df-test-executor/       local checkout/process/artifact execution
      df-test-github/         typed GitHub CLI adapter and commit statuses
    docs/
      ARCHITECTURE.md
      SECURITY.md
      PHASE-0.md
      PHASE-1.md
      PHASE-2.md
      ROADMAP.md
    scripts/
      test-phase1.ps1
      test-phase2.ps1

## Local worker

    cargo run -p dragonforge-test-lab -- doctor

    cargo run -p dragonforge-test-lab -- run-local --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision main

## GitHub integration

Verify the GitHub CLI connection:

    cargo run -p dragonforge-test-lab -- github-doctor

Resolve a GitHub ref to an immutable commit, run Test Lab, and report status:

    cargo run -p dragonforge-test-lab -- run-github --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-2-github-integration

For the complete Phase 2 Windows validation with an uploadable transcript:

    .\scripts\test-phase2.ps1 -RepositoryUrl https://github.com/djames1987/DragonForge-Test-Lab.git -Revision phase-2-github-integration

## Security principle

DragonForge Test Lab is not a remote shell.

Callers submit typed operations. Workers independently enforce repository allowlists, capability restrictions, protocol compatibility, and resource ceilings. GitHub integration is similarly constrained to validated repository/ref resolution and typed commit-status operations.

See docs/SECURITY.md, docs/PHASE-2.md, and docs/ROADMAP.md.
