# Phase 12 — mTLS / Node Identity

Status: complete; Windows host validation passed on 2026-09-22.

## Goal

Phase 12 adds certificate-backed controller/worker identity and encrypted mutual-TLS transport primitives while preserving the outbound-only, typed-work security model established in Phase 8.

The existing HMAC-SHA256 transport remains available as a compatibility/private-lab path, but mTLS is the preferred identity and confidentiality layer going forward.

## New crate

    crates/df-test-identity

The identity crate owns:

- X.509 certificate material loading;
- rustls client configuration;
- rustls server configuration requiring client certificates;
- certificate-authority trust stores;
- node identity binding by SHA-256 certificate fingerprint;
- certificate enrollment;
- certificate renewal with bounded overlap;
- certificate revocation;
- whole-identity revocation;
- trust/revocation state serialization;
- private/local controller address validation;
- a real loopback mutual-TLS fixture.

## Mutual TLS

The server configuration:

- trusts only certificates chaining to the configured CA roots;
- requires a client certificate;
- presents its own configured server certificate.

The client configuration:

- validates the server certificate against configured CA roots;
- presents its configured client certificate.

Phase 12 uses rustls rather than platform TLS APIs so Windows/Linux behavior can converge on the same Rust implementation.

## Node identity binding

TLS certificate-chain validation proves that a certificate chains to a trusted CA.

DragonForge node authorization additionally binds a typed node ID to an enrolled SHA-256 end-entity certificate fingerprint.

A certificate issued by the CA cannot impersonate another enrolled node merely by changing a claimed node ID.

The trust store verifies:

    node_id
      +
    presented certificate fingerprint
      +
    accepted lifecycle generation
      +
    logical validity window
      +
    revocation state

## Enrollment

Enrollment registers a certificate fingerprint against a validated node ID.

Limits:

    maximum identities: 4096
    maximum certificates per identity: 8
    maximum certificate chain entries: 8
    maximum aggregate certificate bytes: 256 KiB

Duplicate or already-revoked certificates are rejected.

## Renewal / key rotation

Renewal adds a new certificate generation while allowing an explicitly bounded overlap with the previous generation.

Example:

    generation 1 valid until 1000
    generation 2 starts at 500
    overlap cutover = 700

During the overlap both certificates may validate. After the configured overlap endpoint the old generation fails closed.

This provides a rotation hook without requiring an unsafe instant cutover.

## Revocation

Phase 12 supports:

- revoking one certificate fingerprint;
- revoking all enrolled certificates for an identity.

A revoked certificate fails closed even when its configured logical validity window has not expired.

## Trust-store persistence

The trust store serializes only:

- node IDs;
- certificate fingerprints;
- generation numbers;
- logical validity windows;
- revocation state.

Private keys are deliberately not stored in the trust-state JSON.

Certificate and private-key PEM material remains operator-controlled input to TLS configuration.

## Controller address policy

The mTLS identity layer retains the Phase 8 conservative network policy for built-in direct connections:

- IPv4 loopback;
- RFC1918 private IPv4;
- IPv4 link-local;
- IPv6 loopback;
- IPv6 unique-local;
- IPv6 link-local.

Public controller addresses fail closed.

Future Internet-facing operation still requires a separately reviewed deployment/authentication design.

## CLI

Readiness:

    cargo run -p dragonforge-test-lab -- identity-doctor

Real loopback mutual-TLS fixture:

    cargo run -p dragonforge-test-lab -- identity-fixture

The fixture verifies:

- encrypted TLS round trip;
- required client certificate;
- validated server certificate;
- node ID/certificate fingerprint binding;
- overlapping renewal/key rotation;
- old-generation retirement;
- certificate revocation.

## Validation

Run:

    .\scripts\test-phase12.ps1

The validation performs:

1. environment inspection;
2. cargo fmt --check;
3. strict Clippy;
4. full workspace tests;
5. mTLS identity doctor;
6. real mutual-TLS fixture;
7. distributed compatibility doctor/fixtures;
8. general doctor Phase 12 check;
9. GitHub-aware native worker regression.

The script writes:

    test-logs\phase12-validation-*.log

## Security boundary

Phase 12 does not introduce:

- arbitrary remote commands;
- inbound worker listeners;
- arbitrary TLS verification bypass;
- certificate acceptance without CA validation;
- node identity acceptance based only on a claimed string;
- private-key persistence in trust metadata;
- public-Internet controller exposure.

mTLS authenticates and encrypts transport. Typed work still requires the existing worker-side authorization boundary.

## Legacy HMAC transport

Phase 8 HMAC envelopes remain in the codebase for backward compatibility and private-lab debugging.

They still provide:

- message authentication;
- integrity;
- nonce replay protection.

They do not provide transport confidentiality by themselves.

New service-oriented worker/controller flows beginning in Phase 13 should prefer Phase 12 mTLS.

## Exit criteria

Phase 12 is complete when validation proves:

- the workspace builds cleanly with rustls/rcgen;
- a real mutual-TLS connection succeeds;
- the server observes a client certificate;
- the client observes/validates the server certificate;
- identity binding works;
- renewal overlap works;
- revoked certificates fail closed;
- existing distributed/HMAC fixtures still pass;
- existing GitHub-aware execution remains green.


## Validation status

Phase 12 validation completed successfully on the Windows host on 2026-09-22. The final run passed formatting, strict Clippy, the full workspace and doc-test suite, all mTLS/node-identity unit tests, the real mutual-TLS fixture, certificate renewal/revocation checks, legacy distributed/HMAC compatibility, the Phase 12/general doctors, and the GitHub-aware native worker regression.
