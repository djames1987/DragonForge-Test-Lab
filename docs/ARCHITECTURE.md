# Architecture

## Purpose

DragonForge Test Lab is a local-first test orchestration platform. The controller schedules typed jobs. Agents advertise capabilities and enforce their own local execution policy before a job is accepted.

## Components

- df-test-protocol: versioned, serializable contracts shared by controllers and agents.
- df-test-policy: repository, capability, and resource-limit authorization.
- df-test-agent: worker-side trust boundary and protocol compatibility gate.
- df-test-controller: queue and capability-aware worker scheduling.
- df-test-executor: local workspace, fixed-command process execution, cancellation, output capture, artifact generation, and cleanup.
- dragonforge-test-lab: operator CLI, doctor checks, and local execution entry point.

## Trust model

The controller is not trusted to execute arbitrary code directly on a worker. A job contains typed actions, not command strings. An agent independently checks repository allowlists, capabilities, protocol version, and resource ceilings.

The executor receives already-authorized typed jobs and maps supported actions to fixed git/cargo executable and argument templates. It does not invoke a shell.

## Phase 1 local flow

Operator -> CLI -> Agent policy validation -> LocalExecutor -> per-job workspace -> Git checkout -> fixed Cargo actions -> logs/report -> cleanup.

## Planned topology

ChatGPT / operator / GitHub -> authenticated gateway/controller -> authenticated worker -> sandbox/container/VM -> artifacts/results.

Future transports, including MCP-facing gateways, must preserve the worker-side authorization and typed-execution boundary.
