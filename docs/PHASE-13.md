# Phase 13 — Worker Services

Status: complete; Windows host validation passed on 2026-09-22.

## Goal

Phase 13 turns the previously command-driven worker into a service-oriented runtime that can start automatically, reconnect to the controller, maintain authenticated outbound presence, send typed heartbeats, drain gracefully, and recover non-secret runtime state after restart.

## New crate

    crates/df-test-worker-service

The worker-service crate owns:

- validated service configuration;
- outbound mTLS controller sessions;
- typed registration and heartbeat frames;
- service lifecycle state;
- graceful drain/resume;
- bounded reconnect backoff;
- non-secret runtime snapshot persistence;
- native Windows Service launch metadata;
- hardened systemd unit generation;
- deterministic service fixtures.

## Worker lifecycle

    starting
      -> connecting
      -> online
      -> draining
      -> offline/reconnect

The worker never listens for inbound controller connections.

A service restart does not claim that the previous network session is still online. Persisted state is reopened as connecting or draining and must establish a new authenticated session.

## Outbound mTLS

Phase 13 consumes the Phase 12 identity layer.

Configuration contains paths to:

- CA certificate PEM;
- worker client certificate PEM;
- worker private-key PEM.

The runtime loads those files locally and creates a rustls client configuration. Private keys are not copied into the persisted runtime snapshot.

Registration sends a typed WorkerServiceHello after the TLS handshake. Heartbeats send a typed WorkerHeartbeat.

## Graceful drain

Drain mode immediately prevents new work from being accepted while preserving the count of already active jobs.

A running worker refreshes the persisted drain/resume control before each heartbeat, so the CLI control commands affect the live service without requiring a restart. The worker remains in draining state until the service is resumed or stopped.

CLI:

    worker-service-drain --config <worker.json>
    worker-service-resume --config <worker.json>

## Restart recovery

The persisted runtime snapshot contains:

- worker ID;
- service state;
- active job count;
- drain request;
- last heartbeat timestamp;
- reconnect attempt count.

On restart:

- online is never trusted as still online;
- normal workers resume in connecting;
- draining workers remain draining;
- active-job count is retained as uncertain operational state;
- certificates/private keys are never persisted in the runtime snapshot.

## Reconnect policy

Connection failures increment a bounded reconnect attempt counter.

Current delay progression is exponential and capped:

    2, 4, 8, 16, 32, 64 seconds

A successful connection resets the counter.

## Windows Service

The operator binary now contains a native Windows Service Control Manager entry point using the windows-service crate.

Command:

    dragonforge-test-lab worker-service-windows --config <worker.json>

The service:

- registers with the SCM;
- reports Running;
- accepts Stop;
- sets a shared stop flag;
- moves the worker into drain state;
- persists state;
- reports Stopped.

The generated WindowsServiceSpec launches the native `worker-service-windows` SCM dispatcher and uses the fixed service name:

    DragonForgeTestWorker

and automatic startup metadata with restart delays of 5, 15, and 60 seconds.

Phase 21 will package/install/upgrade this service automatically. Phase 13 provides the service-capable executable and validated specification.

## Linux systemd

The generated unit launches:

    dragonforge-test-lab worker-service-run --config <worker.json>

with:

- Restart=on-failure;
- RestartSec=5;
- NoNewPrivileges=true;
- PrivateTmp=true;
- ProtectSystem=strict;
- ProtectHome=true.

Phase 19 performs full Linux qualification and Phase 21 provides installer/upgrade automation.

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- worker-service-doctor

Real service fixture:

    cargo run -p dragonforge-test-lab -- worker-service-fixture

Foreground worker:

    cargo run -p dragonforge-test-lab -- worker-service-run --config <worker.json>

One-shot connection:

    cargo run -p dragonforge-test-lab -- worker-service-run --config <worker.json> --once

Windows SCM host:

    dragonforge-test-lab worker-service-windows --config <worker.json>

Generate Windows/systemd service specs:

    cargo run -p dragonforge-test-lab -- worker-service-specs --executable <absolute-path> --config <worker.json>

## Validation

Run:

    .\scripts\test-phase13.ps1

The validation checks:

1. cargo fmt;
2. strict Clippy;
3. full workspace tests;
4. real mTLS registration/heartbeat worker fixture;
5. drain/restart persistence behavior;
6. Windows Service/systemd specs;
7. Phase 12 identity regression;
8. general doctor Phase 13;
9. GitHub-aware native worker regression.

## Security boundary

Phase 13 does not add:

- inbound worker listeners;
- arbitrary service command strings from jobs;
- public controller targets;
- plaintext private-key persistence;
- shell command execution;
- automatic retry of interrupted test jobs.

The service layer only maintains worker presence and lifecycle. Actual jobs still require the existing typed Agent/Policy/Executor authorization path.

## Exit criteria

Phase 13 is complete when host validation proves:

- the native Windows service code compiles;
- real mTLS service registration succeeds;
- typed heartbeat delivery succeeds;
- drain blocks new work;
- restart state is recovered safely;
- Windows and systemd service specifications are valid;
- Phase 12 mTLS remains green;
- existing GitHub-aware execution remains green.


## Validation status

Phase 13 validation completed successfully on the Windows host on 2026-09-22. The final run passed formatting, strict Clippy, the full workspace and doc-test suite, all worker-service unit tests, real mTLS worker registration/heartbeat, graceful drain behavior, restart-state recovery, Windows/systemd service-spec checks, Phase 12 mTLS compatibility, the Phase 13/general doctors, and the GitHub-aware native worker regression.
