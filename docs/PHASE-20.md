# Phase 20 — ARM / Raspberry Pi

Phase 20 adds first-class ARM and Raspberry Pi worker awareness plus bounded, read-only hardware-in-the-loop inspection. It deliberately does not turn physical nodes into remote shells or expose arbitrary device writes.

## Delivered

- dedicated `df-test-arm` crate;
- ARM32/AArch64 architecture inventory;
- Raspberry Pi board-model detection from bounded device-tree metadata;
- typed GPIO, I²C, SPI, UART, and thermal capabilities;
- bounded read-only hardware probes;
- automatic ARM/Raspberry Pi feature advertisement for distributed nodes;
- `arm-doctor`, `arm-fixture`, and `arm-probe` CLI commands;
- physical ARM qualification script;
- workspace version 0.21.0.

## Typed hardware capabilities

Phase 20 models hardware as explicit capabilities rather than paths or commands:

- `arm32`
- `arm64`
- `raspberry_pi`
- `gpio`
- `i2c`
- `spi`
- `uart`
- `thermal_sensor`

Distributed node registration maps these to typed node features such as `ArmWorker`, `RaspberryPi`, `Gpio`, `I2c`, `Spi`, and `Uart`.

## Safe HIL boundary

The operator-facing hardware operation is `arm-probe`. The probe name is allowlisted and compiles to one fixed read-only inspection operation:

    board-model
    cpu-temperature
    gpio-controllers
    i2c-buses
    spi-devices
    serial-devices

There is no caller-provided filesystem path, executable, shell text, bus address, GPIO value, UART payload, SPI transaction, or I²C write. Device enumeration is bounded to 64 entries and metadata reads are bounded to 4096 bytes.

Phase 20 therefore establishes safe hardware-in-the-loop discovery and readiness checks without yet introducing mutating hardware operations. Future mutating HIL support must remain separately typed, privilege-aware, allowlisted, and explicitly confirmed.

## Doctor and fixture

On an ARM Linux host:

    cargo run -p dragonforge-test-lab -- arm-doctor

Cross-platform deterministic fixture:

    cargo run -p dragonforge-test-lab -- arm-fixture

Example read-only probe on ARM:

    cargo run -p dragonforge-test-lab -- arm-probe --probe board-model

## Physical ARM qualification

On a Raspberry Pi or other native ARM Linux worker:

    bash ./scripts/test-phase20-arm.sh --revision main --install-tools

Optionally include Docker/Podman qualification:

    bash ./scripts/test-phase20-arm.sh --revision main --container-runtime docker

The script validates formatting, strict Clippy, workspace tests, ARM inventory, deterministic HIL fixtures, all typed read-only probes, Phase 19 Linux containment/service/mTLS regressions, worker-service behavior, nextest, llvm-cov, protocol property tests, a GitHub-aware native ARM worker, and the Phase 20 doctor marker.

## Raspberry Pi expectations

A Raspberry Pi should normally report a board model containing `Raspberry Pi`. GPIO, I²C, SPI, UART, and thermal capabilities are reported only when their corresponding Linux interfaces are visible. Disabled buses remain absent rather than being treated as errors.

## Security boundary

DragonForge Test Lab remains not a remote shell.

Phase 20 does not add arbitrary commands, arbitrary filesystem reads, raw device writes, unrestricted GPIO mutation, arbitrary I²C/SPI transactions, UART transmit, public controller addresses, or inbound worker listeners. Existing Agent/Policy/Executor and mTLS boundaries remain authoritative.

## Validation status

**Status: Needs Testing.** Implementation is complete in the repository and deterministic fixture coverage is included, but Phase 20 will remain marked Needs Testing until `scripts/test-phase20-arm.sh` is run on a real Raspberry Pi/physical ARM host and the generated log is reviewed.
