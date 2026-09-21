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
13. Windows native execution is assigned to a Job Object before the suspended child is resumed.
14. Memory and active-process ceilings are kernel-enforced for Windows native jobs and runtime-enforced for Docker/Podman project actions.
15. Dedicated worker identity can be required without placing account passwords in Test Lab configuration or job payloads.

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

## Phase 3 sandbox boundary

On Windows native mode, each fixed Git/Cargo command receives a fresh Job Object configured with kill-on-close, the job memory limit, and the active-process limit. The command is created suspended, assigned to the Job Object, and only then resumed. Timeout/cancellation terminates the entire Job Object. When the leader exits, closing the Job Object reaps descendants that remain alive.

Docker/Podman mode wraps only fixed Cargo project actions. Test Lab supplies a fixed Rust image, drops all container capabilities, requests no-new-privileges, enforces memory/PID limits, and bind-mounts only the checked-out repository.

Worker identity may be constrained with `--worker-user`. On Windows, Test Lab reads the process account from the operating system with `GetUserNameW` rather than trusting the `USERNAME` environment variable. It fails before repository execution on mismatch and does not create accounts, accept passwords, or impersonate users.

Phase 3 does not claim outbound-network isolation. Native execution retains normal worker networking and containers use runtime-default networking so dependency resolution can function.

## Current enforcement

Phases 1-3 enforce repository allowlisting, HTTPS URLs, worker capabilities, protocol compatibility, total job timeout, bounded captured output, post-step disk usage ceilings, sanitized executor environments, GitHub repository/ref validation, immutable commit resolution, typed commit-status reporting, Windows process-tree containment, Windows aggregate memory/process ceilings, whole-tree cancellation/timeout, optional dedicated worker identity, and Docker/Podman project-code isolation.

## Sandbox and distributed-node work still required

Before truly hostile third-party code is considered safely isolated, later phases must add disposable VM boundaries, live disk quotas or disposable filesystems, outbound network policy, authenticated controller-agent transport, immutable audit records, artifact hashing, and stronger privilege/network isolation. Phase 3 provides meaningful process/container containment but Phase 4 VMs remain the preferred hostile-code boundary.

Before distributed nodes are enabled, add mutual controller/agent authentication, short-lived node credentials, replay protection, node identity, connection health/lease expiry, explicit high-risk capability approvals, and encrypted transport. Nodes should not expose a general remote shell.

Before VM, GUI, or MCP support, also add VM snapshot rollback and a user-visible audit trail.

The project must not evolve into an unrestricted remote shell.
