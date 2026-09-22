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
28. Distributed agents must initiate outbound-only connections; inbound agent listeners are rejected.
29. Distributed node messages use HMAC-SHA256 with nonce replay protection and bounded clock skew.
30. Controller targets are restricted to loopback, private, or link-local addresses; public Internet controller targets are rejected.
31. Remote Phase 8 work is typed; the built-in cross-node probe exposes only the fixed NetworkFixtureSuite task.
32. Distributed result manifests are bounded and carry SHA-256 artifact digests.
33. Phase 8 fault injection is fixture-local delay/drop behavior only; Test Lab does not modify firewall, routes, NIC configuration, packet filters, or system DNS.
34. The Phase 9 MCP gateway may bind only to loopback addresses.
35. Every /mcp request requires a bearer token sourced only from DRAGONFORGE_MCP_TOKEN; the raw token is not retained after initialization.
36. MCP repository targets must match operator-configured HTTPS allowlist entries; owner/org entries ending in `/` act as prefixes, while repository entries are exact identities after optional `.git` normalization.
37. MCP job submission accepts only named typed profiles; arbitrary executable, shell, PowerShell, Cargo, or process-argument fields are not exposed.
38. MCP HTTP headers and bodies are bounded before JSON-RPC dispatch.
39. Modern MCP transport headers must agree with the JSON-RPC method/name before tool execution.
40. MCP result retrieval excludes raw filesystem access and returns SHA-256 artifact metadata only.
41. The MCP gateway permits at most four queued/running jobs and rejects new submissions above that limit.
42. The MCP HTTP parser rejects duplicate headers and transfer-encoding to avoid ambiguous request framing.
43. Phase 10 intelligence accepts only bounded typed change/history/worker data and never accepts command text.
44. Intelligence recommendations cannot execute jobs or bypass worker capability/policy checks.
45. Resource-aware scheduling must explicitly report unscheduled profiles rather than silently weakening requested coverage.
46. Failure clustering fingerprints normalized bounded text with SHA-256 and does not treat historical failure text as executable input.
47. Phase 11 SQLite state is controller-owned and cannot bypass worker-side Agent/Policy authorization.
48. SQL statements and migrations are fixed in Test Lab; jobs and remote clients cannot submit arbitrary SQL.
49. Restart recovery marks uncertain assigned/running work interrupted instead of silently treating it as completed or automatically executing it again.
50. Databases with a schema version newer than the running binary fail closed.
51. Phase 12 mutual TLS requires certificate-chain validation against configured CA roots and server-side client certificates.
52. Node authorization binds the claimed node ID to an enrolled SHA-256 end-entity certificate fingerprint; CA issuance alone is not node authorization.
53. Certificate renewal uses explicit bounded overlap between generations, and revoked certificates fail closed.
54. Serialized identity trust state contains certificate fingerprints/lifecycle metadata only and does not persist private keys.
55. The built-in direct controller address policy remains loopback/private/link-local even when mTLS is enabled.

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

## Phase 8 distributed boundary

Phase 8 introduces authenticated distributed node contracts without adding a remote shell. Node registrations, heartbeats, commands, acknowledgements, and results are wrapped in HMAC-SHA256 authenticated envelopes with random nonces and issue timestamps. The verifier rejects stale messages, duplicate nonces, unknown keys, invalid MACs, protocol mismatches, and key/node identity mismatches.

Agents declare `outbound_only=true`; registrations that request an inbound agent listener are rejected. The built-in outbound client accepts only loopback, RFC1918/private, link-local, IPv6 loopback, IPv6 unique-local, or IPv6 link-local controller addresses.

The shared node secret is read from `DRAGONFORGE_NODE_SHARED_SECRET`; it is not accepted as a CLI argument or emitted in logs. The current Phase 8 transport authenticates and integrity-protects messages but does not encrypt them. Cross-host use should therefore remain on a trusted private network or VPN. Internet/untrusted-network use requires an additional confidentiality layer such as WireGuard/Tailscale or future mTLS.

The cross-node validation path sends a fixed typed `NetworkFixtureSuite` command and receives a signed `NodeResultManifest`. It cannot send executable paths, arbitrary command lines, shell fragments, firewall commands, packet-capture instructions, or raw process arguments.

## Phase 9 MCP gateway boundary

Phase 9 exposes a loopback-only HTTP MCP endpoint. Binding to 0.0.0.0, LAN addresses, or public addresses is rejected by configuration validation.

Every POST to /mcp requires a Bearer token read from DRAGONFORGE_MCP_TOKEN. The token must be at least 32 characters. The gateway hashes it with SHA-256 during construction, clears the raw token from retained configuration, and compares fixed-length digests without early exit. The token is not accepted on the command line.

MCP-submitted repositories must match DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES and use HTTPS. Job calls select only a fixed named profile (rust_standard or rust_test); those profiles map to existing typed TestAction values and then pass through Agent policy validation before LocalExecutor execution.

The gateway bounds HTTP headers to 16 KiB and request bodies to 256 KiB. It validates JSON-RPC 2.0, modern MCP protocol/method/name headers, and rejects header/body mismatches before dispatch.

The MCP result surface exposes job status, bounded step metadata, and artifact metadata. Artifact paths are canonicalized beneath the LocalExecutor artifact root and SHA-256 hashed. Phase 9 does not expose raw artifact file reads, arbitrary filesystem browsing, or raw stdout/stderr through MCP.

The gateway is intentionally local. Its static bearer authentication is defense-in-depth for a loopback service, not a standards-compliant Internet-facing OAuth deployment. Remote exposure requires a separate future design with OAuth/resource metadata, TLS or an authenticated tunnel, credential rotation, and durable auditing.

## Phase 10 intelligence boundary

Phase 10 is advisory. It scores typed TestProfile values, clusters historical failures, and suggests eligible workers. It does not create process argument vectors, execute commands, authenticate nodes, mutate VMs, or authorize repositories. Any recommendation that later becomes a real job must still pass the existing Agent/Policy/Executor and distributed-worker boundaries.

Changed-file paths and historical records are bounded and validated before analysis. Scheduling uses declared support, free memory, parallel-job slots, and load; if no worker satisfies those constraints the profile remains explicitly unscheduled.

## Phase 11 durable-state boundary

Phase 11 persists controller metadata and typed payloads in SQLite. Persistence is not an execution authorization mechanism. Recovered jobs retain their typed JobRequest and must still pass the same worker capability and policy checks before execution.

Controller migrations are fixed source-controlled SQL. The CLI selects only the database path; no API accepts SQL text. In-flight jobs found after restart become `interrupted`, producing an audit event and requiring a later explicit retry/rescheduling decision rather than risking duplicate execution.

## Phase 12 mTLS / node-identity boundary

Phase 12 adds rustls-based mutual TLS configuration and a certificate lifecycle trust store. TLS verifies certificate chains against operator-configured CA roots and requires the worker to present a client certificate. DragonForge then separately verifies the end-entity certificate fingerprint against the enrolled node identity and its generation/validity/revocation state.

Private keys remain in operator-controlled PEM material and are not serialized into the DragonForge identity trust store. Rotation may temporarily accept two generations only within an explicit overlap window. Revocation overrides logical validity and fails closed.

The older HMAC transport remains available for compatibility/private-lab use, but it is not relabeled as encrypted. New service-oriented distributed paths should use mTLS.

## Current enforcement

Phases 1-12 enforce repository allowlisting, HTTPS URLs, worker capabilities, protocol compatibility, total job timeout, bounded captured output, post-step disk usage ceilings, sanitized executor environments, GitHub repository/ref validation, immutable commit resolution, typed commit-status reporting, Windows process-tree containment, Windows aggregate memory/process ceilings, whole-tree cancellation/timeout, optional dedicated worker identity, Docker/Podman project-code isolation, typed Hyper-V VM lifecycle control, managed VM namespacing, golden-image containment, differencing disks, and deterministic checkpoint rollback.

## Sandbox and distributed-node work still required

Phase 4 provides disposable VM lifecycle and rollback, but truly hostile third-party code still requires careful network isolation, patched hosts/hypervisors, immutable audit records, artifact hashing, and stronger privilege/network controls. Hyper-V reduces host exposure but does not make hypervisor escape impossible.

Phase 8 now adds authenticated node messages, replay protection, node identity, lease expiry, outbound-only connection policy, and typed capability scheduling. The current transport is deliberately constrained to private/link-local networks and does not provide confidentiality itself; deployments on untrusted networks still require VPN/mTLS encryption. Nodes must not expose a general remote shell.

Before Internet-facing MCP or distributed control, add standards-compliant OAuth/resource metadata, certificate-backed transport identity, durable audit storage, credential rotation, and explicit high-risk capability approval workflows.

The project must not evolve into an unrestricted remote shell.
