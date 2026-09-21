# Roadmap

## Phase 0 — Core Foundation
Protocol, controller, agent, policy, manifests, tests, security model.

## Phase 1 — Local Rust Worker
Safe workspace creation, trusted repository checkout, fixed Cargo commands, bounded process execution, logs, artifacts, cancellation, and cleanup.

## Phase 2 — GitHub Integration
Repository/commit job submission, status reporting, and optional GitHub Actions handoff.

## Phase 3 — Sandboxing
Dedicated Windows worker identity plus Docker/Podman isolation.

## Phase 4 — VM Lab
Hyper-V orchestration, clean snapshots, Windows/Linux test images, rollback.

## Phase 5 — Deep Rust Testing
Coverage, nextest, Miri, sanitizers, fuzzing, benchmarks, property testing.

## Phase 6 — Windows Integration
Services, Event Log, registry, process/network fixtures, installers, permissions.

## Phase 7 — GUI Automation
Windows UI Automation, screenshots, crash capture, deterministic interaction scripts.

## Phase 8 — Multi-machine & Network Lab
Coordinated TCP/UDP/DNS fixtures, fault injection, multi-agent scenarios.

## Phase 9 — ChatGPT/MCP Gateway
Authenticated high-level tool surface for job submission, status, and artifact/result retrieval.

## Phase 10 — Test Intelligence
Change-aware profile selection, historical regression targeting, failure clustering, and resource-aware scheduling.
