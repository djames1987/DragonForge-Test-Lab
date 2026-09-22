# Phase 18 — Dashboard

Phase 18 adds a local operator web dashboard over the durable Test Lab controller.

## Scope

The dashboard is deliberately read-only. It exposes bounded projections of:

- recent jobs and lifecycle state;
- registered workers and online state;
- stored Phase 16 test plans;
- recent artifact metadata;
- persisted Phase 17 intelligence records, including stored failure-cluster information;
- recent hash-chained audit history plus current audit-chain verification;
- dashboard security/settings metadata.

It does not submit jobs, cancel jobs, reschedule jobs, mutate plans, change controller configuration, execute commands, run SQL, browse the filesystem, or expose raw artifact contents.

## Crate

`crates/df-test-dashboard` contains the loopback HTTP service, bearer-token authentication, bounded request parser, fixed route table, static dashboard assets, controller projections, and deterministic Phase 18 fixture.

## Security model

`DashboardConfig` requires a non-zero loopback bind, a 32–4096 byte bearer token, and a durable controller database path. The plaintext token is SHA-256 hashed during construction and then removed from retained configuration; authentication uses a constant-time digest comparison.

The HTML shell and static CSS/JavaScript are non-sensitive and may be fetched without authentication. Every `/api/*` route requires the bearer token. The browser bootstrap accepts the token only from the URL fragment (`#token=...`), moves it into `sessionStorage`, removes the fragment from browser history, and sends it only as an `Authorization: Bearer ...` header.

Responses add `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, and a restrictive Content Security Policy. Only GET is accepted; other methods return 405.

## Routes

Public shell/health routes: `/`, `/dashboard.js`, `/dashboard.css`, `/health`.

Authenticated read-only API routes: `/api/overview`, `/api/jobs`, `/api/workers`, `/api/plans`, `/api/artifacts`, `/api/intelligence`, `/api/audit`, `/api/settings`. Unknown API routes return 404.

## Controller projections

Phase 18 adds bounded read methods to `DurableController`: `recent_jobs`, `recent_artifact_records`, `recent_intelligence_records`, and `recent_audit_events`. Phase 18 does not require a new database migration; the controller remains schema v5.

## CLI

Required environment:

    $env:DRAGONFORGE_DASHBOARD_TOKEN = "<at-least-32-random-characters>"

Readiness:

    cargo run -p dragonforge-test-lab -- dashboard-doctor

Deterministic fixture:

    cargo run -p dragonforge-test-lab -- dashboard-fixture

Serve the dashboard:

    cargo run -p dragonforge-test-lab -- dashboard-serve

Default URL:

    http://127.0.0.1:8788/#token=<DRAGONFORGE_DASHBOARD_TOKEN>

Alternate state database and loopback bind are supported with `--state-db` and `--bind`. Public/non-loopback bind addresses are rejected.

## Validation

Run:

    .\scripts\test-phase18.ps1 -Revision main

The validation performs formatting, strict Clippy, full workspace tests, dashboard-specific tests, controller/schema checks, dashboard doctor/fixture checks, Phase 17/16/15/14 regressions, the general Phase 18 doctor, and a GitHub-aware native worker regression.

## Deliberate limits

Phase 18 is an operator visibility surface, not a second controller API. Future typed operator actions must remain explicit, bounded, auditable, and routed through existing controller/lifecycle policy.

DragonForge Test Lab remains not a remote shell.
