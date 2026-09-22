# Phase 10 — Test Intelligence

Status: implementation complete; Windows host validation pending.

## Goal

Phase 10 adds an explainable, deterministic intelligence layer that helps DragonForge Test Lab decide what should be tested and where it should run. It does not execute arbitrary work and does not bypass the Agent, Policy, Executor, distributed-node, VM, or MCP trust boundaries.

The intelligence engine answers four questions:

1. Which test profiles are relevant to the changed files?
2. Which historical regressions should increase confidence requirements?
3. Which failures appear to be repeated instances of the same underlying problem?
4. Which eligible worker should receive each recommended profile based on capacity?

## New crate

    crates/df-test-intelligence

The crate contains no process-launching, shell, PowerShell, GitHub, network-listener, or filesystem-mutation surface.

Its inputs are typed data and its output is a typed recommendation report.

## Test profiles

Phase 10 models these bounded profiles:

    rust_fast
    rust_standard
    rust_deep
    windows_integration
    gui_automation
    distributed_network
    mcp_gateway
    full_regression

Profiles are recommendations only. Existing phases remain responsible for translating approved profiles into typed execution.

## Change-aware selection

A ChangeSet contains repository-relative paths only.

The engine rejects:

- absolute paths;
- parent traversal;
- control characters;
- more than 4096 changed files.

Rules are deterministic and visible in code.

Examples:

- Cargo.toml/Cargo.lock -> Rust standard validation.
- core protocol/policy/agent/executor/controller -> Rust standard plus deeper regression weight.
- df-test-windows -> Windows integration.
- df-test-gui -> GUI automation.
- df-test-distributed -> distributed/network validation.
- df-test-mcp -> MCP gateway validation.
- four or more changed crates -> full regression recommendation.

Every recommendation carries an integer score and human-readable reasons.

## Historical regression targeting

Historical failures can carry:

- profile;
- step name;
- bounded failure message;
- changed-file list;
- timestamp.

When a current changed file overlaps the changed files associated with a prior failure, the corresponding profile receives an explicit score bonus.

The final report exposes regression_targets for profiles with sufficiently high confidence scores.

History is bounded to 10,000 records.

## Failure clustering

Phase 10 normalizes volatile failure data before fingerprinting.

Examples of volatile data include:

- long numeric IDs;
- long hexadecimal IDs;
- UUID-like tokens;
- filesystem paths.

The normalized step/message signature is SHA-256 hashed.

Cluster output contains:

- fingerprint;
- normalized signature;
- affected profiles;
- occurrence count;
- first/last seen timestamps;
- up to five sample failure IDs.

Clustering is deterministic; no external AI model or probabilistic service is involved.

## Resource-aware scheduling

Workers declare:

- worker ID;
- supported intelligence profiles;
- total/free memory;
- maximum parallel jobs;
- current active jobs;
- load percentage.

Scheduling rejects workers that:

- do not support the profile;
- lack estimated free memory;
- have no free job slot.

Among eligible workers, the scheduler deterministically prefers:

1. lower load;
2. fewer active jobs;
3. more free memory;
4. lexical worker ID as a stable tie-break.

Any recommendation that cannot be safely assigned is returned in unscheduled_profiles. The engine never silently drops unsupported work.

## CLI

Run the deterministic fixture:

    cargo run -p dragonforge-test-lab -- intelligence-fixture

Analyze a JSON input:

    cargo run -p dragonforge-test-lab -- intelligence-analyze --input .\examples\phase10-intelligence.json

The command prints the full IntelligenceReport as JSON.

## Example

See:

    examples/phase10-intelligence.json

The example includes MCP, controller, distributed, and workspace changes, repeated historical network failures, and two workers with different capabilities/resources.

## Validation

Run:

    .\scripts\test-phase10.ps1

The validation performs:

1. cargo fmt --check;
2. strict Clippy across all targets/features;
3. full workspace tests;
4. deterministic intelligence fixture;
5. realistic JSON analysis;
6. recommendation verification;
7. historical cluster verification;
8. resource-aware schedule verification;
9. Phase 10 doctor verification;
10. GitHub-aware native worker regression.

The script writes:

    test-logs\phase10-validation-*.log
    test-logs\phase10-analysis-*.json

## Security boundary

Phase 10 intentionally does not provide:

- command execution;
- shell or PowerShell generation;
- executable selection;
- raw process arguments;
- repository checkout;
- remote node control;
- VM mutation;
- firewall/network mutation;
- MCP authentication changes;
- autonomous approval of high-risk work.

Intelligence output is advisory input to the existing typed Test Lab execution boundary.

## Design principle

Phase 10 is intentionally explainable.

A recommendation should always be answerable with:

    why this profile?
    why this score?
    why this worker?
    why was this profile not scheduled?

The system favors deterministic rules and auditable history over opaque model output.
