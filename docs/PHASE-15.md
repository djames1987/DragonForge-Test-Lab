# Phase 15 — Recovery / Retry / Job Lifecycle

Status: complete; Windows host validation passed on 2026-09-22.

## Goal

Phase 15 makes job failure and recovery behavior explicit, durable, bounded, and auditable.

It adds:

- explicit failure classifications;
- persisted retry policies;
- controller schema v3;
- retry-pending and retry-exhausted lifecycle states;
- bounded exponential retry delay;
- due-time-aware retry scheduling;
- attempt-level failure classification;
- restart recovery decisions;
- explicit manual interrupted-job rescheduling;
- cancellation of scheduled retries;
- lifecycle status/query tooling.

## New crate

    crates/df-test-lifecycle

The lifecycle crate contains the policy engine only. It does not execute jobs.

## Failure classification

Supported classes:

    test_failure
    infrastructure_transient
    infrastructure_permanent
    policy_rejected
    cancelled
    interrupted

Automatic retry behavior is deliberately narrow:

- test_failure: never automatically retried;
- infrastructure_transient: retryable when attempts remain;
- infrastructure_permanent: terminal;
- policy_rejected: terminal;
- cancelled: terminal;
- interrupted: retryable only when retry_interrupted=true.

The controller's legacy complete_job API classifies ordinary JobStatus::Failed as test_failure, preserving the pre-Phase-15 fail-closed behavior.

## Retry policy

RetryPolicy contains:

    max_attempts
    base_delay_secs
    max_delay_secs
    retry_interrupted

Hard limits:

    max_attempts <= 5
    base_delay_secs <= 3600
    max_delay_secs <= 86400

Delay grows exponentially from the persisted attempt number and is capped by max_delay_secs.

Retry policy is stored with the durable job so controller restart does not silently change behavior.

Jobs enqueued through the existing enqueue_job API receive RetryPolicy::no_retry().

## Controller schema v3

Schema v3 extends the Phase 14 durable controller.

New jobs columns:

    retry_policy_json
    failure_class
    next_retry_at_secs
    retry_reason

New job_attempts column:

    failure_class

New index:

    idx_jobs_retry_ready

Existing schema v1/v2 databases migrate through the fixed source-controlled migration path to v3.

## Lifecycle states

Phase 15 adds:

    retry_pending
    exhausted

Existing states remain:

    queued
    assigned
    running
    passed
    failed
    rejected
    cancelled
    interrupted

A transient infrastructure failure with attempts remaining becomes retry_pending with a durable next_retry_at_secs.

The scheduler will not assign it before that timestamp.

Once due, it re-enters the normal capability-aware assignment path and creates a new durable attempt.

When retry attempts are exhausted, the job becomes exhausted and is no longer schedulable.

## Restart recovery

On controller restart, jobs persisted as assigned/running are classified as interrupted.

The most recent attempt is closed as interrupted.

If the stored policy explicitly has retry_interrupted=true and attempts remain:

    interrupted -> retry_pending

Otherwise:

    interrupted -> interrupted

This prevents implicit duplicate execution.

## Manual recovery

An operator can explicitly reschedule an interrupted job:

    lifecycle-reschedule --job-id <uuid>

Manual rescheduling is an explicit override of the automatic retry policy, but still obeys the global five-attempt safety ceiling.

Only jobs currently in interrupted state may be manually rescheduled.

The action is hash-chained into the durable audit history.

## Retry cancellation

Jobs in queued or retry_pending state may be cancelled before assignment.

Cancellation clears next_retry_at_secs and retry_reason so a cancelled scheduled retry cannot later become eligible.

## Attempt history

Every assignment creates a new durable attempt number.

Attempt records preserve:

- worker;
- assigned/running/terminal state;
- start/finish timestamps;
- summary;
- failure classification.

The retry policy is based on the persisted attempt count rather than process-local counters.

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- lifecycle-doctor

End-to-end lifecycle fixture:

    cargo run -p dragonforge-test-lab -- lifecycle-fixture

Inspect a durable job:

    cargo run -p dragonforge-test-lab -- lifecycle-status --job-id <uuid>

Use another controller database:

    cargo run -p dragonforge-test-lab -- lifecycle-status --job-id <uuid> --state-db <path>

Explicitly reschedule an interrupted job:

    cargo run -p dragonforge-test-lab -- lifecycle-reschedule --job-id <uuid>

## Validation

Run:

    .\scripts\test-phase15.ps1

Validation checks:

1. environment;
2. cargo fmt;
3. strict workspace Clippy;
4. full workspace tests/doc-tests;
5. controller schema v3;
6. lifecycle doctor;
7. retry/recovery fixture;
8. Phase 14 observability regression;
9. Phase 13 worker-service regression;
10. Phase 12 mTLS regression;
11. general doctor Phase 15;
12. GitHub-aware native execution.

## Security boundary

Phase 15 does not infer that a failed test is infrastructure failure.

Retry classification is explicit.

Test failures remain terminal even when a retry policy exists.

Retries still pass through the normal capability-aware controller scheduler and then the existing Agent/Policy/Executor worker boundary.

No retry state stores or accepts command text.

No restart automatically replays in-flight work unless the persisted policy explicitly opted into interrupted retry.

Manual rescheduling is restricted to interrupted jobs and a global five-attempt ceiling.

## Exit criteria

Phase 15 is complete when validation proves:

- schema v1/v2 migration reaches v3;
- test failures are terminal;
- transient infrastructure failures retry only after their due timestamp;
- retries create new durable attempts;
- retry limits produce exhausted;
- retry_pending can be cancelled;
- interrupted restart recovery respects retry_interrupted;
- explicit interrupted rescheduling works;
- global manual reschedule limit is enforced;
- lifecycle actions remain audit chained;
- Phase 14 observability remains green;
- Phase 13 worker services remain green;
- Phase 12 mTLS remains green;
- GitHub-aware execution remains green.


## Validation status

Phase 15 validation completed successfully on the Windows host on 2026-09-22. The final run passed formatting, strict Clippy, the full workspace and doc-test suite, schema migration through v3, bounded retry policy tests, retry scheduling and due-time enforcement, retry exhaustion, retry-pending cancellation, interrupted-job recovery policy, manual interrupted rescheduling with the global attempt ceiling, audit-chain verification, the Phase 14 observability regression, Phase 13 worker-service regression, Phase 12 mTLS regression, and the GitHub-aware native worker regression.
