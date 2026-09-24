# Phase 24 — Reliability / Chaos

Status: implementation complete; local qualification pending.

## Goal

Phase 24 verifies that DragonForge Test Lab remains fail-closed, recoverable, and free from uncontrolled duplicate execution when important infrastructure components fail or become stale.

The phase deliberately uses **bounded deterministic fault scenarios**. It does not expose generic fault-injection commands, arbitrary process killing, raw disk corruption against operator data, network tampering against unrelated systems, or unrestricted shell access.

## New crate

    crates/df-test-chaos

Workspace version: `0.25.0`.

## Chaos scenarios

The `chaos-fixture` command runs all scenarios and returns a machine-readable report.

### Controller restart during an in-flight job

A durable controller is created on a temporary SQLite database. A job is assigned and marked running, then the controller is dropped without completing it.

After reopening:

- `recover_after_restart` must identify the job;
- the in-flight attempt becomes `interrupted`;
- the job is unassigned;
- an opted-in interrupted retry becomes `retry_pending`;
- the original attempt remains exactly one durable attempt.

This verifies recovery does not silently duplicate an in-flight execution.

### Duplicate assignment race

Two registered workers compete for one queued durable job at the same logical time.

Only one worker may obtain the job. The second assignment must return no job.

This exercises the controller's immediate SQLite transaction and state-transition guard.

### Stale distributed node / network lease

A signed node registration is accepted, then time advances beyond its lease.

The stale node:

- becomes offline;
- cannot satisfy a multi-node allocation;
- can return only after a fresh authenticated heartbeat.

### Replay fault

The same authenticated registration envelope is submitted twice.

The second envelope must be rejected by nonce replay protection.

### Worker restart

A worker enters drain mode and persists its runtime state.

After recovery:

- drain state survives;
- stale heartbeat time is cleared;
- the worker does not accept new jobs until explicitly resumed/reconnected.

### Reconnect storm

Repeated disconnect events increase reconnect attempts.

Backoff remains capped at 64 seconds rather than growing without bound.

### Database corruption

A temporary file containing invalid non-SQLite bytes is opened as a controller database.

The controller must return an error and must not recreate/overwrite the corrupt file as a fresh database.

### Disk/write failure

The worker runtime is pointed at a deliberately invalid state path whose parent is a regular file.

Persistence must return an error rather than pretending the state was stored.

The scenario is contained entirely beneath a disposable temporary directory.

### Certificate revocation

A certificate fingerprint is enrolled and verified, then revoked.

Verification after revocation must fail closed.

### High-volume durable stress

The controller queues a bounded number of jobs, assigns each exactly once, transitions each through running to passed, then verifies:

- every requested job completed;
- every job has exactly one attempt;
- the hash-chained audit history remains valid.

The default standalone fixture uses 250 jobs. Validation uses 1000 plus repeated 250-job recovery cycles.

Hard maximum:

    2000 jobs per fixture invocation

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- chaos-doctor

Default fixture:

    cargo run -p dragonforge-test-lab -- chaos-fixture

Larger bounded stress run:

    cargo run -p dragonforge-test-lab -- chaos-fixture --stress-jobs 1000

## Validation

Windows:

    .\scripts\test-phase24.ps1

Optional tuning:

    .\scripts\test-phase24.ps1 -StressJobs 1500 -RepeatIterations 5

Linux:

    bash ./scripts/test-phase24-linux.sh

The platform scripts run:

1. environment checks;
2. rustfmt;
3. strict Clippy;
4. full workspace tests;
5. chaos doctor;
6. 1000-job chaos fixture;
7. repeated 250-job recovery/stress cycles;
8. Phase 15 lifecycle regression;
9. Phase 13 worker-service regression;
10. Phase 8 distributed/network regression;
11. Phase 12 mTLS identity regression;
12. Phase 14 observability/audit regression;
13. focused chaos crate tests;
14. general doctor phase marker.

## Reliability invariants

Phase 24 requires:

- no automatic duplicate assignment of one durable queued job;
- no implicit replay of in-flight work after controller restart;
- interrupted retry only when policy opted in;
- stale distributed leases excluded from scheduling;
- authenticated replay rejected;
- worker drain state survives restart;
- reconnect delay remains bounded;
- corrupt controller databases fail closed;
- state-write failures are visible;
- revoked certificate identities fail closed;
- high-volume execution preserves one attempt per successful job;
- audit chaining remains valid after stress.

## Security boundary

Chaos testing is not a new execution authority.

The phase does not add a generic `kill-process`, `corrupt-file`, `drop-packet`, `run-command`, or remote chaos API.

All destructive inputs are synthesized inside disposable temporary directories or in-memory structures. Network failure is modeled through existing lease/replay/fault primitives. Existing Agent/Policy/Executor authorization remains unchanged.

## Exit criteria

Phase 24 is implementation-complete when:

- the chaos crate and CLI are integrated;
- controller/worker/network/disk/database/certificate scenarios are deterministic and bounded;
- the stress harness detects duplicate attempts or audit corruption;
- platform validation scripts are checked in;
- Phase 15/13/8/12/14 regressions remain part of qualification.

Full qualification requires a successful platform validation log.
