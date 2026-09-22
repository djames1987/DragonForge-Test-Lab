# Phase 16 — Test Plans

Status: implementation complete; Windows host validation pending.

## Goal

Phase 16 introduces versioned declarative test plans that compile into existing typed Test Lab jobs without adding a command-string execution surface.

A plan can declare:

- typed executable profiles;
- explicit typed actions;
- dependency DAGs;
- dependency completion conditions;
- resource/time limits;
- required worker capabilities;
- requested artifact classes;
- Phase 15 retry policy;
- target operating system;
- required node labels.

## New crate

    crates/df-test-plans

The crate owns plan parsing, validation, dependency ordering, readiness decisions, target matching, and compilation into typed JobRequest values.

Current plan format version:

    1

Unsupported versions fail closed.

## Profiles

Executable profiles are intentionally limited to actions already represented by df-test-protocol.

### rust_fast

    checkout
    cargo fmt --check
    cargo test

### rust_standard

    checkout
    cargo fmt --check
    cargo clippy -D warnings
    cargo test --all-features

### rust_release

    checkout
    cargo build --release
    cargo test --all-features

### typed_actions

A plan may explicitly list existing TestAction enum values.

It still cannot supply:

- executable names;
- arbitrary arguments;
- shell text;
- PowerShell;
- environment mutation commands.

Broader advisory profiles from Test Intelligence are not silently converted into executable plan steps. Windows/GUI/distributed execution will be added to plans only when those execution paths have a unified typed job contract.

## Dependencies and conditions

Each step has a unique bounded ID.

depends_on forms a DAG. Validation rejects:

- duplicate IDs;
- missing dependency IDs;
- self-dependencies;
- cycles;
- excessive steps/dependencies.

Conditions:

    dependencies_passed
    always_after_dependencies

dependencies_passed requires every dependency to have passed.

always_after_dependencies waits for each dependency to reach a terminal plan-step state, then allows the dependent step regardless of pass/fail/skip/cancel result.

Readiness is deterministic and follows topological ordering.

## Resource limits

Plans contain ResourceLimits and validate them before compilation.

Current bounds:

    timeout_seconds: 1..=86400
    max_memory_mib: 128..=131072
    max_disk_mib: 128..=1048576
    max_processes: 1..=4096

The executor still independently applies its existing resource and sandbox controls.

## Capabilities

A plan may declare required capabilities.

Phase 16 adds backward-compatible JobRequest.extra_required_capabilities.

JobRequest.required_capabilities now returns the union of:

- capabilities derived from typed actions;
- extra plan-declared capabilities.

This means existing controller capability scheduling automatically enforces plan-added capability requirements.

If a plan supplies a non-empty capability declaration, it must include every capability implied by its typed actions.

## Target OS and node labels

CompiledPlanStep retains:

    target_os
    node_labels

matches_target(worker_os, labels) enforces:

- any/windows/linux/macos OS selection;
- exact required label key/value matches.

These are plan-orchestrator scheduling constraints. The older DurableController worker record has no node-label inventory, so Phase 16 deliberately does not pretend its legacy assign_next method can enforce labels. Distributed/plan orchestration must call matches_target before submitting a compiled job to a selected worker/node.

## Artifacts

Artifact requests are typed:

    step_logs
    execution_report

These map to outputs already produced by the current Rust executor. Plans do not accept arbitrary filesystem paths or glob patterns.

## Retry policy

Every step contains the Phase 15 RetryPolicy.

Compiled steps retain the policy and can be queued through:

    DurableController::enqueue_job_with_retry

The global Phase 15 retry/attempt bounds still apply.

## Durable storage

Controller schema v4 adds:

    test_plans

Stored fields:

    plan_name
    plan_version
    plan_json
    created_at_secs
    updated_at_secs

Plan creation and updates are hash-chain audited as:

    test_plan_created
    test_plan_updated

Plans are validated before persistence.

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- plan-doctor

Validate:

    cargo run -p dragonforge-test-lab -- plan-validate --plan .\examples\phase16-plan.json

Compile one step:

    cargo run -p dragonforge-test-lab -- plan-compile --plan .\examples\phase16-plan.json --step standard

Store:

    cargo run -p dragonforge-test-lab -- plan-store --plan .\examples\phase16-plan.json

List stored plans:

    cargo run -p dragonforge-test-lab -- plan-list

End-to-end fixture:

    cargo run -p dragonforge-test-lab -- plan-fixture

## Bounds

Current format limits:

    plan steps <= 256
    dependencies per step <= 64
    artifact requests per step <= 32
    node labels per step <= 32
    step ID <= 64 bytes
    input JSON <= 1 MiB

## Security boundary

Phase 16 is configuration and orchestration, not a new execution primitive.

Plans cannot introduce arbitrary commands.

Compilation produces ordinary JobRequest values and those jobs still pass through:

    plan validation
      -> controller capability scheduling
      -> worker Agent/Policy
      -> Executor fixed action templates
      -> sandbox/resource controls

Extra capability declarations strengthen scheduling requirements; they do not grant capabilities.

OS/label target constraints restrict candidate workers/nodes; they do not authorize them.

Artifact declarations are typed classes, not caller-controlled deletion/read paths.

## Validation

Run:

    .\scripts\test-phase16.ps1

Validation covers:

- formatting;
- strict Clippy;
- all workspace/doc tests;
- schema v4 migration;
- plan doctor;
- checked-in plan validation;
- step compilation;
- DAG/readiness behavior;
- target OS/label matching;
- durable plan storage/audit;
- extra capability scheduling enforcement;
- Phase 15 lifecycle regression;
- Phase 14 observability regression;
- Phase 13 worker-service regression;
- Phase 12 mTLS regression;
- GitHub-aware native execution.

## Exit criteria

Phase 16 is complete when all validation is green on the Windows host.
