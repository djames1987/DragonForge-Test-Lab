# Phase 0 — Core Foundation

Status: implemented on the Phase 0 feature branch.

## Goals

Phase 0 establishes the contracts and security boundaries required before the lab is allowed to execute real test processes.

## Delivered

- Rust workspace with separated controller, agent, protocol, and policy crates.
- Versioned protocol contract.
- Typed test actions and capabilities.
- Repository allowlist enforcement.
- Worker capability enforcement.
- Resource ceilings.
- Capability-aware FIFO scheduling.
- Initial operator CLI with `doctor`.
- Project test manifest seed.
- Unit tests for protocol, scheduler, and policy behavior.
- GitHub CI for formatting, Clippy, and workspace tests.
- Architecture and security documentation.

## Explicitly deferred

- cloning repositories;
- spawning Cargo or arbitrary child processes;
- Windows services;
- Docker/Podman;
- Hyper-V;
- remote networking;
- GUI automation;
- GitHub App credentials;
- MCP/ChatGPT connectivity.

Deferring execution is intentional: Phase 1 will introduce real local execution behind the Phase 0 policy boundary rather than mixing transport, execution, and authorization in the first implementation.

## Exit criteria

Phase 0 is complete when the workspace builds, formatting and Clippy pass, tests pass, and the security boundary remains typed/no-arbitrary-shell.
