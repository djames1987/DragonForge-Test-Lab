# Phase 8 — Multi-machine & Network Lab

Status: implementation complete; host, VM, and optional real cross-node validation pending.

## Goal

Phase 8 turns DragonForge Test Lab into a distributed capability-based test cluster foundation. Authorized Windows, Linux, VM, container-host, Raspberry Pi/ARM, and other supported nodes can advertise typed capabilities, maintain health leases, receive only compatible work, return hashed result manifests, and participate in bounded network fixtures without exposing a general remote shell.

## Delivered

- New `df-test-distributed` crate.
- Signed HMAC-SHA256 node messages.
- Per-message nonce replay protection and bounded clock-skew validation.
- Outbound-only node registration policy.
- Private/loopback/link-local controller address policy.
- Node profiles with:
  - OS;
  - architecture;
  - labels;
  - feature capabilities;
  - parallel-job capacity.
- Lease-based liveness and heartbeat tracking.
- Load-aware, capability-aware scheduling.
- Distinct-node role allocation for multi-node plans.
- Typed distributed network task execution.
- Signed cross-node result return.
- SHA-256 artifact manifests.
- Bounded frame sizes.
- TCP loopback fixture.
- UDP loopback fixture.
- DNS fixture.
- Deterministic delay/drop fault profile.
- Real host-to-node one-shot authenticated registration and typed network-job probe.
- PowerShell readiness and validation tooling.
- Example multi-node plan.

## Node features

Phase 8 nodes advertise only explicit features:

- `rust`
- `docker`
- `podman`
- `hyper_v`
- `windows_integration`
- `gui_automation`
- `tcp_fixture`
- `udp_fixture`
- `dns_fixture`
- `fault_injection`
- `hardware_io`

Schedulers match work to features, labels, OS, architecture, lease state, current load, and available parallel-job capacity.

## Authentication and replay protection

Each registration, heartbeat, command, acknowledgement, and result is wrapped in an authenticated envelope containing:

- key ID;
- random nonce;
- issue timestamp;
- typed payload;
- HMAC-SHA256 MAC.

Keys are installed by the operator. The shared secret is never accepted as a CLI argument. The cross-node CLI reads:

    DRAGONFORGE_NODE_SHARED_SECRET

The secret must be at least 32 bytes and is not printed by Test Lab.

A verifier rejects:

- unknown keys;
- invalid MACs;
- stale timestamps;
- duplicate nonces;
- key/node identity mismatches.

## Transport boundary

Agents initiate outbound TCP connections to the controller. Agent-side inbound listeners are forbidden by the registration contract.

Controller targets accepted by the built-in client are limited to:

- loopback;
- RFC1918/private IPv4;
- link-local IPv4;
- IPv6 loopback;
- IPv6 unique-local;
- IPv6 link-local.

Public Internet addresses are rejected.

Phase 8 provides message authentication, integrity, replay protection, and typed framing. It does not claim confidentiality against an attacker who can observe a private network segment. Production use across untrusted networks should place this transport inside a trusted VPN such as WireGuard/Tailscale or a future mTLS transport before carrying sensitive metadata.

## Cross-node typed job probe

The real cross-node probe proves more than registration.

Flow:

    node -> outbound authenticated registration
    controller -> signed acceptance
    controller -> signed typed NetworkFixtureSuite job
    node -> execute bounded Phase 8 network fixtures
    node -> signed NodeResultManifest
    controller -> verify node identity, status, artifact digests

The node never receives a shell command, executable path, PowerShell fragment, or arbitrary process request.

## Capability-aware scheduling

A multi-node plan contains named roles with requirements.

Example:

    interactive-gui:
      os=windows
      arch=x86_64
      features=gui_automation,tcp_fixture
      label=interactive

    network-fault:
      os=linux
      arch=x86_64
      features=dns_fixture,fault_injection
      label=container

The scheduler chooses distinct online nodes and prefers lower load and fewer active jobs. Nodes beyond lease expiry are not eligible.

See:

    examples/phase8-multi-node-plan.json

## Result and artifact return

Remote results use a typed `NodeResultManifest` containing:

- job ID;
- authenticated node ID;
- job status;
- bounded summary;
- artifact metadata;
- SHA-256 artifact digest.

Artifact file contents are not interpreted as commands.

## Network fixtures

Phase 8 includes:

- TCP loopback request/response;
- UDP loopback request/response;
- localhost DNS resolution;
- bounded deterministic delay;
- deterministic drop-every-N simulation.

Fault injection is implemented inside Test Lab fixture behavior. Phase 8 does not alter the Windows firewall, Linux firewall, routing table, NIC settings, packet filters, or system DNS configuration.

## Local validation

Run on the Windows host:

    .\scripts\test-phase8.ps1

Run independently inside the Windows VM:

    .\scripts\test-phase8.ps1

This validates:

1. cargo fmt;
2. cargo clippy;
3. cargo test;
4. network/node readiness;
5. distributed doctor;
6. authenticated registration/replay/heartbeat scheduling fixtures;
7. outbound framed transport;
8. TCP/UDP/DNS/fault fixtures;
9. hashed result artifacts;
10. GitHub-aware worker regression.

## Real host-to-VM validation

Choose a private host IP and unused TCP port, for example:

    192.168.1.50:45880

Set the same strong temporary secret in both PowerShell sessions:

    $env:DRAGONFORGE_NODE_SHARED_SECRET = "replace-with-at-least-32-random-characters"

On the host/controller:

    .\scripts\test-phase8.ps1 -CrossNodeRole controller -ControllerAddress 192.168.1.50:45880

While the host is waiting, run inside the VM:

    $env:DRAGONFORGE_NODE_SHARED_SECRET = "replace-with-the-same-secret"
    .\scripts\test-phase8.ps1 -CrossNodeRole node -ControllerAddress 192.168.1.50:45880 -NodeId DragonForge-Windows-VM

The node initiates the connection. The controller does not remotely open an agent shell.

After testing:

    Remove-Item Env:\DRAGONFORGE_NODE_SHARED_SECRET

## Security boundaries

Phase 8 intentionally does not provide:

- arbitrary command execution;
- arbitrary shell or PowerShell payloads;
- public-Internet controller targets;
- inbound agent listeners;
- unrestricted port scanning;
- packet capture;
- firewall modification;
- route manipulation;
- credential transport in job payloads;
- remote desktop control.

The distributed layer is a typed test transport, not a remote administration framework.
