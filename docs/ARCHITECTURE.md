# Architecture

## Purpose

DragonForge Test Lab is a local-first test orchestration platform designed to grow into a distributed capability-based test cluster. The controller schedules typed jobs. Agents advertise capabilities and enforce their own local execution policy before a job is accepted.

## Components

- df-test-protocol: versioned, serializable contracts shared by controllers and agents.
- df-test-policy: repository, capability, and resource-limit authorization.
- df-test-agent: worker-side trust boundary and protocol compatibility gate.
- df-test-controller: queue/capability-aware scheduling plus SQLite-backed durable controller state, restart recovery, Phase 14 observability, Phase 15 retry/lifecycle persistence, and Phase 16 plan storage/auditing.
- df-test-executor: local workspace, fixed-command process execution, cancellation, output capture, artifact generation, and cleanup.
- df-test-github: typed GitHub repository/ref resolution and commit-status reporting through the authenticated gh CLI.
- df-test-sandbox: native/container sandbox selection, Windows Job Object containment, resource ceilings, worker-identity enforcement, and fixed Docker/Podman wrapping.
- df-test-vm: typed Hyper-V host checks, golden-image differencing VM creation, managed lifecycle, checkpoints, and rollback.
- df-test-windows: typed Windows registry/process/network/service/Event Log/installer fixtures with privilege separation.
- df-test-gui: managed-window UI Automation, deterministic typed plans, screenshots, and owned-process crash capture.
- df-test-distributed: authenticated node envelopes, lease/heartbeat inventory, capability/load-aware multi-node scheduling, framed outbound transport, typed network tasks, and hashed distributed result manifests.
- df-test-mcp: loopback-only authenticated MCP HTTP gateway, protocol/version handling, named tool schemas, bounded asynchronous job registry, typed profile submission, and result/artifact metadata projection.
- df-test-observability: structured log validation/redaction and JSONL rotation, metrics registry/snapshots, SHA-256 artifact cataloging, retention pruning, and audit digest construction.
- df-test-lifecycle: typed failure classes, bounded retry policy, exponential backoff calculation, and lifecycle decisions without process execution.
- df-test-plans: versioned declarative plans, DAG validation, typed profiles/actions, readiness evaluation, target predicates, and compilation into typed jobs.
- dragonforge-test-lab: operator CLI, doctor checks, sandbox preflight, deep-Rust tool readiness, local execution, and GitHub-aware execution entry point.

## Trust model

The controller is not trusted to execute arbitrary code directly on a worker. A job contains typed actions, not command strings. An agent independently checks repository allowlists, capabilities, protocol version, and resource ceilings.

The executor receives already-authorized typed jobs and maps supported actions to fixed git/cargo executable and argument templates. It does not invoke a shell.

The GitHub adapter receives validated repository/ref/status data and maps it to fixed gh argument templates. GitHub authentication remains outside job payloads.

## Phase 1 local flow

Operator -> CLI -> Agent policy validation -> LocalExecutor -> per-job workspace -> Git checkout -> fixed Cargo actions -> logs/report -> cleanup.

## Phase 2 GitHub flow

Operator -> run-github -> GitHub ref resolution -> exact commit SHA -> pending status -> Agent policy validation -> LocalExecutor -> exact SHA checkout -> Cargo validation -> result status.

GitHub Actions is optional. A self-hosted Actions runner may later invoke Test Lab, but Test Lab does not depend on Actions as its execution engine.

## Phase 3 sandbox flow

    authorized typed job
      -> select sandbox mode
      -> optional worker-identity check
      -> fixed Git/Cargo command
      -> Windows: suspended spawn -> Job Object assignment -> resume
      -> or Docker/Podman: fixed Cargo container wrapper
      -> memory/process ceilings
      -> bounded output/artifacts
      -> whole-tree teardown on timeout/cancel
      -> cleanup

Windows native mode is the default on the current worker platform. Non-Windows native mode fails closed until a native containment implementation exists; Docker/Podman remain available as the portable project-code sandbox path.

## Phase 4 VM flow

    validated VM request
      -> require DragonForge-* managed name
      -> validate switch/resources/base VHDX
      -> create differencing child disk
      -> create Generation 2 Hyper-V VM
      -> configure CPU/memory/firmware/checkpoint type
      -> optional per-instance setup
      -> create DragonForge-Baseline
      -> destructive/integration test
      -> restore baseline
      -> repeat or destroy managed VM

Golden images live outside the Git repository. Managed VM storage is separate from the immutable parent image root. The VM adapter never accepts arbitrary PowerShell text.

## Phase 5 deep Rust flow

    repository
      -> fmt / Clippy / baseline tests
      -> cargo-nextest workspace execution
      -> proptest invariants
      -> cargo-llvm-cov summary
      -> Criterion benchmark compile
      -> optional Miri protocol checks
      -> optional Linux AddressSanitizer
      -> optional bounded cargo-fuzz target
      -> transcript / regression artifacts

Phase 5 keeps the remote job protocol typed. The deep-testing scripts are repository-maintained fixed workflows rather than caller-supplied command strings. Nightly and fuzz lanes are explicit opt-ins and are intended for disposable Linux workers/VMs when testing untrusted repositories.

## Phase 6 Windows integration flow

    Windows worker / VM
      -> read-only Windows doctor
      -> safe HKCU registry fixture
      -> fixed direct process fixture
      -> TCP/UDP loopback fixtures
      -> Windows Installer discovery
      -> optional MSI signature inspection
      -> optional elevated service create/query/delete
      -> optional elevated Application Event Log write
      -> cleanup / transcript

Safe and privileged fixtures are deliberately separated. Privileged mutation requires an elevated token and explicit CLI confirmation. Fixture names and registry locations are DragonForge-managed and generated internally; callers cannot supply service binary paths, registry scripts, Event Log commands, or arbitrary PowerShell.

## Phase 7 GUI automation flow

    interactive Windows desktop
      -> validate DragonForge-* managed window target
      -> replay typed JSON action plan
      -> UI Automation exact window/control lookup
      -> ValuePattern / InvokePattern operation
      -> bounded assertion
      -> target-window screenshot
      -> optional owned fixture termination
      -> exit-code/crash artifact
      -> cleanup

GUI plans cannot supply executable commands, raw PowerShell, arbitrary UI Automation properties, raw keyboard injection, or unrestricted desktop coordinates. Phase 7 requires an interactive desktop session and is intended to run on the Windows host or Windows VM console/RDP session.

## Phase 8 distributed flow

    authorized node
      -> outbound connection to private/controller address
      -> signed registration envelope
      -> nonce/timestamp/MAC verification
      -> lease + capability inventory
      -> scheduler selects compatible low-load node
      -> signed typed job command
      -> worker executes supported fixture only
      -> signed result manifest + SHA-256 artifact metadata
      -> controller verifies node identity/result
      -> lease/active-job state updated

Multi-node plans allocate named roles to distinct nodes based on OS, architecture, labels, features, current load, lease state, and free job slots.

The Phase 8 cross-node validation sends only the fixed `NetworkFixtureSuite` task. No shell string, executable path, arbitrary program arguments, firewall command, or remote desktop operation exists in the wire contract.

Phase 8 message authentication provides integrity and replay protection. Cross-host use is restricted to private/link-local addresses; deployments crossing untrusted networks should additionally use a trusted VPN or future mTLS layer for confidentiality.

## Phase 9 MCP flow

    MCP-capable client
      -> loopback HTTP /mcp
      -> bearer authentication
      -> JSON-RPC + MCP header validation
      -> fixed tool dispatch
      -> named typed test profile
      -> Agent policy validation
      -> LocalExecutor
      -> bounded in-memory job state
      -> status/result/artifact metadata tools

The MCP layer does not bypass the worker trust boundary. Repository allowlisting and typed TestAction authorization are applied before LocalExecutor execution.

Phase 9 returns result summaries, step status metadata, and SHA-256 artifact metadata. It does not return arbitrary filesystem content or raw stdout/stderr through MCP.

The gateway supports modern stateless MCP discovery as well as the legacy initialize path, but remains bound to loopback. Internet-facing OAuth/TLS exposure is explicitly outside Phase 9.

## Distributed target

The Phase 8 topology is:

    ChatGPT / operator / GitHub
              |
              v
        Test Lab Controller
              |
       authenticated transport
       +------+------+------+
       |             |      |
    Windows        Linux   Raspberry Pi / ARM
    worker         worker  worker
       |             |      |
    sandbox/VM   container  hardware/GPIO

Workers can be physical machines, VMs, container hosts, Raspberry Pi systems, or other authorized nodes. Scheduling should use capabilities, architecture, OS, availability, and eventually load rather than hard-coded machine names.

Where practical, remote agents should establish outbound authenticated connections to the controller. Future transports must preserve worker-side authorization and the typed-execution boundary.


## Phase 10 intelligence flow

    changed repository paths
      + bounded historical failures
      + worker capacity snapshots
             |
             v
    deterministic profile scoring
             |
             +--> historical regression score boosts
             +--> normalized SHA-256 failure clusters
             +--> explicit regression targets
             v
    resource-aware scheduler
             |
             +--> scheduled profile -> eligible worker
             +--> unscheduled profile -> explicit report

The intelligence crate does not launch processes or mutate workers. It produces recommendations consumed by existing typed execution boundaries.


## Phase 11 durable controller flow

    typed job / worker registration / intelligence report
              |
              v
        DurableController
              |
        SQLite schema v1
       +------+------+------+------+
       |      |      |      |      |
      jobs  attempts workers audit config
       |                    |
       +--> artifact metadata
       +--> intelligence history

On startup, queued and terminal jobs retain their state. Jobs persisted as assigned or running are changed to `interrupted` and their latest attempt is closed with a recovery audit event. Phase 11 deliberately does not automatically retry an interrupted job; retry classification and rescheduling are reserved for Phase 15.

The database is controller-owned state only. A persisted assignment still has to pass the existing worker-side Agent/Policy trust boundary before any process execution occurs.


## Phase 12 mTLS / identity flow

    controller CA trust roots
              |
              v
       rustls mTLS server
       requires client cert
              ^
              |
       outbound worker TLS
       client certificate
              |
              v
      certificate chain validation
              +
      SHA-256 fingerprint binding
              +
      enrolled node identity
              +
      generation / validity / revocation

The TLS layer provides encryption plus certificate authentication. DragonForge authorization additionally maps the presented end-entity certificate fingerprint to the claimed node ID. A CA-issued certificate therefore does not automatically authorize every node identity.

Certificate trust metadata is serializable without private keys. Rotation uses overlapping certificate generations so a new key can become active before the old generation is retired. Revocation fails closed. The Phase 8 HMAC transport remains available for compatibility/private-lab debugging; Phase 13 service-oriented workers should prefer mTLS.


## Phase 13 worker-service flow

    OS service manager
       |          |
    Windows      systemd
      SCM          |
       \          /
        worker-service runtime
              |
       recover local snapshot
              |
       outbound mTLS connect
              |
       typed registration
              |
       typed heartbeats
              |
       online / draining
              |
       disconnect -> bounded backoff -> reconnect

Windows uses a native Service Control Manager dispatcher and Stop control handling. Linux uses the same foreground worker runtime beneath a hardened systemd unit. Service installation/upgrade packaging is reserved for Phase 21; the Phase 13 executable and service definitions are service-manager capable now.

Drain mode prevents new work while retaining existing active-job state. Runtime snapshots contain no certificate or private-key bytes. A restart never trusts a previously online session and reconnects before advertising availability.


## Phase 14 observability flow

    controller / worker lifecycle
              |
              +--> structured events -> redact + bound -> SQLite / JSONL
              |
              +--> metric samples -> validate finite values -> SQLite
              |
              +--> audit events -> SQLite audit_id + previous digest -> SHA-256 chain
              |
              +--> artifacts -> canonical root check -> SHA-256 catalog
                                      |
                                      +--> age/count/byte retention policy
                                      +--> root-contained regular-file deletion

Controller schema v2 extends the durable Phase 11 database rather than creating a separate telemetry database. Existing v1 audit records remain readable; new Phase 14 audit records form a verifiable SHA-256 chain. Telemetry pruning can remove structured logs and metrics by timestamp but does not remove audit history.

WorkerServiceRuntime exposes deterministic operational metrics for active jobs, job admission, drain state, and reconnect attempts. These metrics are intended to become input to the Phase 18 dashboard and later scheduling/intelligence integrations, but Phase 14 metrics do not themselves authorize or execute work.

Artifact retention is filesystem-root constrained. Cataloging canonicalizes files beneath the configured artifact root, rejects symlinks and escapes, hashes content with SHA-256, and retention removes only eligible canonical regular files beneath that same root.


## Phase 15 recovery / retry / lifecycle flow

    typed durable job
          |
          +--> persisted RetryPolicy
          |
       assigned -> running
          |
          +--> passed/rejected/cancelled -> terminal
          |
          +--> test failure -> failed (never automatic retry)
          |
          +--> permanent infrastructure failure -> failed
          |
          +--> transient infrastructure failure
                    |
                    +--> attempts remain -> retry_pending + due timestamp
                    |                         |
                    |                         +--> due -> normal capability scheduler
                    |                                      -> new durable attempt
                    +--> limit reached -> exhausted

Controller restart treats assigned/running work as interrupted. The latest attempt is closed as interrupted. Only jobs whose persisted policy explicitly enables interrupted retry can move automatically to retry_pending; otherwise they remain interrupted until an operator explicitly reschedules them.

Manual interrupted-job rescheduling is auditable and bypasses only the job's automatic retry preference, not the global five-attempt safety ceiling. Every reassignment still returns through normal capability-aware scheduling and subsequently through the existing Agent/Policy/Executor worker trust boundary.

Schema v3 stores retry policy, failure class, retry due time, retry reason, and attempt-level failure class. Retry due times are durable, so restart cannot make an early retry eligible.


## Phase 16 test-plan flow

    versioned plan JSON
          |
          +--> bounds/version validation
          +--> dependency DAG validation
          +--> typed profile / typed TestAction mapping
          +--> resource + retry validation
          +--> OS / node-label predicates
          |
          v
    CompiledPlanStep
          |
          +--> JobRequest with extra required capabilities
          +--> Phase 15 RetryPolicy
          +--> typed artifact classes
          |
          v
    normal controller capability scheduling
          -> Agent/Policy
          -> Executor fixed actions

Controller schema v4 persists validated plans in test_plans and writes create/update events into the existing hash-chained audit stream. Plan persistence does not authorize execution.

The legacy durable worker table does not contain node-label inventory. Phase 16 therefore exposes target predicates on CompiledPlanStep and requires plan/distributed orchestration to apply them before selecting a node; it does not falsely claim the older assign_next method enforces labels.


## Phase 17 intelligence integration

Phase 17 adds `df-test-intelligence-integration` between the deterministic intelligence engine and the existing durable controller/plan boundary.

Data flow:

    GitHub compare -> ChangeSet
    controller intelligence_job_context + failed jobs -> HistoricalFailure
    online durable workers + assigned/running slots -> WorkerCapacity
    Phase 10 analyze -> IntelligenceReport
    stored Phase 16 plan -> exact executable profile match
    advisory mode -> decision only
    automatic mode -> bounded typed root-step enqueue
    decision -> intelligence_history + hash-chained audit

Automatic mode is intentionally narrower than advisory recommendations. Only exact `rust_fast` and `rust_standard` plan profiles are executable through this bridge. Dependency-bearing or OS/label-constrained steps are not auto-enqueued because the legacy durable queue does not represent a full plan-run dependency/routing state machine.

Every automatically generated JobRequest is pinned to the immutable compared head SHA before persistence.


## Phase 18 dashboard flow

Phase 18 adds `df-test-dashboard` as a local operator visibility layer over the durable controller.

    browser on controller host
              |
      loopback HTTP only
              |
      static shell/CSS/JS
              |
      bearer-authenticated GET /api/*
              |
       bounded projections
       +------+------+------+------+------+------+
       |      |      |      |      |      |
      jobs workers plans artifacts intelligence audit
                                      |
                              chain verification
              |
           settings
       (security metadata only)

The dashboard opens the existing durable controller database and uses typed controller query methods. It does not bypass the controller, Agent, Policy, Executor, lifecycle, or worker-service boundaries.

The Phase 18 HTTP API is deliberately read-only. There is no generic action endpoint, terminal, command field, raw SQL endpoint, filesystem browser, artifact-content endpoint, or arbitrary URL proxy. Future operator mutations must be introduced as separately designed typed/audited operations rather than extending the dashboard into a general control channel.
