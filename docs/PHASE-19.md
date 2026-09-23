# Phase 19 — Linux Qualification

Phase 19 qualifies DragonForge Test Lab as a first-class Linux worker platform rather than relying only on container execution.

## Delivered

- native Linux process-tree containment using a dedicated session/process group;
- per-job Linux address-space and process-count rlimits;
- whole-tree SIGKILL cancellation through the process group;
- Linux-native sandbox doctor reporting `linux_process_group_rlimit`;
- Linux qualification fixture covering containment, cancellation, worker-service mTLS, heartbeat, drain, restart recovery, systemd metadata, encrypted transport, and node identity;
- Linux container image build script for Docker/Podman;
- full Linux validation script covering native/container workers, services, recovery, observability, and advanced Rust lanes;
- workspace version 0.20.0.

## Native containment

Before spawning a native Linux test command, the sandbox uses `setsid()` so the child becomes the leader of a dedicated session/process group. The child also receives fixed `RLIMIT_AS` and `RLIMIT_NPROC` ceilings derived from the existing typed `SandboxLimits`. Cancellation targets the entire process group with `SIGKILL`.

This keeps the executor shell-free: executable and argument construction remain fixed by typed TestAction mappings.

## Linux worker doctor

Run on Linux:

    cargo run -p dragonforge-test-lab -- linux-doctor

The doctor requires Linux and reports native containment, architecture, systemd hardening metadata, outbound mTLS availability, and supported container modes.

## Linux fixture

Run:

    cargo run -p dragonforge-test-lab -- linux-fixture

The fixture verifies native process-group containment and tree cancellation with a fixed `sleep` child, then reuses the real Phase 13 worker-service fixture and Phase 12 mTLS fixture to verify registration, heartbeat, drain behavior, restart recovery, systemd unit generation, encrypted transport, and certificate-bound node identity.

## Full qualification

On a Linux host or Linux VM:

    bash ./scripts/test-phase19-linux.sh --revision main

Install/update mandatory advanced Rust tools automatically:

    bash ./scripts/test-phase19-linux.sh --revision main --install-tools

Add Linux nightly Miri, AddressSanitizer, and bounded fuzzing:

    bash ./scripts/test-phase19-linux.sh --revision main --install-tools --include-nightly --fuzz-seconds 30

The script requires Docker or Podman. It validates:

1. environment/toolchain;
2. rustfmt;
3. strict Clippy;
4. full workspace tests;
5. Linux doctor;
6. Linux containment/service/mTLS fixture;
7. native sandbox doctor;
8. container image build;
9. container sandbox doctor;
10. worker-service regression;
11. lifecycle/recovery and observability regressions;
12. cargo-nextest;
13. cargo-llvm-cov, property tests, and benchmark compilation;
14. GitHub-aware native worker execution;
15. GitHub-aware container worker execution;
16. general doctor reporting Phase 19.

## Service operation

Phase 13 already generated hardened systemd units. Phase 19 qualifies that Linux path together with the shared worker runtime. The service remains outbound-only and uses mTLS; no inbound worker listener or remote shell is added.

## Containers

`scripts/build-sandbox-image.sh` builds the existing fixed Rust worker image with either Docker or Podman. Container execution retains capability dropping, no-new-privileges, PID limits, memory limits, fixed Cargo-only wrapping, and workspace bind-mount containment.

## Advanced Rust lanes

The mandatory Phase 19 Linux qualification includes nextest, llvm-cov, protocol property tests, and Criterion benchmark compilation. `--include-nightly` additionally runs Miri, Linux AddressSanitizer, and bounded cargo-fuzz directly on the qualified Linux worker.

## Security boundary

Phase 19 does not introduce generic commands, shell text, caller-provided executable paths, arbitrary arguments, inbound worker control, or public controller addresses. Linux native containment is an additional worker-side enforcement layer; it does not replace Agent/Policy capability and repository authorization.

## Validation status

Implementation and real-host Linux qualification are complete. `scripts/test-phase19-linux.sh` passed end-to-end on an Ubuntu Server x86_64 host on 2026-09-23, including rustfmt, strict Clippy, full workspace tests/doc-tests, Linux doctor/fixture, native containment, Docker sandbox validation, worker-service/lifecycle/observability regressions, nextest, llvm-cov, protocol property tests, benchmark compilation, GitHub-aware native execution, GitHub-aware Docker execution, and the final Phase 19 doctor.
