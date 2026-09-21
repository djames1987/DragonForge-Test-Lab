# Architecture

## Purpose

DragonForge Test Lab is a local-first test orchestration platform designed to grow into a distributed capability-based test cluster. The controller schedules typed jobs. Agents advertise capabilities and enforce their own local execution policy before a job is accepted.

## Components

- df-test-protocol: versioned, serializable contracts shared by controllers and agents.
- df-test-policy: repository, capability, and resource-limit authorization.
- df-test-agent: worker-side trust boundary and protocol compatibility gate.
- df-test-controller: queue and capability-aware worker scheduling.
- df-test-executor: local workspace, fixed-command process execution, cancellation, output capture, artifact generation, and cleanup.
- df-test-github: typed GitHub repository/ref resolution and commit-status reporting through the authenticated gh CLI.
- dragonforge-test-lab: operator CLI, doctor checks, local execution, and GitHub-aware execution entry point.

## Trust model

The controller is not trusted to execute arbitrary code directly on a worker. A job contains typed actions, not command strings. An agent independently checks repository allowlists, capabilities, protocol version, and resource ceilings.

The executor receives already-authorized typed jobs and maps supported actions to fixed git/cargo executable and argument templates. It does not invoke a shell.

The GitHub adapter receives validated repository/ref/status data and maps it to fixed gh argument templates. GitHub authentication remains outside job payloads.

## Phase 1 local flow

Operator -> CLI -> Agent policy validation -> LocalExecutor -> per-job workspace -> Git checkout -> fixed Cargo actions -> logs/report -> cleanup.

## Phase 2 GitHub flow

Operator -> run-github -> GitHub ref resolution -> exact commit SHA -> pending status -> Agent policy validation -> LocalExecutor -> exact SHA checkout -> Cargo validation -> result status.

GitHub Actions is optional. A self-hosted Actions runner may later invoke Test Lab, but Test Lab does not depend on Actions as its execution engine.

## Distributed target

The planned Phase 8 topology is:

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
