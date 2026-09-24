# Dogfood Manual Validation Inventory

Phase 25 automates routine, repeatable validation where Test Lab already has a typed and security-reviewed execution path. It deliberately leaves the following work manual or explicitly opt-in.

| Area | Automation status | Reason |
| --- | --- | --- |
| Rust formatting, Clippy, tests, release build profiles | Automated | Existing typed TestAction surface |
| Immutable GitHub revision resolution | Automated | Existing df-test-github fixed gh API adapter |
| Test Lab self-host validation | Automated | One-layer bounded dogfood execution |
| Multi-repository DragonForge campaign | Automated / explicit operator invocation | Real repositories may have project-specific readiness requirements |
| Physical Raspberry Pi / ARM HIL | Manual pending hardware | Requires real attached hardware and Phase 20 qualification |
| Production code-signing identity | Manual | Private signing credentials remain operator controlled |
| Privileged Windows security operations | Explicit opt-in | Elevation and host mutation require human approval |
| Destructive security scenarios | Isolated / explicit | Must not run implicitly on development hosts |
| Subjective UX / visual acceptance | Manual when applicable | Not safely reducible to existing typed deterministic checks |

## Rule

A manual item must not be converted into automatic dogfood execution merely to reduce the count of manual steps. It moves to automated status only after Test Lab has a typed, bounded, documented execution path with appropriate security and recovery controls.
