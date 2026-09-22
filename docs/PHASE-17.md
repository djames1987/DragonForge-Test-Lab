# Phase 17 — Intelligence Integration

Status: implementation complete; validation pending branch qualification.

## Goal

Phase 17 connects the deterministic Phase 10 intelligence engine to real Test Lab state without widening the execution boundary.

It integrates:

- real GitHub changed-file comparisons;
- durable historical test failures;
- durable online worker capability/slot state;
- stored Phase 16 test plans;
- advisory and automatic decision modes;
- immutable head-SHA execution;
- hash-chained intelligence decision auditing.

## Git change input

`df-test-github` now provides a bounded compare operation. The requested base and head revisions are first resolved to immutable commit SHAs, then GitHub's compare endpoint supplies repository-relative changed files.

The changed-file list is sorted, deduplicated, and capped at the Phase 10 maximum of 4096 entries.

## Durable historical failures

Controller schema v5 adds `intelligence_job_context`.

When Phase 17 automatically queues a job, it records:

- job ID;
- selected intelligence profile;
- changed-file set;
- creation time.

Later terminal test failures can therefore be reconstructed as real `HistoricalFailure` input for Phase 10 scoring and clustering. Only jobs classified as `test_failure` and ending in failed/exhausted states are admitted to this history.

## Live worker capacity

Phase 17 derives scheduling candidates from durable workers that are currently marked online.

Supported intelligence profiles are inferred only from the worker's existing typed capabilities. Current assigned/running jobs are counted as live slot consumption.

The current controller registration does not persist host RAM/load telemetry or service max-parallel configuration, so Phase 17 deliberately uses a conservative one-slot capacity model with a fixed 8192 MiB scheduling envelope. This is an explicit compatibility bridge, not a claim of full host-resource telemetry.

## Advisory mode

Advisory mode performs the complete integration pipeline and records the decision, but never enqueues work.

It is the default mode.

## Automatic mode

Automatic mode is intentionally narrow.

A step may be automatically queued only when all of the following are true:

1. its recommendation score meets the configured threshold;
2. Phase 10 found live eligible worker capacity;
3. the Phase 16 profile maps exactly to `rust_fast` or `rust_standard`;
4. the plan step has no dependencies;
5. the step target OS is `any`;
6. the step has no node-label constraints.

The compiled typed job is pinned to the resolved head commit SHA before enqueue.

Automatic mode does not execute shell strings, generate raw commands, bypass dependency gates, bypass target routing, or convert broader advisory profiles into executable work.

## Audit

Every integration decision receives a UUID and is persisted in the existing durable intelligence history.

A hash-chained `intelligence_decision` audit event records the decision mode and durable history row ID.

Automatically queued jobs also retain their intelligence profile/change context for future regression targeting.

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- intelligence-integration-doctor

Advisory integration:

    cargo run -p dragonforge-test-lab -- intelligence-integrate \
      --repo https://github.com/djames1987/DragonForge-Test-Lab.git \
      --base main \
      --head phase-17-intelligence-integration \
      --plan dragonforge-standard \
      --mode advisory

Automatic integration:

    cargo run -p dragonforge-test-lab -- intelligence-integrate \
      --repo https://github.com/djames1987/DragonForge-Test-Lab.git \
      --base main \
      --head <revision> \
      --plan <stored-plan-name> \
      --mode automatic \
      --min-score 60

## Validation

Run:

    .\scripts\test-phase17.ps1

The validation covers formatting, strict Clippy, the complete workspace tests, schema v5, Phase 17 doctor output, the dedicated integration crate tests, a real GitHub compare in advisory mode, audit persistence, Phase 16/15/14 regressions, the general Phase 17 doctor, and GitHub-aware native execution.

## Security boundary

Phase 17 makes intelligence operational, but not authoritative over arbitrary execution.

It does not add:

- shell/PowerShell fields;
- executable or raw argument injection;
- arbitrary repository URLs beyond existing GitHub validation;
- direct worker commands;
- dependency bypass;
- OS/label-routing bypass;
- automatic execution for GUI, distributed, MCP, Windows integration, Rust Deep, or Full Regression recommendations.

The existing Agent, Policy, Executor, lifecycle, plan, identity, and audit boundaries remain authoritative.
