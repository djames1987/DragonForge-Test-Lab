# Phase 2 — GitHub Integration

Status: implementation complete; local validation pending.

## Goal

Phase 2 connects DragonForge Test Lab to GitHub without making GitHub Actions the execution engine. Test Lab remains responsible for policy, checkout, execution, artifacts, and results. GitHub supplies repository/ref identity and receives commit status updates.

## Delivered

- New df-test-github crate.
- Strict parsing of GitHub HTTPS repository URLs.
- Strict revision validation.
- GitHub branch/tag/ref resolution to an immutable 40-character commit SHA.
- GitHub CLI authentication health check.
- Fixed gh CLI argument construction with no shell invocation.
- GitHub commit status lifecycle:
  - pending when validation starts;
  - success when validation passes;
  - failure when the test job completes unsuccessfully;
  - error when the Test Lab execution path itself errors.
- Stable status context: dragonforge/test-lab.
- run-github CLI command.
- github-doctor CLI command.
- Phase 2 PowerShell validation script with timestamped transcript output.

## GitHub-aware execution flow

    Operator
      -> run-github
      -> validate GitHub repository URL
      -> verify gh authentication
      -> resolve requested revision to exact commit SHA
      -> set commit status pending
      -> Agent policy validation
      -> LocalExecutor
      -> clone repository
      -> fetch exact commit SHA
      -> fixed Cargo checks
      -> write artifacts/report
      -> set commit status success/failure/error

The exact SHA is passed into the executor rather than the mutable branch name, so the tested source cannot silently move after GitHub resolution.

## Authentication

Phase 2 does not accept GitHub tokens in Test Lab job payloads or command-line flags. It uses the existing authenticated GitHub CLI installation on the operator machine.

Run:

    cargo run -p dragonforge-test-lab -- github-doctor

before a GitHub-aware test. If the GitHub CLI is not authenticated or lacks access to a private repository, the operation fails before execution.

## Commands

Resolve a GitHub ref, report status, and execute it locally:

    cargo run -p dragonforge-test-lab -- run-github --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-2-github-integration

To perform GitHub resolution but suppress commit-status writes:

    cargo run -p dragonforge-test-lab -- run-github --repo https://github.com/djames1987/DragonForge-Test-Lab.git --revision phase-2-github-integration --no-status

## Validation

On Windows:

    .\scripts\test-phase2.ps1 -RepositoryUrl https://github.com/djames1987/DragonForge-Test-Lab.git -Revision phase-2-github-integration

The script writes a timestamped transcript beneath test-logs for upload and review.

## Security boundary

The GitHub adapter invokes gh directly with fixed argument vectors. It never invokes cmd.exe, PowerShell, sh, or another shell. Repository owner/name, revisions, and commit SHAs are validated before being used to construct GitHub API endpoints.

Phase 2 deliberately does not provide arbitrary GitHub API calls, arbitrary workflow commands, or arbitrary gh arguments.

## GitHub Actions

GitHub Actions is an optional trigger/execution interface, not a foundational dependency. A self-hosted runner can later submit or execute Test Lab jobs, but the controller/agent/executor architecture remains usable independently of Actions minutes and runner availability.
