# Phase 11 — Durable Controller & State

Status: implementation complete; Windows host validation pending.

## Goal

Phase 11 converts controller state from process-lifetime memory into durable SQLite state so queued jobs, attempts, workers, intelligence history, artifact metadata, audit records, and controller configuration survive controller restarts.

Phase 11 does not implement automatic retries. In-flight jobs discovered after a restart are explicitly marked `interrupted`; retry policy and rescheduling belong to Phase 15.

## Storage

Default database:

    .dragonforge-test-lab/controller.sqlite3

The controller uses SQLite through `rusqlite` with the bundled SQLite feature so Test Lab does not require a separately installed SQLite runtime.

Current schema version:

    1

Schema changes are migration-driven through SQLite `PRAGMA user_version`. A database with a schema newer than the running Test Lab build fails closed.

## Persistent records

Phase 11 stores:

- jobs;
- job attempts;
- worker registrations and online/last-seen state;
- Test Intelligence report history;
- artifact metadata;
- append-oriented audit events;
- controller configuration.

Typed `JobRequest` and `WorkerRegistration` values are serialized as JSON inside bounded controller-owned tables. The database does not introduce command strings or executable arguments.

## Durable job states

Phase 11 uses controller persistence states:

    queued
    assigned
    running
    passed
    failed
    rejected
    cancelled
    interrupted

`interrupted` is deliberately a durable-controller state rather than an automatic retry decision.

## Restart recovery

On controller startup/recovery:

- queued jobs remain queued;
- terminal jobs remain terminal;
- assigned/running jobs are marked interrupted;
- their latest attempts are marked interrupted;
- an audit event records recovery;
- no job is automatically re-executed.

This avoids duplicate execution after uncertain controller/worker failures.

## Durable scheduling

The durable controller persists worker registrations and continues to require typed capability subsets before assigning queued jobs.

Assignment creates a persistent attempt record and audit event.

The persistence layer does not bypass worker-side Agent/Policy validation. The worker must still independently authorize any assigned job before execution.

## Artifacts and intelligence

Job completion can persist artifact metadata:

    name
    relative_path
    size_bytes
    sha256

The controller stores metadata only; artifact file storage/retention policy is expanded in Phase 14.

Phase 10 intelligence reports can be persisted as JSON history for later integration. Phase 17 will connect this durable history to real automatic/advisory intelligence decisions.

## CLI

Readiness / schema doctor:

    cargo run -p dragonforge-test-lab -- controller-state-doctor

Custom database path:

    cargo run -p dragonforge-test-lab -- controller-state-doctor --state-db C:\DragonForge-Test-Lab\state\controller.sqlite3

Restart-recovery fixture:

    cargo run -p dragonforge-test-lab -- controller-state-fixture

## Validation

Run:

    .\scripts\test-phase11.ps1

The validation performs:

1. environment inspection;
2. cargo fmt --check;
3. strict Clippy;
4. full workspace tests;
5. durable controller doctor/migration;
6. restart-recovery persistence fixture;
7. second-process reopen of the same SQLite database;
8. general doctor Phase 11 check;
9. GitHub-aware native worker regression.

The script writes:

    test-logs\phase11-validation-*.log

## Security boundary

SQLite is controller state, not an authorization source that overrides worker policy.

Phase 11 does not add:

- shell execution;
- arbitrary SQL supplied by clients;
- arbitrary filesystem browsing;
- worker impersonation;
- automatic retry;
- Internet-facing controller APIs;
- secret storage in job payloads.

Database paths are operator CLI configuration. Database SQL statements and migrations are fixed inside Test Lab.

## Exit criteria

Phase 11 is complete when Windows validation proves:

- schema migration succeeds;
- state persists across controller reopen;
- queued work remains queued;
- in-flight work becomes interrupted after restart;
- attempts/config/audit data persist;
- capability-aware assignment still works;
- the existing GitHub-aware worker regression remains green.
