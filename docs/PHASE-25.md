# Phase 25 — Dogfooding

Status: complete; Windows host qualification passed on 2026-09-24.

## Goal

Phase 25 makes DragonForge Test Lab the normal repeatable validation path for active DragonForge Rust projects instead of relying on hand-copied Cargo command sequences.

The phase introduces checked-in dogfood profiles, immutable-SHA execution, bounded self-hosting, consolidated campaign execution, and an explicit inventory of validation that remains manual for legitimate reasons.

## New crate

    crates/df-test-dogfood

Workspace version:

    0.26.0

## Dogfood profiles

A profile maps one approved GitHub repository to one existing typed Rust validation profile.

Supported profiles are deliberately limited to:

- `rust_fast`;
- `rust_standard`;
- `rust_release`.

They compile only to existing `TestAction` variants. Profiles cannot contain executable names, arguments, shell text, PowerShell, environment mutation, arbitrary artifact paths, or remote commands.

Current checked-in profiles:

- `dogfood/dragonforge-test-lab.json`;
- `dogfood/dragonforge-security-suite.json`;
- `dogfood/dragonforge-security-test-lab.json`.

The representative campaign is:

    dogfood/phase25-campaign.json

## Immutable revisions

`dogfood-run` never executes a mutable branch name directly.

The requested revision is first resolved through the existing GitHub integration to a 40-character commit SHA. The profile then compiles a job pinned to that SHA.

The resolved SHA is printed before execution and appears in campaign reports.

## Self-hosting

DragonForge Test Lab dogfoods itself using a previously running Test Lab process and a disposable worker workspace.

Self-hosting has a hard orchestration-depth ceiling:

    1

The self-host profile requires depth exactly one. An external dogfood profile uses depth zero and cannot recurse.

There is no automatic nested dogfood invocation inside the checked-out repository's Cargo tests, and profile compilation rejects depths beyond the declared ceiling.

## Execution boundary

Dogfood jobs execute through the existing path:

    profile validation
      -> GitHub immutable SHA resolution
      -> typed JobRequest
      -> Agent
      -> ExecutionPolicy
      -> LocalExecutor
      -> sandbox/resource controls

Phase 25 does not add a second execution engine.

## Campaigns

`dogfood-campaign-run` executes enabled profiles serially.

For each repository it records:

- profile name;
- repository;
- resolved immutable SHA;
- terminal status;
- execution summary;
- artifact directory.

The campaign stops on the first failed project and the consolidated report is not considered passed unless every executed profile passed.

This prevents a later successful project from hiding an earlier failure.

## Manual validation inventory

Dogfood profiles explicitly retain validation that should not be silently automated.

Current examples include:

- physical ARM/Raspberry Pi qualification;
- production signing identities;
- privileged Windows security integration requiring explicit approval;
- destructive security scenarios that must remain isolated and explicitly authorized.

Phase 25 eliminates routine manual Rust gates where practical; it does not remove legitimate human approval or hardware boundaries.

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- dogfood-doctor

Fixture:

    cargo run -p dragonforge-test-lab -- dogfood-fixture

Validate a profile:

    cargo run -p dragonforge-test-lab -- dogfood-profile-validate --profile .\dogfood\dragonforge-test-lab.json

Compile a profile to an immutable typed job:

    cargo run -p dragonforge-test-lab -- dogfood-profile-compile --profile .\dogfood\dragonforge-test-lab.json --sha <40-char-sha> --depth 1

Run one profile:

    cargo run -p dragonforge-test-lab -- dogfood-run --profile .\dogfood\dragonforge-test-lab.json --revision <revision>

Validate the campaign:

    cargo run -p dragonforge-test-lab -- dogfood-campaign-validate --campaign .\dogfood\phase25-campaign.json

Run the full external campaign:

    cargo run -p dragonforge-test-lab -- dogfood-campaign-run --campaign .\dogfood\phase25-campaign.json

## Qualification

Windows:

    .\scripts\test-phase25.ps1

The default qualification executes a real self-hosted validation of the current Test Lab commit and validates the external DragonForge profiles/campaign.

To additionally execute all enrolled external repositories:

    .\scripts\test-phase25.ps1 -RunExternalCampaign

Linux:

    bash ./scripts/test-phase25-linux.sh

or:

    bash ./scripts/test-phase25-linux.sh --run-external-campaign

The default qualification runs:

1. environment/tooling checks;
2. rustfmt;
3. strict Clippy;
4. complete workspace tests;
5. dogfood doctor and deterministic fixture;
6. all checked-in profile and campaign validation;
7. immutable self-host profile compilation;
8. an actual self-hosted Test Lab run pinned to the current commit SHA;
9. optional real multi-repository campaign execution;
10. Phase 24 reliability/chaos regression;
11. focused dogfood crate tests;
12. final doctor reporting `phase=25`.

## Security and reliability invariants

Phase 25 requires:

- GitHub repositories use the existing strict HTTPS parser;
- mutable revisions are resolved before execution;
- jobs are pinned to immutable 40-character SHAs;
- profiles compile only typed actions;
- self-hosting cannot recurse beyond one orchestration layer;
- external profiles cannot recurse;
- profile/campaign files are bounded to 1 MiB;
- campaigns reject duplicate profile names and duplicate repositories;
- resource ceilings remain bounded;
- campaign execution stops on first failure;
- Agent/Policy/Executor checks remain mandatory;
- no auto-merge, code rewriting, arbitrary shell, or generic remote execution is introduced;
- Phase 24 reliability invariants remain green.

## Exit criteria

Phase 25 is implementation-complete when:

- the dogfood profile/campaign schema is integrated;
- at least three representative DragonForge repositories are enrolled;
- Test Lab can validate itself at an immutable SHA through the normal executor;
- recursion protection is tested;
- consolidated campaign reports are available;
- legitimate manual validation is recorded explicitly;
- Windows/Linux qualification scripts are checked in;
- Phase 24 reliability remains a qualification regression.

Full Windows qualification requires a successful `test-phase25.ps1` log.


## Qualification result

Windows qualification completed successfully on 2026-09-24.

The successful run passed:

- environment and tool checks;
- cargo fmt;
- strict workspace Clippy with `-D warnings`;
- complete workspace tests;
- dogfood doctor and deterministic fixture;
- validation of all checked-in dogfood profiles and the consolidated campaign;
- immutable self-host profile compilation;
- a real self-hosted DragonForge Test Lab dogfood run;
- Phase 24 reliability / chaos regression;
- focused `df-test-dogfood` tests;
- final general doctor reporting `phase=25`.

Qualification log:

    test-logs/phase25-dogfooding-20260924-085253.log

The external multi-repository campaign was intentionally skipped during the baseline Windows qualification. Its profiles and campaign definition were validated successfully, and full external execution remains available through `-RunExternalCampaign`.
