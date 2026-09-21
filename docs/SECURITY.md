# Security Model

DragonForge Test Lab treats every remotely requested job as untrusted input.

## Core invariants

1. No API field accepts arbitrary shell or PowerShell command text.
2. Workers advertise explicit capabilities.
3. Workers independently authorize jobs; controller approval is insufficient.
4. Repository access is allowlist based and HTTPS only.
5. Resource requests are bounded by policy before execution.
6. Protocol versions must match before a worker is accepted.
7. Secrets must never be placed in job payloads or logs.
8. Executor commands are built from typed actions and fixed argument templates.
9. Child processes run with a cleared and narrowly rebuilt environment.
10. Captured stdout/stderr is size bounded and artifact paths stay beneath configured roots.
11. GitHub integration does not accept tokens or arbitrary gh/API arguments from jobs.
12. Mutable GitHub refs are resolved to exact commit SHAs before Test Lab execution.

## Local execution boundary

The local executor launches only executables required by supported typed actions:

- git for clone/fetch/switch;
- cargo for build, test, Clippy, and rustfmt.

It does not invoke cmd.exe, PowerShell, sh, or another command shell.

Repository revisions are validated before Git is invoked so option-like or command-looking values are rejected.

Each job receives a UUID-named workspace beneath the configured workspace root. Artifacts are written beneath a separate artifact root. Completed workspaces are deleted unless retention is explicitly enabled.

## GitHub integration boundary

The Phase 2 GitHub adapter launches gh directly with internally constructed argument vectors.

Allowed GitHub operations are deliberately narrow:
- verify authentication for github.com;
- resolve a validated repository/ref to an exact commit SHA;
- create a commit status with a validated state, context, and description.

The adapter does not expose arbitrary GitHub API endpoints, workflow commands, shell commands, or arbitrary gh arguments. Authentication comes from the operator machine's existing GitHub CLI configuration and is not copied into Test Lab payloads or artifacts.

## Current enforcement

Phases 1-2 enforce repository allowlisting, HTTPS URLs, worker capabilities, protocol compatibility, total job timeout, bounded captured output, post-step disk usage ceilings, one Test Lab-managed direct child at a time, sanitized executor environments, direct-child cancellation, GitHub repository/ref validation, immutable commit resolution, and typed commit-status reporting.

## Sandbox and distributed-node work still required

Before untrusted third-party code is considered safely isolated, later phases must add a dedicated low-privilege worker identity, Windows Job Object or equivalent process-tree containment, kernel-enforced memory/process limits, live disk quotas or disposable filesystems, outbound network policy, authenticated controller-agent transport, immutable audit records, artifact hashing, and stronger sandbox/container/VM isolation.

Before distributed nodes are enabled, add mutual controller/agent authentication, short-lived node credentials, replay protection, node identity, connection health/lease expiry, explicit high-risk capability approvals, and encrypted transport. Nodes should not expose a general remote shell.

Before VM, GUI, or MCP support, also add VM snapshot rollback and a user-visible audit trail.

The project must not evolve into an unrestricted remote shell.
