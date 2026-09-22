# Phase 14 — Audit / Artifacts / Observability

Status: implementation complete; Windows host validation pending.

## Goal

Phase 14 makes Test Lab operations inspectable and retainable without weakening the typed execution boundary. It builds on Phase 11 durable SQLite state, Phase 12 authenticated transport, and Phase 13 long-running workers.

Phase 14 adds:

- controller schema v2;
- hash-chained durable audit events;
- durable redacted structured logs;
- durable metric samples;
- worker runtime metric snapshots;
- bounded JSONL log files with rotation;
- artifact SHA-256 cataloging;
- root-contained artifact retention pruning;
- artifact creation/retention metadata;
- bounded telemetry queries and pruning;
- operator doctor, fixture, and summary commands.

## New crate

    crates/df-test-observability

The observability crate owns typed structured-log validation/redaction, JSONL output/rotation, metric registry/snapshots, artifact hashing and retention, and audit digest construction.

## Controller schema v2

Schema v2 migrates the Phase 11 schema in place.

New durable data:

    structured_logs
    metric_samples

Extended durable data:

    artifact_metadata.created_at_secs
    artifact_metadata.retained_until_secs
    audit_events.previous_sha256
    audit_events.event_sha256

Existing Phase 11-13 databases at user_version=1 are migrated to user_version=2.

New databases still create schema v1 first and then apply the same v2 migration path, ensuring fresh and upgraded databases converge.

## Hash-chained audit

Existing controller lifecycle operations already produce audit events.

Phase 14 adds SHA-256 chaining for every newly written event:

    SQLite audit_id
      + previous event digest
      + timestamp
      + event kind
      + entity type/id
      + event detail
      -> SHA-256 digest

Migrated historical v1 events remain readable and have null digest fields. The first Phase 14 event begins a new verifiable chain using its real SQLite audit sequence.

The controller can verify all Phase 14 chained records with:

    verify_audit_chain()

Audit events are deliberately not removed by telemetry pruning.

## Structured logs

StructuredLogEvent contains:

- timestamp;
- level;
- component;
- bounded message;
- bounded JSON fields;
- optional job ID;
- optional worker ID.

Sensitive field names are redacted before JSONL or SQLite persistence. Current sensitive-name matching includes password/passwd, token, secret, private_key, and authorization.

Limits include:

    message <= 8 KiB
    fields <= 32
    serialized field <= 16 KiB

JSONL files support a configurable bounded file size and five retained rotations.

## Metrics

Metrics are typed finite numeric samples.

The controller persists metric samples in SQLite. The in-memory MetricsRegistry supports counters and gauges and deterministic text/snapshot output.

Phase 13 WorkerServiceRuntime now exposes metrics for:

    dragonforge_worker_active_jobs
    dragonforge_worker_accepting_jobs
    dragonforge_worker_draining
    dragonforge_worker_reconnect_attempt

This provides a stable source for the Phase 18 dashboard.

## Artifact catalog and retention

Artifacts can be cataloged only beneath a configured artifact root.

Cataloging:

- canonicalizes the root and artifact;
- rejects paths outside the root;
- rejects symlink artifacts;
- records relative path;
- records size;
- computes SHA-256;
- records creation time.

Retention supports:

- maximum artifact age;
- maximum total retained bytes;
- maximum retained artifact count.

Pruning considers oldest entries first and deletes only canonical regular files contained beneath the configured root.

Parent traversal and symlink deletion targets fail closed.

Controller artifact metadata also records creation time and an optional retained-until timestamp.

## Telemetry retention

Structured logs and metric samples can be pruned before an explicit timestamp.

Audit history is not included in telemetry pruning.

Artifact files use the separate root-contained artifact retention policy.

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- observability-doctor

End-to-end fixture:

    cargo run -p dragonforge-test-lab -- observability-fixture

Controller summary:

    cargo run -p dragonforge-test-lab -- observability-summary

Custom durable database:

    cargo run -p dragonforge-test-lab -- observability-summary --state-db <path>

## Validation

Run:

    .\scripts\test-phase14.ps1

Validation checks:

1. environment;
2. cargo fmt;
3. strict workspace Clippy;
4. full workspace tests/doc-tests;
5. schema v2 controller doctor;
6. observability doctor;
7. audit/log/metric/artifact fixture;
8. Phase 13 worker-service fixture;
9. Phase 12 mTLS fixture;
10. general doctor reports Phase 14;
11. GitHub-aware native regression.

The unit suite also validates schema v1 -> v2 migration.

## Security boundary

Phase 14 adds visibility, not execution authority.

It does not add:

- arbitrary SQL;
- arbitrary filesystem deletion;
- arbitrary artifact roots supplied by jobs;
- secret-bearing unredacted structured fields;
- inbound worker listeners;
- remote shell access;
- metric-controlled scheduling or execution.

Artifact deletion is root-contained and refuses symlinks/traversal.

Structured log fields are bounded and known sensitive names are redacted before persistence.

Audit chaining is tamper-evidence for new Phase 14 records; it is not a cryptographic signature or external immutable ledger.

## Exit criteria

Phase 14 is complete when validation proves:

- schema v1 migrates to v2;
- strict Clippy/workspace tests pass;
- new audit events form a valid hash chain;
- structured logs redact secrets;
- JSONL logging works within bounds;
- durable metrics round-trip;
- worker metrics reflect runtime state;
- artifact SHA-256 cataloging works;
- retention removes only eligible root-contained artifacts;
- telemetry pruning works;
- Phase 13 worker services remain green;
- Phase 12 mTLS remains green;
- GitHub-aware native execution remains green.
