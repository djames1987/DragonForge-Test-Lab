# Roadmap

## Phase 0 — Core Foundation — Complete
Protocol, controller, agent, policy, manifests, tests, security model.

## Phase 1 — Local Rust Worker — Complete
Safe workspace creation, trusted repository checkout, fixed Cargo commands, bounded process execution, logs, artifacts, cancellation, cleanup, and local validation tooling.

## Phase 2 — GitHub Integration — Complete
GitHub repository/ref resolution to immutable commit SHAs, authenticated GitHub CLI integration, commit-status lifecycle reporting, GitHub-aware local execution, and validation tooling. GitHub Actions remains optional rather than being the Test Lab execution core.

## Phase 3 — Sandboxing — Complete
Dedicated worker-identity enforcement, race-resistant Windows Job Object containment, whole-tree timeout/cancellation, aggregate memory/process ceilings, Docker/Podman Cargo isolation, sandbox preflight, and validation tooling.

## Phase 4 — VM Lab — Implementation complete; host/lifecycle validation pending
Typed Hyper-V orchestration, Generation 2 Windows/Linux guest profiles, golden-image differencing disks, clean baseline checkpoints, rollback, managed lifecycle commands, host readiness tooling, and detailed setup documentation.

## Phase 5 — Deep Rust Testing
Coverage, nextest, Miri, sanitizers, fuzzing, benchmarks, property testing.

## Phase 6 — Windows Integration
Services, Event Log, registry, process/network fixtures, installers, permissions.

## Phase 7 — GUI Automation
Windows UI Automation, screenshots, crash capture, deterministic interaction scripts.

## Phase 8 — Multi-machine & Network Lab
Turn DragonForge Test Lab into a distributed capability-based test cluster. Authorized physical machines, VMs, container hosts, Raspberry Pi systems, Linux servers, Windows machines, and other supported hardware can register as worker nodes and receive only jobs matching their capabilities and policy. Add authenticated controller/agent transport, node lifecycle and health, capability/load-aware scheduling, artifact/result return, coordinated multi-node jobs, TCP/UDP/DNS fixtures, fault injection, hardware-in-the-loop support, and network-isolated test scenarios. Nodes should initiate authenticated outbound connections where practical; Test Lab must not become a general remote shell.

## Phase 9 — ChatGPT/MCP Gateway
Authenticated high-level tool surface for job submission, status, and artifact/result retrieval.

## Phase 10 — Test Intelligence
Change-aware profile selection, historical regression targeting, failure clustering, and resource-aware scheduling.
