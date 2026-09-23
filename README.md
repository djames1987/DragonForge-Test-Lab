# DragonForge Test Lab

DragonForge Test Lab is a local-first, security-conscious test orchestration platform for deeper validation than conventional hosted CI can conveniently provide.

The long-term target is a distributed DragonForge engineering lab spanning Windows/Linux workers, Raspberry Pi and other physical nodes, containers, Hyper-V virtual machines, network fixtures, GUI automation, Rust fuzzing/sanitizers, hardware-in-the-loop testing, GitHub integration, and an authenticated ChatGPT/MCP gateway.

## Current status

Phase 21 — Installer / Upgrades — Implementation Complete

Phases 0-3 established the versioned protocol, controller/agent policy boundary, local Rust worker, GitHub integration, Windows Job Object containment, worker identity checks, and Docker/Podman isolation.

Phase 4 added typed Hyper-V VM orchestration and passed full Windows lifecycle validation on 2026-09-22. Phase 5 added deep Rust testing, Phase 6 added validated Windows OS integration fixtures, Phase 7 added validated GUI automation, Phase 8 added validated authenticated distributed execution, Phase 9 added the validated authenticated MCP gateway, Phase 10 added validated deterministic Test Intelligence, Phase 11 added validated SQLite-backed durable controller state, Phase 12 added validated rustls mTLS/X.509 node identity, Phase 13 added validated long-running worker services, Phase 14 added validated audit/artifact/observability infrastructure, and Phase 15 added validated recovery/retry lifecycle control. Phase 16 added validated versioned declarative plans, controller schema v4 plan persistence, DAG dependencies/conditions, typed profile compilation, capability enforcement, typed artifacts, retry policy integration, target predicates, and plan operator tooling. Phase 17 connects Test Intelligence to real GitHub changes, durable historical failures, online worker capacity, stored plans, advisory/automatic modes, immutable head-SHA jobs, and hash-chained decision audits. Phase 18 adds an authenticated loopback-only, read-only web dashboard over bounded durable controller projections for jobs, workers, plans, artifact metadata, intelligence/failure clusters, audit history, chain verification, and safe settings. Phase 19 adds first-class Linux native process containment with process groups and rlimits, Linux worker/service/mTLS qualification, Docker/Podman validation, advanced Rust lanes, and native/container GitHub-aware qualification tooling. Full Ubuntu Server x86_64 platform qualification passed on 2026-09-23. Phase 20 adds typed ARM/Raspberry Pi hardware inventory, bounded read-only HIL probes, distributed ARM/device capabilities, and a physical ARM qualification path.

## Workspace

    apps/
      dragonforge-test-lab/   operator CLI
    crates/
      df-test-arm/            ARM/Raspberry Pi inventory and typed read-only HIL probes
      df-test-install/        installer layouts, manifests, upgrades, and rollback metadata
      df-test-protocol/       shared versioned contracts
      df-test-policy/         worker authorization policy
      df-test-agent/          worker-side trust boundary
      df-test-controller/     scheduling/controller core
      df-test-dashboard/      authenticated local read-only operator dashboard
      df-test-executor/       checkout/process/artifact execution
      df-test-github/         typed GitHub adapter
      df-test-sandbox/        native/container containment
      df-test-vm/             Hyper-V VM Lab orchestration
      df-test-windows/        Windows integration fixtures
      df-test-gui/            Windows GUI automation
      df-test-distributed/    distributed nodes, scheduling, transport, network fixtures
      df-test-mcp/            authenticated MCP HTTP gateway and typed tool surface
      df-test-intelligence/   change analysis, regression targeting, clustering, scheduling
      df-test-intelligence-integration/ real Git/controller/worker/plan intelligence orchestration
      df-test-identity/       mTLS, X.509 node identity, trust, rotation, revocation
      df-test-worker-service/ long-running worker lifecycle and service hosting
      df-test-observability/  audit/log/metrics/artifact observability and retention
      df-test-lifecycle/      failure classification and bounded retry policy
      df-test-plans/          versioned declarative plan validation and compilation
    docs/
      ARCHITECTURE.md
      SECURITY.md
      HOST-SETUP-HYPERV.md
      PHASE-0.md
      PHASE-1.md
      PHASE-2.md
      PHASE-3.md
      PHASE-4.md
      PHASE-5.md
      PHASE-6.md
      PHASE-7.md
      PHASE-8.md
      PHASE-9.md
      PHASE-10.md
      PHASE-11.md
      PHASE-12.md
      PHASE-13.md
      PHASE-14.md
      PHASE-15.md
      PHASE-16.md
      PHASE-17.md
      PHASE-18.md
      PHASE-19.md
      PHASE-20.md
      PHASE-21.md
      ROADMAP.md
    scripts/
      check-hyperv-host.ps1
      test-phase1.ps1
      test-phase2.ps1
      test-phase3.ps1
      test-phase4.ps1
      test-phase5.ps1
      check-rust-deep-tools.ps1
      check-windows-integration.ps1
      test-phase6.ps1
      check-gui-host.ps1
      phase7-gui-fixture.ps1
      test-phase7.ps1
      check-distributed-host.ps1
      test-phase8.ps1
      check-mcp-host.ps1
      test-phase9.ps1
      test-phase10.ps1
      test-phase11.ps1
      test-phase12.ps1
      test-phase13.ps1
      test-phase14.ps1
      test-phase15.ps1
      test-phase16.ps1
      test-phase17.ps1
      test-phase18.ps1
      build-sandbox-image.sh
      test-phase19-linux.sh
      test-phase20-arm.sh
      test-phase21.ps1
      test-phase21-linux.sh
      package-release-windows.ps1
      package-release-linux.sh
      install-windows.ps1
      install-linux.sh
      rollback-windows.ps1
      rollback-linux.sh
      uninstall-windows.ps1
      uninstall-linux.sh

## Hyper-V host setup

Follow docs/HOST-SETUP-HYPERV.md before running VM lifecycle tests.

Read-only host readiness:

    .\scripts\check-hyperv-host.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -SwitchName "Default Switch"

VM Lab doctor:

    cargo run -p dragonforge-test-lab -- vm-doctor --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch"

## VM lifecycle

Create:

    cargo run -p dragonforge-test-lab -- vm-create --name DragonForge-Windows-Test-01 --guest-os windows --base-vhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images

Create clean baseline:

    cargo run -p dragonforge-test-lab -- vm-baseline --name DragonForge-Windows-Test-01

Restore baseline:

    cargo run -p dragonforge-test-lab -- vm-restore --name DragonForge-Windows-Test-01

Destroy:

    cargo run -p dragonforge-test-lab -- vm-destroy --name DragonForge-Windows-Test-01 --vm-root C:\DragonForge-Test-Lab-VMs --confirm

## Phase 4 validation

Core host/code validation:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs

Full VM lifecycle validation after a golden image is prepared:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -BaseVhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx -GuestOs windows

## Phase 5 deep Rust testing

Readiness:

    .\scripts\check-rust-deep-tools.ps1

Mandatory validation:

    .\scripts\test-phase5.ps1

Install/update required cargo tools and validate:

    .\scripts\test-phase5.ps1 -InstallTools

See docs/PHASE-5.md for Miri, sanitizer, fuzzing, benchmark, and coverage lanes.

## Phase 6 Windows integration

Read-only readiness:

    .\scripts\check-windows-integration.ps1

Safe host validation:

    .\scripts\test-phase6.ps1

Elevated service/Event Log validation:

    .\scripts\test-phase6.ps1 -IncludePrivileged

See docs/PHASE-6.md for fixture boundaries, installer inspection, and permission behavior.

## Phase 7 GUI automation

Readiness:

    .\scripts\check-gui-host.ps1

End-to-end validation:

    .\scripts\test-phase7.ps1

Run a typed plan:

    cargo run -p dragonforge-test-lab -- gui-run-plan --plan .\examples\phase7-plan.json

See docs/PHASE-7.md for the managed-window boundary, screenshots, deterministic plans, and crash capture.

## Phase 8 distributed/network lab

Readiness:

    .\scripts\check-distributed-host.ps1

Local Phase 8 validation:

    .\scripts\test-phase8.ps1

Distributed doctor:

    cargo run -p dragonforge-test-lab -- distributed-doctor

Authenticated scheduler/network fixtures:

    cargo run -p dragonforge-test-lab -- distributed-fixtures

For the real host-to-VM typed job probe and shared-secret setup, see docs/PHASE-8.md.

## Phase 9 ChatGPT/MCP gateway

Required environment:

    $env:DRAGONFORGE_MCP_TOKEN = "<at-least-32-random-characters>"
    $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = "https://github.com/djames1987/DragonForge-Test-Lab"

Readiness:

    .\scripts\check-mcp-host.ps1
    cargo run -p dragonforge-test-lab -- mcp-doctor

Start the loopback MCP endpoint:

    cargo run -p dragonforge-test-lab -- mcp-serve

End-to-end Phase 9 validation:

    .\scripts\test-phase9.ps1

See docs/PHASE-9.md for authentication, MCP protocol compatibility, typed tool schemas, job profiles, and artifact/result boundaries.

## Phase 10 Test Intelligence

Readiness:

    cargo run -p dragonforge-test-lab -- intelligence-doctor

Deterministic built-in fixture:

    cargo run -p dragonforge-test-lab -- intelligence-fixture

Analyze a JSON scenario:

    cargo run -p dragonforge-test-lab -- intelligence-analyze --input .\examples\phase10-intelligence.json

End-to-end Phase 10 validation:

    .\scripts\test-phase10.ps1

See docs/PHASE-10.md for profile scoring, history targeting, clustering, scheduling rules, and security boundaries.

## Phase 11 Durable Controller & State

Readiness and schema migration:

    cargo run -p dragonforge-test-lab -- controller-state-doctor

Restart-recovery fixture:

    cargo run -p dragonforge-test-lab -- controller-state-fixture

End-to-end Phase 11 validation:

    .\scripts\test-phase11.ps1

The default durable database is:

    .dragonforge-test-lab\controller.sqlite3

See docs/PHASE-11.md for schema, persistence records, restart behavior, audit metadata, and security boundaries.

## Phase 12 mTLS / Node Identity

Readiness:

    cargo run -p dragonforge-test-lab -- identity-doctor

Real loopback mutual-TLS + identity lifecycle fixture:

    cargo run -p dragonforge-test-lab -- identity-fixture

End-to-end Phase 12 validation:

    .\scripts\test-phase12.ps1

Phase 12 uses rustls for certificate-authenticated encrypted transport and binds node IDs to enrolled SHA-256 certificate fingerprints. Renewal supports a bounded overlap for key rotation; revocation fails closed. Trust metadata deliberately excludes private keys.

See docs/PHASE-12.md for certificate lifecycle, transport, compatibility, and security boundaries.

## Phase 13 Worker Services

Readiness and real service fixture:

    cargo run -p dragonforge-test-lab -- worker-service-doctor
    cargo run -p dragonforge-test-lab -- worker-service-fixture

Run a foreground worker:

    cargo run -p dragonforge-test-lab -- worker-service-run --config .\examples\phase13-worker.json

Request drain/resume:

    cargo run -p dragonforge-test-lab -- worker-service-drain --config .\examples\phase13-worker.json
    cargo run -p dragonforge-test-lab -- worker-service-resume --config .\examples\phase13-worker.json

Native Windows SCM entry point:

    dragonforge-test-lab worker-service-windows --config <worker.json>

Generate Windows/systemd service definitions:

    cargo run -p dragonforge-test-lab -- worker-service-specs --executable <absolute-path> --config <worker.json>

End-to-end Phase 13 validation:

    .\scripts\test-phase13.ps1

See docs/PHASE-13.md for service lifecycle, mTLS registration, drain behavior, restart recovery, Windows SCM hosting, systemd hardening, and security boundaries.

## Phase 14 Audit / Artifacts / Observability

Readiness:

    cargo run -p dragonforge-test-lab -- observability-doctor

End-to-end audit/log/metric/artifact fixture:

    cargo run -p dragonforge-test-lab -- observability-fixture

Inspect the durable controller:

    cargo run -p dragonforge-test-lab -- observability-summary

End-to-end Phase 14 validation:

    .\scripts\test-phase14.ps1

Phase 14 upgrades the durable controller database to schema v2. New audit records are SHA-256 chained, structured logs are bounded/redacted before persistence, metrics are durable, worker services expose runtime metrics, artifacts can be SHA-256 cataloged and safely pruned under a configured root, and old telemetry can be removed without deleting audit history.

See docs/PHASE-14.md for persistence, audit-chain, logging, metrics, retention, and security boundaries.

## Phase 15 Recovery / Retry / Job Lifecycle

Readiness:

    cargo run -p dragonforge-test-lab -- lifecycle-doctor

End-to-end retry/recovery fixture:

    cargo run -p dragonforge-test-lab -- lifecycle-fixture

Inspect a durable job and its attempts:

    cargo run -p dragonforge-test-lab -- lifecycle-status --job-id <uuid>

Explicitly reschedule an interrupted job:

    cargo run -p dragonforge-test-lab -- lifecycle-reschedule --job-id <uuid>

End-to-end Phase 15 validation:

    .\scripts\test-phase15.ps1

Phase 15 upgrades the durable controller to schema v3. Test failures remain terminal, transient infrastructure failures may retry only under an explicit bounded policy, retry-pending work cannot run before its durable due time, interrupted restart recovery requires explicit policy opt-in for automatic retry, and manual interrupted-job rescheduling is auditable and globally bounded.

See docs/PHASE-15.md for failure classes, retry policies, lifecycle states, recovery behavior, query commands, and security boundaries.

## Phase 16 Test Plans

Readiness:

    cargo run -p dragonforge-test-lab -- plan-doctor

Validate the checked-in example:

    cargo run -p dragonforge-test-lab -- plan-validate --plan .\examples\phase16-plan.json

Compile a plan step into a typed job:

    cargo run -p dragonforge-test-lab -- plan-compile --plan .\examples\phase16-plan.json --step standard

Persist a validated plan:

    cargo run -p dragonforge-test-lab -- plan-store --plan .\examples\phase16-plan.json

List stored plans:

    cargo run -p dragonforge-test-lab -- plan-list

End-to-end Phase 16 validation:

    .\scripts\test-phase16.ps1

Phase 16 upgrades the durable controller to schema v4 and adds versioned declarative plans for typed profiles/actions, dependency DAGs, conditions, resource limits, capabilities, typed artifacts, Phase 15 retries, target OS and node labels. Plans remain configuration: compilation yields ordinary typed JobRequest values and cannot inject arbitrary commands.

See docs/PHASE-16.md for the format, profile mappings, dependency semantics, bounds, persistence, and security boundary.

## Phase 17 Intelligence Integration

Readiness:

    cargo run -p dragonforge-test-lab -- intelligence-integration-doctor

Run a real GitHub comparison against a stored plan in advisory mode:

    cargo run -p dragonforge-test-lab -- intelligence-integrate --repo https://github.com/djames1987/DragonForge-Test-Lab.git --base main --head <revision> --plan <stored-plan-name> --mode advisory

Automatic mode is opt-in and only queues high-confidence, dependency-free, unconstrained `rust_fast` / `rust_standard` plan steps that have live eligible worker capacity. Enqueued jobs are pinned to the resolved head SHA and remain subject to existing controller, Agent, Policy, Executor, retry, and capability enforcement.

End-to-end Phase 17 validation:

    .\scripts\test-phase17.ps1

See docs/PHASE-17.md for durable historical-failure context, worker-capacity behavior, advisory/automatic modes, audit records, and the deliberate automation limits.


## Phase 18 Dashboard

Required environment:

    $env:DRAGONFORGE_DASHBOARD_TOKEN = "<at-least-32-random-characters>"

Readiness:

    cargo run -p dragonforge-test-lab -- dashboard-doctor

Deterministic security/data fixture:

    cargo run -p dragonforge-test-lab -- dashboard-fixture

Start the local dashboard:

    cargo run -p dragonforge-test-lab -- dashboard-serve

Default browser URL:

    http://127.0.0.1:8788/#token=<DRAGONFORGE_DASHBOARD_TOKEN>

The Phase 18 dashboard is loopback-only and read-only. Authenticated views expose bounded controller data for jobs, workers, plans, artifact metadata, persisted intelligence/failure clusters, audit history/chain verification, and dashboard settings. It does not provide terminal access, raw command execution, raw SQL, filesystem browsing, artifact-content browsing, or state-changing HTTP actions.

End-to-end Phase 18 validation:

    .\scripts\test-phase18.ps1 -Revision main

See docs/PHASE-18.md for routes, authentication, browser security headers, controller projections, validation, and deliberate limits.


## Phase 19 Linux Qualification

Linux readiness:

    cargo run -p dragonforge-test-lab -- linux-doctor

Linux containment/service/mTLS fixture:

    cargo run -p dragonforge-test-lab -- linux-fixture

Full Linux qualification:

    bash ./scripts/test-phase19-linux.sh --revision main

Install/update the mandatory advanced Rust tools first:

    bash ./scripts/test-phase19-linux.sh --revision main --install-tools

Optional Linux nightly Miri/ASan/fuzz qualification:

    bash ./scripts/test-phase19-linux.sh --revision main --install-tools --include-nightly --fuzz-seconds 30

Phase 19 adds native Linux process-group containment with address-space/process rlimits and whole-tree cancellation. The qualification script also validates Docker/Podman execution, worker-service mTLS/heartbeat/drain/restart behavior, lifecycle recovery, observability, nextest, llvm-cov, property tests, benchmark compilation, and GitHub-aware native/container workers.

See docs/PHASE-19.md for the Linux trust boundary, required tools, validation matrix, and platform qualification procedure.


## Phase 20 ARM / Raspberry Pi

ARM readiness on a native Linux ARM worker:

    cargo run -p dragonforge-test-lab -- arm-doctor

Deterministic typed hardware fixture:

    cargo run -p dragonforge-test-lab -- arm-fixture

Bounded read-only hardware probe:

    cargo run -p dragonforge-test-lab -- arm-probe --probe board-model

Physical ARM qualification:

    bash ./scripts/test-phase20-arm.sh --revision main --install-tools

Optional Docker/Podman qualification on ARM:

    bash ./scripts/test-phase20-arm.sh --revision main --container-runtime docker

Phase 20 exposes only allowlisted read-only board/thermal/GPIO-controller/I²C/SPI/UART discovery. It does not expose arbitrary filesystem paths, generic shell commands, or raw hardware writes. Phase 20 remains **Needs Testing** until the Phase 20 script is run on a real Raspberry Pi/physical ARM host.

See docs/PHASE-20.md for the capability model, HIL boundary, and qualification procedure.

## Phase 21 Installer / Upgrades

Installer readiness:

    cargo run -p dragonforge-test-lab -- install-doctor

Deterministic install/upgrade/rollback fixture:

    cargo run -p dragonforge-test-lab -- install-fixture

Windows validation:

    .\scripts\test-phase21.ps1

Linux validation:

    bash ./scripts/test-phase21-linux.sh

Phase 21 adds fixed Windows/Linux install layouts, SHA-256 verified release manifests, stable upgrade ordering, immediate rollback metadata, installer configuration migration, platform packaging/install/rollback/uninstall scripts, and state-preserving uninstall by default. Windows host validation and the full disposable Windows VM install/upgrade/rollback/uninstall lifecycle passed on 2026-09-23. Linux host validation remains pending.

See docs/PHASE-21.md for package format, upgrade guarantees, rollback limits, filesystem layout, and validation.

## Security principle

DragonForge Test Lab is not a remote shell.

VM management is constrained to typed Hyper-V operations, managed DragonForge-* names, validated paths/resources, and explicit destructive confirmation. Phase 4 creates a disposable VM boundary but does not claim protection from hypervisor escape or provide arbitrary host-to-guest execution.

See docs/SECURITY.md, docs/HOST-SETUP-HYPERV.md, docs/PHASE-4.md, docs/PHASE-5.md, docs/PHASE-6.md, docs/PHASE-7.md, docs/PHASE-8.md, docs/PHASE-9.md, docs/PHASE-10.md, docs/PHASE-11.md, docs/PHASE-12.md, docs/PHASE-13.md, docs/PHASE-14.md, docs/PHASE-15.md, docs/PHASE-16.md, docs/PHASE-17.md, docs/PHASE-18.md, docs/PHASE-19.md, docs/PHASE-20.md, docs/PHASE-21.md, and docs/ROADMAP.md.
