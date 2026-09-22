# Roadmap

## Phase 0 — Core Foundation — Complete
Protocol, controller, agent, policy, manifests, tests, security model.

## Phase 1 — Local Rust Worker — Complete
Safe workspace creation, trusted repository checkout, fixed Cargo commands, bounded process execution, logs, artifacts, cancellation, cleanup, and local validation tooling.

## Phase 2 — GitHub Integration — Complete
GitHub repository/ref resolution to immutable commit SHAs, authenticated GitHub CLI integration, commit-status lifecycle reporting, GitHub-aware local execution, and validation tooling. GitHub Actions remains optional rather than being the Test Lab execution core.

## Phase 3 — Sandboxing — Complete
Dedicated worker-identity enforcement, race-resistant Windows Job Object containment, whole-tree timeout/cancellation, aggregate memory/process ceilings, Docker/Podman Cargo isolation, sandbox preflight, and validation tooling.

## Phase 4 — VM Lab — Complete
Typed Hyper-V orchestration, Generation 2 Windows/Linux guest profiles, golden-image differencing disks, clean baseline checkpoints, rollback, managed lifecycle commands, host readiness tooling, and detailed setup documentation. Full Windows Hyper-V lifecycle validation passed on 2026-09-22.

## Phase 5 — Deep Rust Testing — Complete
Nextest workspace execution, LLVM coverage, deterministic property tests, Criterion benchmark targets, platform-neutral Miri checks, Linux sanitizer lanes, bounded cargo-fuzz targets, readiness tooling, and uploadable validation logs. Mandatory Windows host validation passed on 2026-09-22.

## Phase 6 — Windows Integration — Complete
Typed Windows doctor and fixtures for registry, processes, TCP/UDP loopback networking, Service Control Manager, Application Event Log writes, Windows Installer discovery, MSI Authenticode inspection, elevation detection, explicit privileged confirmation, readiness tooling, and uploadable validation logs. Safe and privileged validation passed on both the Windows host and Windows VM on 2026-09-22.

## Phase 7 — GUI Automation — Complete
Typed Windows UI Automation for DragonForge-managed windows, deterministic JSON interaction plans, ValuePattern/InvokePattern actions, bounded assertions, window screenshots, artifact containment, deterministic WPF fixture application, owned-process crash/exit capture, readiness tooling, and uploadable validation logs. Host and Windows VM validation passed on 2026-09-22.

## Phase 8 — Multi-machine & Network Lab — Complete
Authenticated outbound-only node registration, replay-protected HMAC envelopes, lease/heartbeat health, OS/architecture/label/feature capability inventory, load-aware distinct-node scheduling, coordinated typed multi-node role plans, signed result return with SHA-256 artifact manifests, bounded framed transport, TCP/UDP/DNS fixtures, deterministic fault profiles, and a real host-to-node typed network-job probe. Public controller addresses, inbound agent listeners, and arbitrary remote commands remain forbidden. Host, Windows VM, and real cross-node validation passed on 2026-09-22.

## Phase 9 — ChatGPT/MCP Gateway — Implementation complete; validation pending
Loopback-only authenticated MCP HTTP gateway with modern 2026-07-28 discovery and legacy 2025-11-25 initialization compatibility, typed lab/node/job/result/artifact tools, repository allowlisting, asynchronous named-profile execution through the existing Agent/Policy/Executor trust boundary, bounded HTTP/JSON-RPC handling, SHA-256 artifact metadata, and end-to-end validation tooling.

## Phase 10 — Test Intelligence
Change-aware profile selection, historical regression targeting, failure clustering, and resource-aware scheduling.
