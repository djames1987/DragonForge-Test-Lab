# DragonForge Test Lab

DragonForge Test Lab is a local-first, security-conscious test orchestration platform for deeper validation than conventional hosted CI can conveniently provide.

The long-term target is a reusable DragonForge engineering lab spanning Windows/Linux workers, containers, Hyper-V virtual machines, network fixtures, GUI automation, Rust fuzzing/sanitizers, hardware-in-the-loop testing, and an authenticated ChatGPT/MCP gateway.

## Phase 0

Phase 0 establishes the security and architectural foundation **without enabling arbitrary remote process execution**.

Current workspace:

```text
apps/
  dragonforge-test-lab/   operator CLI
crates/
  df-test-protocol/       shared versioned contracts
  df-test-policy/         worker authorization policy
  df-test-agent/          worker-side trust boundary
  df-test-controller/     scheduling/controller core
docs/
  ARCHITECTURE.md
  SECURITY.md
  PHASE-0.md
  ROADMAP.md
```

## Build

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p dragonforge-test-lab -- doctor
```

## Security principle

**The Test Lab is not a remote shell.**

Remote callers submit typed actions. Workers independently enforce repository allowlists, capability restrictions, protocol compatibility, and resource ceilings. Real process execution will only be added behind this boundary.

See [docs/SECURITY.md](docs/SECURITY.md) and [docs/ROADMAP.md](docs/ROADMAP.md).
