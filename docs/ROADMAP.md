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

## Phase 9 — ChatGPT/MCP Gateway — Complete
Loopback-only authenticated MCP HTTP gateway with modern 2026-07-28 discovery and legacy 2025-11-25 initialization compatibility, typed lab/node/job/result/artifact tools, repository allowlisting, asynchronous named-profile execution through the existing Agent/Policy/Executor trust boundary, bounded HTTP/JSON-RPC handling, SHA-256 artifact metadata, and end-to-end validation tooling. Windows host validation passed on 2026-09-22.

## Phase 10 — Test Intelligence — Complete
Deterministic change-aware profile scoring, historical regression targeting from changed-file overlap, normalized failure fingerprint clustering, explicit regression target selection, and resource-aware scheduling based on worker profile support, free memory, job slots, and load. Intelligence produces explainable recommendations only and never bypasses existing typed execution/policy boundaries. Windows host validation passed on 2026-09-22.

## Phase 11 — Durable Controller & State — Complete
Persistent controller state, SQLite migrations, restart recovery, durable jobs/attempts/workers/intelligence/artifact metadata/audit/configuration, and recovery validation. Windows host validation passed on 2026-09-22.

## Phase 12 — mTLS / Node Identity — Complete
Certificate-backed controller/worker identity, encrypted mutual-TLS transport primitives, enrollment, renewal with bounded overlap, revocation, serializable trust stores, SHA-256 certificate-to-node binding, private/local address policy, and key-rotation hooks. Legacy Phase 8 HMAC transport remains compatibility/private-lab mode. Windows host validation passed on 2026-09-22.

## Phase 13 — Worker Services — Complete
Native Windows SCM service hosting, hardened Linux systemd unit generation, outbound mTLS controller registration, typed heartbeats, graceful drain/resume, bounded reconnect backoff, non-secret restart-state recovery, and service diagnostics. Windows host validation passed on 2026-09-22.

## Phase 14 — Audit / Artifacts / Observability — Complete
Controller schema v2, hash-chained durable audit events, redacted structured logs, JSONL rotation, durable metrics, worker runtime metrics, SHA-256 artifact cataloging, root-contained retention pruning, artifact retention metadata, telemetry pruning, and operator query tooling. Windows host validation passed on 2026-09-22.

## Phase 15 — Recovery / Retry / Job Lifecycle — Implementation complete; validation pending
Controller schema v3, explicit failure classification, persisted bounded retry policies, retry-pending/exhausted states, due-time retry scheduling, interrupted restart decisions, manual interrupted-job rescheduling with a global attempt ceiling, retry cancellation, lifecycle query tooling, and audit integration.

## Phase 16 — Test Plans
Versioned declarative test plans for typed profiles, dependencies, conditions, timeouts, capabilities, artifacts, retries, target OS, and node labels.

## Phase 17 — Intelligence Integration
Connect Test Intelligence to real Git changes, durable historical failures, live worker capacity, advisory/automatic modes, and auditable decision explanations.

## Phase 18 — Dashboard
Local operator web dashboard for jobs, workers, test plans, artifacts, failure clusters, audit history, and settings without exposing arbitrary terminal access.

## Phase 19 — Linux Qualification
Full Linux worker validation including Rust execution, containers, service operation, encrypted transport, artifacts, cancellation, advanced Rust lanes, and recovery.

## Phase 20 — ARM / Raspberry Pi
Qualified ARM/Raspberry Pi worker support with typed hardware capabilities and safe hardware-in-the-loop operations.

## Phase 21 — Installer / Upgrades
Installable controller/worker packages, documented configuration/state/log layout, database/config migrations, upgrades, rollback, and uninstall support.

## Phase 22 — Release Engineering
Versioned release artifacts, checksums, SBOM, dependency/license audits, binary/installer signing, and dev/beta/stable release channels.

## Phase 23 — Security Review
Dedicated adversarial review of protocol, workers, paths, artifacts, MCP, certificates, transport, DoS bounds, secrets, logs, persistence, privileges, and installers.

## Phase 24 — Reliability / Chaos
Automated controller/worker/network/disk/database/certificate fault scenarios, long-running stress tests, and verification against lost state or uncontrolled duplicate execution.

## Phase 25 — Dogfooding
Use Test Lab as the normal validation platform for active DragonForge projects and eliminate routine manual validation where practical.

## Phase 26 — Release Candidate
Feature freeze, full qualification matrix, bug/security/reliability fixes only, and v1.0.0 release-candidate validation.

## Phase 27 — v1.0
Installable, persistent, recoverable, encrypted, observable, multi-platform Test Lab release with documented operations, upgrades, backup/restore, MCP/GitHub integration, and validated release artifacts.
