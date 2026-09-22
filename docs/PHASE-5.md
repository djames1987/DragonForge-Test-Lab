# Phase 5 — Deep Rust Testing

Status: implementation complete; host validation pending.

## Goal

Phase 5 adds repeatable deep Rust quality testing beyond the baseline fmt/Clippy/unit-test gate while preserving DragonForge Test Lab's local-first and no-arbitrary-shell design.

## Delivered

- cargo-nextest profile for parallel workspace test execution.
- cargo-llvm-cov workspace coverage summary.
- deterministic property tests with proptest.
- Criterion benchmark target for capability derivation.
- cargo-fuzz libFuzzer target for serialized JobRequest input.
- Miri lane for the platform-neutral protocol crate.
- AddressSanitizer lane for supported Linux targets.
- Deep Rust tool readiness checker.
- `dragonforge-test-lab rust-doctor`.
- Phase 5 PowerShell validation script with an uploadable transcript.
- Optional tool installation through an explicit `-InstallTools` switch.

## Mandatory validation lane

The mandatory lane is intentionally cross-platform and consists of:

1. cargo fmt --check;
2. cargo clippy with warnings denied;
3. cargo test --workspace --all-features;
4. deep Rust tool doctor;
5. cargo nextest run --workspace --all-features;
6. targeted property tests;
7. cargo llvm-cov workspace summary;
8. Criterion benchmark compilation;
9. GitHub-aware native-worker regression against an immutable commit.

Run:

    .\scripts\test-phase5.ps1

To install/update the mandatory cargo tools first:

    .\scripts\test-phase5.ps1 -InstallTools

## Property testing

The protocol crate uses proptest to exercise invariants over generated JobRequest data:

- derived capability sets exactly match the capabilities required by generated action sequences;
- ResourceLimits and JobRequest JSON serialization round-trip over broad generated ranges.

The property lane is deterministic in structure and bounded by proptest's normal case limits.

## Benchmarks

The Criterion benchmark target is:

    cargo bench -p df-test-protocol --bench capability_derivation

The Phase 5 validation compiles the benchmark with `--no-run` so ordinary correctness validation is not coupled to noisy wall-clock performance thresholds. Performance baselines can be gathered separately on controlled hardware.

## Coverage

Run:

    cargo llvm-cov --workspace --all-features --summary-only

Coverage is treated as diagnostic evidence rather than an arbitrary repository-wide pass percentage. Future phases may add per-boundary thresholds once enough historical data exists.

## Miri

Install:

    rustup toolchain install nightly
    rustup component add --toolchain nightly miri rust-src

Run the Phase 5 Miri lane:

    .\scripts\test-phase5.ps1 -IncludeMiri

The default Miri target is df-test-protocol because it is platform-neutral. Miri's platform API support is more complete on Linux than Windows, so host/Hyper-V integration crates are not forced through Miri in this phase.

Reference:

- https://github.com/rust-lang/miri

## Sanitizers

AddressSanitizer is a nightly Linux lane in Phase 5:

    pwsh ./scripts/test-phase5.ps1 -IncludeSanitizer

The script runs the sanitizer only on supported non-Windows hosts and explicitly reports the Windows lane as skipped. The intended execution environment is the Linux Test Lab worker/golden VM.

Reference:

- https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/sanitizer.html

## Fuzzing

The bounded fuzz target is:

    fuzz/fuzz_targets/job_request_json.rs

It deserializes arbitrary bytes as JobRequest when possible, derives capabilities, and reserializes valid requests. This focuses fuzzing on the versioned protocol boundary without exposing shell input.

Install cargo-fuzz:

    cargo install --locked cargo-fuzz

Run on Linux for 60 seconds:

    pwsh ./scripts/test-phase5.ps1 -IncludeFuzz -FuzzSeconds 60

Crash artifacts and generated corpus data are local test artifacts and are ignored by Git.

Reference:

- https://rust-fuzz.github.io/book/cargo-fuzz.html

## Tool readiness

Read-only readiness:

    .\scripts\check-rust-deep-tools.ps1

Require the optional nightly/fuzz tools too:

    .\scripts\check-rust-deep-tools.ps1 -RequireNightlyTools

The `rust-doctor` CLI requires cargo-nextest and cargo-llvm-cov. Miri and cargo-fuzz are reported but remain optional because their execution support is target-dependent.

## Security boundary

Phase 5 does not add a generic command field to JobRequest.

The baseline Test Lab executor still constructs Git and Cargo commands from typed actions and fixed argument templates. Deep-test scripts contain explicit project-maintained commands. Fuzz input is consumed only as serialized protocol data and never becomes an executable command.

Nightly tooling is opt-in. Sanitizer and fuzz lanes are intended for disposable Linux worker/VM environments when testing untrusted repositories.

## Validation

Windows mandatory lane:

    .\scripts\test-phase5.ps1 -InstallTools

Expanded platform-neutral lane:

    .\scripts\test-phase5.ps1 -IncludeMiri

Linux deep lane:

    pwsh ./scripts/test-phase5.ps1 -IncludeMiri -IncludeSanitizer -IncludeFuzz -FuzzSeconds 60

Upload the generated `test-logs/phase5-validation-*.log` for review before Phase 5 is merged.
