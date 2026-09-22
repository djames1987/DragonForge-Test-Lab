# Phase 7 — GUI Automation

Status: implementation complete; Windows host and VM validation pending.

## Goal

Phase 7 adds typed Windows GUI automation for DragonForge-managed application windows. It supports deterministic interaction plans, Windows UI Automation control discovery, value entry, control invocation, assertions, screenshot capture, and controlled crash/exit capture without exposing an arbitrary shell or unrestricted desktop-control API.

## Delivered

- New `df-test-gui` crate.
- `gui-doctor` readiness command.
- `gui-run-plan --plan <plan.json>` typed deterministic plan runner.
- `gui-fixture` end-to-end validation command.
- Managed-window namespace enforcement: titles must begin with `DragonForge-`.
- AutomationId validation and bounded text values.
- Typed actions:
  - `wait_for_window`
  - `set_value`
  - `invoke`
  - `assert_value`
  - `screenshot`
- Windows UI Automation integration through fixed internally generated commands.
- Window-bounded PNG screenshot capture.
- Artifact path containment.
- Deterministic WinForms fixture application.
- Owned fixture process monitoring.
- Deliberate crash/exit fixture using expected code 23.
- JSON crash report generation.
- Example deterministic plan.
- GUI host readiness checker.
- Full Phase 7 validation script with uploadable transcript and artifact directory.

## Managed-window boundary

Phase 7 does not allow plans to target arbitrary desktop applications.

Window titles must begin with:

    DragonForge-

Titles and AutomationIds use restricted character sets and bounded lengths. This keeps the GUI automation surface aligned with DragonForge-owned/test fixtures rather than turning Test Lab into a generic desktop remote-control tool.

## Deterministic interaction plan

Plans use JSON with a fixed schema. Example:

    {
      "window_title": "DragonForge-GUI-Fixture",
      "actions": [
        { "type": "wait_for_window", "timeout_ms": 10000 },
        { "type": "set_value", "automation_id": "inputBox", "value": "DragonForge deterministic plan" },
        { "type": "invoke", "automation_id": "applyButton" },
        { "type": "assert_value", "automation_id": "outputBox", "expected": "DragonForge deterministic plan" },
        { "type": "screenshot", "name": "phase7-plan.png" }
      ]
    }

Run:

    cargo run -p dragonforge-test-lab -- gui-run-plan --plan .\examples\phase7-plan.json

Plans cannot specify executable names, shell commands, PowerShell fragments, arbitrary UI Automation properties, or raw input events.

## UI Automation

Phase 7 locates managed top-level windows by exact UI Automation Name and child controls by exact AutomationId.

Supported control patterns are deliberately narrow:

- ValuePattern for set/assert operations.
- InvokePattern for button-like controls.

Raw mouse coordinates, raw keyboard injection, arbitrary COM patterns, and arbitrary scripts are not exposed in this phase.

## Screenshot capture

Screenshots are captured only for the bounding rectangle of the managed target window.

Screenshot names:

- must be leaf `.png` names;
- use a restricted character set;
- cannot contain `..`;
- are written beneath the caller-selected artifact directory.

The Phase 7 validation generates:

    phase7-plan.png
    phase7-fixture.png

## Crash capture

The deterministic Phase 7 fixture includes a controlled crash button.

The fixture is launched as an owned child process by Test Lab. After the normal interaction and screenshot path passes, Test Lab invokes the controlled crash action and waits for the child process to terminate.

Expected exit code:

    23

The result is persisted as:

    phase7-crash-report.json

This proves Test Lab can distinguish an expected fixture termination from a hung GUI process and record the exit outcome as an artifact.

## Interactive desktop requirement

GUI automation requires an active interactive Windows desktop.

Suitable environments include:

- the normal logged-in Windows host session;
- an active console or RDP session inside the Windows Test Lab VM.

It is not expected to work from Session 0, a disconnected GUI-less worker, or a non-interactive service desktop.

## Validation

Host:

    .\scripts\test-phase7.ps1

Windows VM:

    .\scripts\test-phase7.ps1

The validation performs:

1. cargo fmt;
2. cargo clippy;
3. cargo test;
4. GUI host readiness;
5. GUI doctor;
6. JSON plan replay against a live fixture;
7. screenshot artifact verification;
8. owned fixture crash capture;
9. GitHub-aware native worker regression.

Upload the generated `test-logs/phase7-validation-*.log` and the Phase 7 artifact directory before merging Phase 7.
