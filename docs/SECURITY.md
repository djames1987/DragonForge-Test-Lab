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
56. Phase 13 workers initiate controller connections outbound over mTLS and do not expose inbound worker listeners.
57. Drain mode stops new job admission immediately while preserving already-active job state for graceful shutdown; running services refresh the persisted drain/resume control before heartbeats.
58. Worker restart snapshots contain lifecycle metadata only; TLS private keys remain file-backed operator secrets and are not serialized into runtime state.
59. Windows service launch metadata is fixed to the DragonForgeTestWorker service identity and internally generated worker-service command shape.
60. Service reconnect uses bounded exponential backoff and a restart never treats a previously online session as still authenticated.
61. Phase 14 structured logs are bounded and redact known secret-bearing field names before JSONL or SQLite persistence.
62. New Phase 14 audit records are SHA-256 chained using their SQLite audit sequence and prior event digest; audit-chain verification is integrity evidence, not a digital signature.
63. Artifact cataloging and deletion canonicalize paths beneath a configured artifact root, reject symlinks/traversal, and never delete arbitrary caller-selected filesystem paths.
64. Telemetry pruning applies only to structured logs and metric samples; audit history is not deleted by the telemetry-prune operation.
65. Metrics must use validated names and finite values and do not themselves authorize scheduling or execution.
66. Phase 15 test failures are never automatically retried; retryable failure classification must be explicit.
67. Automatic retry is limited to persisted retry policy with a hard global ceiling of five attempts and bounded exponential delay.
68. Restart-interrupted jobs are not automatically replayed unless their persisted policy explicitly enables interrupted retry.
69. Retry-pending jobs remain unschedulable until their durable next-retry timestamp and still pass normal capability/policy authorization when reassigned.
70. Manual interrupted-job rescheduling is allowed only from interrupted state, is hash-chain audited, and remains subject to the global five-attempt ceiling.
71. Cancelling a retry-pending job clears its retry due time so cancelled work cannot later become eligible.
72. Phase 16 plans compile only to existing typed TestAction values; plan JSON cannot supply executables, shell text, arbitrary arguments, or PowerShell.
73. Plan-declared extra capabilities are unioned into JobRequest required capabilities and therefore restrict eligible workers rather than granting authority.
74. Plan dependencies must form a bounded DAG; missing/self/cyclic dependencies fail closed before persistence or compilation.
75. Plan resource limits, retry policies, artifact classes, target OS, and node labels are bounded and validated before use.
76. Plan artifact declarations are typed classes rather than caller-controlled filesystem paths/globs.
77. Stored plans are validated before schema-v4 persistence, and create/update operations are hash-chain audited.
78. Target OS/node-label predicates restrict scheduling candidates; they do not authorize a worker or bypass Agent/Policy/Executor checks.

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

## Phase 13 worker-service boundary

Phase 13 makes the worker long-running without turning it into a remote shell. Workers connect outbound to the configured private/local controller over Phase 12 mTLS, then exchange only typed service registration and heartbeat frames. The service runtime exposes lifecycle controls such as drain/resume but does not accept executable command text.

Windows uses a native Service Control Manager dispatcher and handles Stop by setting a shared stop flag, entering drain state, persisting the snapshot, and reporting Stopped. Linux systemd metadata includes restart behavior and service hardening. Runtime snapshots contain no TLS certificate or private-key bytes.

## Phase 14 audit / artifact / observability boundary

Phase 14 records additional operational state without adding execution authority. Structured fields are validated and bounded before persistence, and known sensitive field names are replaced with `<redacted>`. This is a defensive redaction layer, not a guarantee that arbitrary free-form messages can never contain sensitive content; callers should still avoid placing secrets in log messages.

New audit rows contain a hash of their sequence, previous digest, timestamp, type/entity metadata, and event detail. The chain makes modification/reordering of Phase 14 chained events detectable through the controller verifier. It is not externally signed or immutable against an attacker who can rewrite the database and recompute the entire chain; later release/security phases can add stronger external anchoring if required.

Artifact retention is deliberately root-contained. Cataloging and pruning canonicalize the configured artifact root and target file, reject symlinks and parent traversal, and remove only regular files under the root. Telemetry pruning deletes only old structured-log and metric rows and leaves audit history intact.

## Phase 15 recovery / retry / lifecycle boundary

Phase 15 introduces retry as a typed controller decision, not as a generic process restart mechanism. Failure classification is explicit. The legacy completion path treats ordinary failed tests as `test_failure`, preserving fail-closed behavior. Only `infrastructure_transient` is automatically retryable by default; `interrupted` additionally requires a persisted opt-in.

Retry policies are bounded to five total attempts, bounded delays, and persisted due timestamps. A retry-pending job must wait until its due timestamp and then re-enter the normal capability-aware scheduling path. It does not retain execution authorization from an earlier attempt.

Controller restart closes uncertain assigned/running attempts as interrupted. Jobs without interrupted-retry opt-in remain interrupted. Operators may explicitly reschedule an interrupted job, but only while it is in that state and only below the global attempt limit. Every lifecycle transition is written to the existing hash-chained audit stream.

## Phase 16 test-plan boundary

Phase 16 adds a declarative orchestration format, not a scripting language. Profiles expand only to existing typed TestAction enum values, and the optional typed_actions form can contain only those same enum values. There is no field for an executable, shell, raw arguments, or arbitrary environment mutation.

Extra plan capabilities are added to the JobRequest required-capability set, so a plan can only make worker eligibility stricter. Plans cannot grant capabilities. Dependency graphs are bounded and must be acyclic. Resource limits and Phase 15 retry policies are validated before compilation.

OS and node-label constraints are exposed as explicit target predicates. The older durable worker record does not contain label inventory, so label enforcement belongs to the plan/distributed orchestration layer before a compiled job is submitted; worker-side policy remains authoritative afterward.

Schema v4 persists validated plan JSON and hash-chain audits create/update events. Persistence does not imply authorization or execution.

## Current enforcement

Phases 1-19 enforce repository allowlisting, HTTPS URLs, worker capabilities, protocol compatibility, total job timeout, bounded captured output, post-step disk usage ceilings, sanitized executor environments, GitHub repository/ref validation, immutable commit resolution, typed commit-status reporting, Windows process-tree containment, Windows aggregate memory/process ceilings, whole-tree cancellation/timeout, optional dedicated worker identity, Docker/Podman project-code isolation, typed Hyper-V VM lifecycle control, managed VM namespacing, golden-image containment, differencing disks, and deterministic checkpoint rollback.

## Sandbox and distributed-node work still required

Phase 4 provides disposable VM lifecycle and rollback, but truly hostile third-party code still requires careful network isolation, patched hosts/hypervisors, immutable audit records, artifact hashing, and stronger privilege/network controls. Hyper-V reduces host exposure but does not make hypervisor escape impossible.

Phase 8 now adds authenticated node messages, replay protection, node identity, lease expiry, outbound-only connection policy, and typed capability scheduling. The current transport is deliberately constrained to private/link-local networks and does not provide confidentiality itself; deployments on untrusted networks still require VPN/mTLS encryption. Nodes must not expose a general remote shell.

Before Internet-facing MCP or distributed control, add standards-compliant OAuth/resource metadata, certificate-backed transport identity, durable audit storage, credential rotation, and explicit high-risk capability approval workflows.

The project must not evolve into an unrestricted remote shell.


## Phase 17 — Intelligence Integration

Phase 17 does not turn Test Intelligence into a general execution authority.

Security properties:

- GitHub comparison accepts validated repository/revision inputs and returns a bounded changed-file set.
- Historical regression input is reconstructed only from durable jobs that have an intelligence context and an explicit `test_failure` classification.
- Worker eligibility is derived from existing typed capabilities and online durable state.
- Advisory mode never enqueues jobs.
- Automatic mode has an explicit score threshold and requires a live eligible worker recommendation.
- Only exact `rust_fast` and `rust_standard` Phase 16 profiles can cross the automatic bridge.
- Automatic steps must be dependency-free, target `any`, and have no node labels.
- Jobs are compiled through Phase 16 and pinned to an immutable head SHA.
- Existing Agent/Policy/Executor and retry/capability checks remain authoritative.
- Intelligence decisions are persisted and hash-chain audited.

The current live-capacity bridge deliberately uses a conservative single-slot scheduling model because durable worker registration does not yet carry full memory/load/service-parallelism telemetry. It must not be interpreted as a complete host-resource monitor.


## Phase 18 — Dashboard boundary

The dashboard is a local visibility surface, not a remote administration shell.

Security properties:

- bind validation rejects non-loopback addresses and port zero;
- API authentication requires a 32–4096 byte bearer token;
- the retained dashboard configuration stores only a SHA-256 token digest, not the plaintext token;
- token comparison is constant-time over fixed-size digests;
- the browser receives the token through the URL fragment, which is not sent to the HTTP server, then removes the fragment from browser history and uses an Authorization header;
- all controller data routes are an explicit GET-only allowlist;
- unknown API routes return 404 and all non-GET methods return 405;
- controller projections use bounded query limits;
- artifact views expose metadata only, not file contents;
- settings expose dashboard security posture rather than arbitrary controller configuration values;
- audit responses include current hash-chain verification;
- responses disable caching, framing, MIME sniffing, referrer leakage, and unrestricted script/style/connect origins with fixed security headers;
- there is no terminal, executable/argument field, raw command path, raw SQL path, filesystem browser, generic proxy, or state-mutating HTTP action.

Phase 18 does not add a database migration. Opening the controller uses the existing schema-v5 migration logic; normal Phase 18 operation reads existing durable state through typed controller methods.


## Phase 19 — Linux qualification boundary

Linux native execution now has an explicit containment implementation rather than failing over to an uncontained process.

For native Linux actions, Test Lab creates a new session/process group before exec, applies typed address-space and process-count rlimits, records the spawned process-group leader, and targets that group for cancellation. Container execution remains restricted to the existing fixed Cargo wrapper with capability dropping, no-new-privileges, memory ceilings, PID ceilings, and a controlled workspace mount.

These mechanisms are defense-in-depth. They do not turn arbitrary executables or shell text into allowed inputs. Repository allowlisting, typed TestAction authorization, worker capability checks, sanitized execution, outbound-only service operation, private/local controller targeting, and mTLS identity remain unchanged.

The Linux qualification fixture uses only fixed test programs and existing typed service/identity fixtures. No remote terminal, generic process endpoint, arbitrary systemd command, or caller-supplied shell operation is introduced.


## Phase 20 ARM / Raspberry Pi controls

Physical ARM nodes do not receive a generic hardware or shell escape hatch. Phase 20 exposes only typed read-only probes over fixed Linux metadata locations. Metadata reads are size-bounded, device enumeration is count-bounded, probe names are allowlisted, and callers cannot provide arbitrary paths, bus addresses, GPIO values, UART payloads, SPI transfers, or I²C writes.

ARM/Raspberry Pi capabilities are scheduling metadata, not authorization to execute arbitrary hardware actions. Distributed workers remain outbound-only and authenticated under the existing transport/identity model. Any future mutating HIL capability must introduce its own typed operation, explicit bounds, privilege model, policy authorization, and destructive confirmation where appropriate.


## Phase 21 installer / upgrade controls

Release manifests bind version, target OS, target architecture, binary filename, and SHA-256 digest. Binary names are restricted to safe leaf names; manifest/config/state inputs are size-bounded; upgrades must move to a strictly newer stable semantic version; and production install roots are fixed per platform.

Installers stop the worker service before replacing an existing binary and retain only the immediate prior binary/install-state as the authoritative rollback generation. Uninstall preserves configuration, state, logs, and backups unless purge is explicitly requested. Phase 21 adds no inbound management listener, arbitrary command execution, or remote-shell behavior.


## Phase 22 release engineering controls

Release engineering does not grant new runtime execution authority. Channel and artifact metadata are validated separately from the Agent/Policy/Executor path.

Stable release bundles fail closed if Windows/Linux package signature metadata is missing. Windows stable binaries use Authenticode through an externally supplied PFX and password; release archives use detached minisign signatures supplied from an external secret key. Repository source, release manifests, logs, and generated artifacts must never contain private signing keys or signing passwords.

SBOM generation is deterministic from `Cargo.lock` and emits CycloneDX 1.6 JSON. Release audit gates use `cargo audit` plus `cargo deny` license/advisory/source checks. SHA-256 indexes protect artifact integrity but are not substitutes for signatures.

The release workflow accepts only the explicit dev/beta/stable channel enum and validated version forms. Publication occurs only after formatting, strict Clippy, workspace tests, audit/SBOM generation, platform builds, bundle assembly, and release-bundle verification. Stable publication additionally requires signing material; missing secrets cause the workflow to fail rather than silently publish unsigned artifacts.
