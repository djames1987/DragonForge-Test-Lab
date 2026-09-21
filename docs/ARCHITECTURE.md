# Architecture

## Purpose

DragonForge Test Lab is a local-first test orchestration platform. The controller schedules typed jobs. Agents advertise capabilities and enforce their own local execution policy before any job is accepted.

## Phase 0 components

- `df-test-protocol`: versioned, serializable contracts shared by controllers and agents.
- `df-test-policy`: repository, capability, and resource-limit authorization.
- `df-test-agent`: worker-side trust boundary and protocol compatibility gate.
- `df-test-controller`: queue and capability-aware worker scheduling.
- `dragonforge-test-lab`: initial operator CLI and doctor command.

## Trust model

The controller is not trusted to execute arbitrary code on a worker. A job contains typed actions, not command strings. An agent independently checks repository allowlists, capabilities, protocol version, and resource ceilings.

Future transports (local IPC, mutually authenticated network transport, or MCP-facing gateways) must preserve this boundary.

## Planned topology

ChatGPT / operator / GitHub -> gateway/controller -> authenticated worker -> sandbox/container/VM -> artifacts/results.

Phase 0 intentionally does not execute external processes yet. Actual process execution begins only after the sandbox and command-construction boundary are explicitly implemented and tested.
