# Public Screenshot Capture Checklist

Phase 5 does not fabricate dashboard or worker screenshots. Capture these only from a disposable synthetic lab after the UI is running in its real environment.

## Capture set

1. `docs/assets/readme/dashboard-overview.png` — loopback operator dashboard with synthetic jobs/workers/plans.
2. `docs/assets/readme/test-run-detail.png` — a completed synthetic run showing status and artifact metadata without filesystem paths or private repository data.
3. `docs/assets/readme/worker-inventory.png` — synthetic Windows/Linux/ARM worker inventory using generic node labels.
4. Optional `docs/assets/readme/plan-view.png` — declarative plan view with a harmless demo repository/profile.

## Synthetic-only data

Use neutral names such as `demo-controller`, `windows-demo-01`, `linux-demo-01`, `arm-demo-01`, and `example/project`. Do not display real hostnames, usernames, home paths, tokens, certificates, private repository URLs, external account identifiers, real test history, or the historical mailbox value covered by `DF-P2-TL-002`.

## Capture and sanitization

- Capture only the application/browser content area; exclude desktop chrome and terminal history.
- Prefer PNG at 1600×900 or 1440×900 and optimize before committing.
- Inspect every pixel at 200% for machine-specific paths, certificate identity, private network addresses, access tokens, repository credentials, personal contact data, or historical artifacts.
- Record capture date, source commit, synthetic dataset, dimensions, optimized size, and reviewer in the public-readiness report.
- Do not rewrite Git history as part of screenshot work; `DF-P2-TL-002` remains a separate publication blocker.
