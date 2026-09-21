# Security Model

DragonForge Test Lab treats every remotely requested job as untrusted input.

## Phase 0 invariants

1. No API field accepts arbitrary shell or PowerShell command text.
2. Workers advertise explicit capabilities.
3. Workers independently authorize jobs; controller approval is insufficient.
4. Repository access is allowlist based and HTTPS only.
5. Resource requests are bounded before execution.
6. Protocol versions must match before a worker is accepted.
7. Artifacts are represented by relative paths; future implementations must canonicalize paths and prevent traversal.
8. Secrets must never be placed in job payloads or logs.

## Future mandatory controls

Before process execution is enabled:
- dedicated low-privilege worker account;
- workspace root canonicalization;
- environment allowlist and secret scrubbing;
- process tree termination on timeout;
- CPU, memory, disk, and process limits;
- outbound network policy;
- immutable audit records;
- authenticated controller-agent transport;
- artifact size/hash validation.

Before VM/GUI/MCP support:
- short-lived credentials;
- mutually authenticated transport;
- replay protection;
- explicit high-risk capability approvals;
- VM snapshot rollback;
- user-visible audit trail.

The project must not evolve into an unrestricted remote shell.
