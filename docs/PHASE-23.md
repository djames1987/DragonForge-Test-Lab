# Phase 23 — Security Review

Phase 23 performs a dedicated adversarial review of DragonForge Test Lab's trust boundaries and turns the review into repeatable validation rather than a one-time checklist.

## Delivered

- dedicated `df-test-security-review` crate;
- workspace version `0.24.0`;
- bounded repository security-evidence scanner;
- blocking secret/key-file detection;
- machine-readable JSON security review reports;
- `security-doctor`, `security-fixture`, and `security-review` CLI commands;
- Windows and Linux Phase 23 validation scripts;
- dependency/advisory/license/source audit reuse from Phase 22;
- focused regression coverage for repository allowlisting and security review behavior;
- two concrete review hardening fixes:
  - core repository allowlists now distinguish owner prefixes from exact repository identities and reject look-alike repository names;
  - MCP low-level HTTP failures no longer return internal parser/error detail before authentication.

## Review scope

The formal review covers the areas named in the roadmap:

| Area | Evidence reviewed | Phase 23 gate |
| --- | --- | --- |
| Protocol | typed `TestAction`, capabilities, resource limits | fixed evidence rule + workspace tests |
| Worker authorization | independent Agent/Policy enforcement | policy regression tests |
| Repository identity | HTTPS + allowlist + immutable GitHub SHA | hardened identity matching |
| Paths | canonical roots, leaf-name validation, traversal/symlink rejection | evidence rules + fixture regressions |
| Artifacts | bounded metadata/content generation and SHA-256 | observability/release regressions |
| MCP | loopback-only, bearer auth, bounded framing/jobs, named tools only | MCP fixture + static evidence |
| Certificates | rustls mTLS, CA validation, fingerprint binding, revocation | identity fixture |
| Transport | private/local controller policy, replay controls, outbound workers | distributed/identity evidence |
| DoS bounds | HTTP/body/job/cert/file/count/resource ceilings | static evidence + tests |
| Secrets | no private keys in state; committed key scan | blocking scanner |
| Logs | bounded/redacted structured logs | observability fixture |
| Persistence | fixed SQLite migrations, fail-closed schema behavior, audit chain | workspace/observability/lifecycle regressions |
| Privileges | typed Hyper-V/Windows operations, managed namespaces | static evidence + prior platform fixtures |
| Installers | safe leaf filenames, target checks, hashes, symlink rejection | install fixture |
| Releases | audit gates, checksums, signing requirements | release fixture + cargo audit/deny |

## Security review engine

Run:

    cargo run -p dragonforge-test-lab -- security-review --root . --output ./test-logs/security-report.json

The scanner:

- canonicalizes the review root;
- reads only a fixed set of critical security-evidence files for invariant checks;
- bounds each evidence file to 2 MiB;
- bounds the text scan to 4096 files and 64 MiB total;
- skips `.git`, `target`, validation logs, and generated release output;
- does not follow symlinks;
- fails on committed private-key PEM material in non-document text;
- fails on secret-like key files such as `.pfx`, `.p12`, `.key`, `id_rsa`, or `id_ed25519`;
- reports high/critical findings as blocking.

It is not a replacement for dependency scanning, code review, fuzzing, platform validation, or penetration testing. Phase 23 combines it with the existing security-sensitive fixtures and Phase 22 dependency audit.

## Review findings fixed

### SR-23-01 — Repository prefix identity ambiguity

The core `ExecutionPolicy` previously used raw `starts_with` matching for every configured repository allowlist value. An operator value lacking a trailing slash could unintentionally behave as a textual prefix and admit a look-alike owner/repository string.

Phase 23 changes the rule to:

- values ending in `/` are explicit owner/path prefixes;
- other values are exact repository identities;
- optional terminal `.git` is normalized;
- look-alike identities are rejected.

Regression tests cover both look-alike rejection and exact-repository `.git` normalization.

### SR-23-02 — MCP unauthenticated parser-detail disclosure

The low-level MCP stream handler previously included the internal error string in a generic HTTP 500 response when request parsing/handling failed. Because parsing occurs before authentication, malformed unauthenticated traffic could receive unnecessary implementation detail.

Phase 23 changes that path to a generic error response with no internal detail. Normal authenticated JSON-RPC validation errors remain bounded protocol responses.

## Deliberate non-findings / retained boundaries

Phase 23 does not convert DragonForge into a remote shell.

It does not add arbitrary command execution, raw SQL, arbitrary artifact reads, generic device writes, public controller listeners, trust-on-first-use certificates, or installer execution supplied by remote callers.

The Phase 8 HMAC transport remains compatibility/private-lab functionality; mTLS remains the preferred service transport.

Phase 20 ARM/HIL remains read-only and still requires real physical ARM qualification.

## Validation

Windows:

    .\scripts\test-phase23.ps1 -InstallTools

Linux:

    bash ./scripts/test-phase23-linux.sh --install-tools

The validation runs formatting, strict Clippy, the full workspace suite, the Phase 23 doctor/fixture/repository scan, cargo-audit/cargo-deny, mTLS identity, MCP, observability, installer and release fixtures, focused policy/security-review tests, and the general Phase 23 doctor marker.

## Security boundary

The security-review command is an operator-side local inspection tool. It does not add network listeners or remote execution authority. Its source inspection is bounded and root-contained, and it does not follow symlinks.

A passing Phase 23 report means the encoded invariants and regression suites passed. It is evidence of review coverage, not a mathematical proof that the software contains no vulnerabilities.

## Qualification result

Windows qualification completed successfully on 2026-09-23 against commit `5c69472cd3c7d2684cf388360622c9226062f065`.

Validated gates:

- cargo fmt;
- strict cargo clippy with `-D warnings`;
- full workspace tests with all features;
- Phase 23 security doctor, deterministic fixture, and repository review;
- cargo-audit advisory scan;
- cargo-deny license/advisory/source policy;
- mTLS identity fixture;
- MCP authentication/protocol fixture;
- observability/redaction/audit-chain fixture;
- installer fixture;
- release fixture;
- focused policy and security-review crate tests;
- general doctor reporting `phase=23`.

The machine-readable security report recorded 10/10 invariant checks passed, 173 files scanned, and zero findings. The dependency audit completed with no advisory, license, or source blockers; unmatched allowlisted licenses were informational warnings only.

## Status

**Complete / Windows Qualified.**

Linux-specific Phase 23 validation remains available through `scripts/test-phase23-linux.sh` as an additional cross-platform regression lane, but Phase 23's security-review implementation and Windows qualification are complete.
