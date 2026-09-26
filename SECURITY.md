# DragonForge Test Lab Security Policy

DragonForge Test Lab executes and coordinates potentially untrusted test work, so security-sensitive findings must not be disclosed in ordinary public issues.

## Current status

DragonForge Test Lab is an active development/testing platform. The current project line has completed the Phase 25 dogfooding milestone with Windows qualification recorded in the repository. This is not a claim of independent security certification or production suitability for every environment.

The detailed security model and execution invariants are maintained in [docs/SECURITY.md](docs/SECURITY.md).

## Reporting a vulnerability

Use GitHub's private security-advisory reporting flow for this repository when available:

`https://github.com/djames1987/DragonForge-Test-Lab/security/advisories/new`

If that private flow is unavailable, contact the repository owner through a private GitHub channel. Do **not** open a public issue containing exploit details, credentials, tokens, private keys, proprietary test data, sensitive logs, or evidence from systems you do not own or have permission to test.

A useful private report includes the affected commit/version, affected component, reproduction steps using synthetic data, expected versus observed behavior, impact, and sanitized evidence.

## Scope and safety

Security reports may include authorization or scope bypasses, unsafe executor behavior, sandbox/VM/container boundary failures, secret exposure, authentication weaknesses, evidence-integrity failures, or other defects that could make Test Lab operate outside its configured safety model.

Only test systems and data you own or are explicitly authorized to test.