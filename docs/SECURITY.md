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
16. Hyper-V management accepts only validated typed VM operations; arbitrary PowerShell is not exposed.
17. Destructive VM operations are limited to the DragonForge-* namespace and explicit confirmation.
18. Golden VHDX parents must remain beneath the configured image root and are consumed through differencing children.
19. Phase 5 deep-test profiles remain repository-maintained fixed commands; fuzz bytes never become command text.
20. Nightly Miri/sanitizer and cargo-fuzz lanes are explicit opt-ins and should run in disposable workers/VMs for untrusted repositories.
21. Windows OS mutation is restricted to internally generated DragonForge fixture names and known fixture locations.
22. Service/Event Log mutation requires elevation plus explicit confirmation; callers cannot provide service binary paths or Event Log command text.
23. MSI support is inspection-only in Phase 6; Test Lab does not install, repair, uninstall, or execute caller-supplied installers.
24. GUI plans may target only DragonForge-* managed windows and validated AutomationIds.
25. GUI actions are limited to typed wait, value, invoke, assertion, and screenshot operations; raw input injection and arbitrary UI Automation patterns are not exposed.
26. Screenshot artifacts are leaf PNG files contained beneath the selected artifact root.
27. Crash capture is limited to Test Lab-owned fixture processes in Phase 7 validation.

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

## Phase 4 VM boundary

Phase 4 introduces a separate Hyper-V orchestration boundary. Test Lab generates fixed PowerShell templates for known Hyper-V cmdlets rather than accepting scripts from callers.

Managed VM names must begin with DragonForge-. VM/checkpoint/switch names use restricted character sets, memory and CPU values are bounded, golden VHDX paths are verified beneath the configured image root, and VM deletion is limited to the managed namespace plus an explicit --confirm CLI gate.

Managed instances use differencing disks so the golden parent image is not intentionally modified. Creation failures attempt to remove any partially registered VM and instance storage.

Standard Hyper-V checkpoints are used for the DragonForge-Baseline rollback point. Restoring the baseline powers the VM off, applies the checkpoint, and starts the VM again.

The VM layer does not expose a general host-to-guest shell. Guest execution remains a later typed/authenticated transport problem.

## Phase 5 deep Rust testing boundary

Phase 5 adds deeper local validation without adding a generic command field to the protocol. nextest, llvm-cov, property tests, benchmarks, Miri, sanitizer, and fuzz lanes are invoked through repository-maintained fixed scripts and known Cargo subcommands.

The fuzz harness consumes arbitrary bytes only as serialized JobRequest input. Valid decoded requests may be inspected, have capabilities derived, and be reserialized; fuzz data is never interpreted as a shell command, program name, or argument vector.

Miri and sanitizer runs require a nightly Rust toolchain and are opt-in. The AddressSanitizer lane is restricted to documented supported Linux targets in the Phase 5 script. cargo-fuzz execution is likewise intended for a disposable Linux worker/VM when the repository under test is untrusted.

## Phase 6 Windows integration boundary

Phase 6 introduces fixed Windows OS integration fixtures. The safe lane writes only beneath the current user's DragonForge registry fixture key, launches only a fixed Windows executable directly, and binds network fixtures only to 127.0.0.1 on ephemeral ports.

The privileged lane is separate. It requires an elevated token and explicit `--confirm`, generates a `DragonForge-TestLab-*` service name internally, creates/queries/deletes that SCM entry without starting it, and writes one informational Application Event Log entry. Service executable paths, service names, registry scripts, Event Log commands, credentials, and impersonation data are not caller-controlled.

MSI support is read-only inspection. Paths must resolve to existing .msi files; Authenticode status is queried, but Windows Installer execution is not invoked.

## Phase 7 GUI automation boundary

Phase 7 uses Windows UI Automation only against managed window titles beginning with `DragonForge-`. Plans are deserialized into a fixed action enum and independently validated before any UI Automation operation is generated.

Control lookup uses exact AutomationId matching. Set/assert operations use ValuePattern and button-like activation uses InvokePattern. Plans cannot provide PowerShell fragments, executable names, COM pattern identifiers, raw keyboard input, mouse coordinates, or arbitrary desktop automation commands.

Screenshots are restricted to the managed target window's UI Automation bounding rectangle and are written as validated leaf PNG files beneath the artifact directory.

The crash-capture validation launches only the repository-maintained `phase7-gui-fixture.ps1`, owns the resulting child process, invokes a fixed crash fixture control, waits with a bounded timeout, and records the observed exit code.

## Current enforcement

Phases 1-7 enforce repository allowlisting, HTTPS URLs, worker capabilities, protocol compatibility, total job timeout, bounded captured output, post-step disk usage ceilings, sanitized executor environments, GitHub repository/ref validation, immutable commit resolution, typed commit-status reporting, Windows process-tree containment, Windows aggregate memory/process ceilings, whole-tree cancellation/timeout, optional dedicated worker identity, Docker/Podman project-code isolation, typed Hyper-V VM lifecycle control, managed VM namespacing, golden-image containment, differencing disks, and deterministic checkpoint rollback.

## Sandbox and distributed-node work still required

Phase 4 provides disposable VM lifecycle and rollback, but truly hostile third-party code still requires careful network isolation, patched hosts/hypervisors, immutable audit records, artifact hashing, and stronger privilege/network controls. Hyper-V reduces host exposure but does not make hypervisor escape impossible.

Before distributed nodes are enabled, add mutual controller/agent authentication, short-lived node credentials, replay protection, node identity, connection health/lease expiry, explicit high-risk capability approvals, and encrypted transport. Nodes should not expose a general remote shell.

Before VM, GUI, or MCP support, also add VM snapshot rollback and a user-visible audit trail.

The project must not evolve into an unrestricted remote shell.
