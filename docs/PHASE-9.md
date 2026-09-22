# Phase 9 — ChatGPT/MCP Gateway

Status: implementation complete; Windows validation pending.

## Goal

Phase 9 exposes DragonForge Test Lab through a narrowly scoped Model Context Protocol gateway so an MCP-capable assistant or operator client can inspect the lab, submit approved typed test profiles, poll job status, and retrieve result/artifact metadata without receiving shell access or arbitrary command execution.

## Transport

The Phase 9 gateway uses HTTP on a loopback-only bind address.

Default:

    127.0.0.1:45890

Endpoints:

    POST /mcp
    GET  /health

The gateway rejects non-loopback bind addresses.

Phase 9 supports the current modern MCP revision:

    2026-07-28

and the legacy initialization-era revision:

    2025-11-25

Modern requests support `server/discover`, stateless request metadata, and required MCP transport-header validation. Legacy clients can use `initialize`, `tools/list`, and `tools/call`.

## Authentication

Every request to `/mcp` requires:

    Authorization: Bearer <token>

The token is read only from:

    DRAGONFORGE_MCP_TOKEN

Requirements:

- minimum 32 characters;
- never accepted as a CLI argument;
- never printed by the gateway;
- hashed with SHA-256 at gateway initialization;
- raw token text is cleared from retained gateway configuration;
- comparisons use a fixed-length constant-time digest comparison.

The unauthenticated `/health` endpoint reports only generic service readiness.

This is a local operator authentication mechanism, not an Internet-facing OAuth deployment. If the gateway is later exposed beyond loopback, add standards-compliant OAuth/resource metadata plus TLS or an authenticated tunnel before doing so.

## Repository allowlist

MCP-submitted jobs can target only HTTPS repositories matching operator-defined prefixes from:

    DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES

Multiple prefixes are separated by semicolons.

Example:

    $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = "https://github.com/djames1987/DragonForge-Test-Lab"

The allowlist is enforced before a background job is accepted and again before execution.

## MCP tool surface

Phase 9 exposes exactly six tools:

    dragonforge_lab_status
    dragonforge_nodes_list
    dragonforge_job_submit
    dragonforge_job_status
    dragonforge_job_result
    dragonforge_artifact_list

There is no generic execute/run-shell/run-command tool.

### dragonforge_lab_status

Returns gateway version, protocol version, supported MCP revisions, loopback bind information, and active/total job counts.

### dragonforge_nodes_list

Returns worker-node metadata visible to this gateway. Phase 9 initially reports the local authorized MCP worker. Distributed-node aggregation can be expanded in a future phase without changing the tool trust boundary.

### dragonforge_job_submit

Accepts only:

- allowlisted HTTPS repository;
- validated revision/ref;
- named profile.

Supported profiles:

    rust_standard
    rust_test

`rust_standard` maps to the fixed Test Lab action sequence:

    checkout
    cargo fmt --check
    cargo clippy
    cargo test

`rust_test` maps to:

    checkout
    cargo test

No executable name, shell text, script text, raw process arguments, arbitrary PowerShell, or arbitrary Cargo subcommand is accepted.

Submission returns a UUID immediately. Execution runs asynchronously in the existing Agent -> Policy -> LocalExecutor path.

### dragonforge_job_status

Returns bounded queued/running/completed state and timestamps.

### dragonforge_job_result

Returns the final Test Lab status, summary, sandbox mode, and bounded step metadata.

Raw stdout/stderr is intentionally not returned through MCP in Phase 9.

### dragonforge_artifact_list

Returns artifact metadata only:

- name;
- relative path;
- size;
- SHA-256 digest.

The MCP gateway does not provide arbitrary filesystem reads or artifact-content download in Phase 9.

## Async job model

The gateway keeps a bounded in-memory job registry.

Maximum retained jobs:

    1024

When the registry reaches capacity, completed jobs are pruned before new submissions are accepted.

A submitted job transitions through:

    queued
    running
    passed | failed | cancelled | error

Jobs execute on a background worker thread so `tools/call` does not remain open for the duration of a repository build/test run.

## HTTP and JSON-RPC limits

Phase 9 bounds incoming requests:

    HTTP header bytes: 16 KiB
    HTTP body bytes:   256 KiB

JSON-RPC requests must use version 2.0.

Modern MCP requests validate:

- `MCP-Protocol-Version`;
- `Mcp-Method`;
- `Mcp-Name` for named tool calls;
- matching JSON-RPC body values.

Header/body disagreement is rejected before tool execution.

## Artifact containment

Artifacts are collected only from the LocalExecutor-generated artifact directory.

For each file the gateway:

1. canonicalizes the job artifact root;
2. canonicalizes the artifact path;
3. verifies it remains beneath the root;
4. returns relative metadata;
5. computes SHA-256.

At most 256 artifact entries are returned for one job.

## Operator commands

Readiness:

    cargo run -p dragonforge-test-lab -- mcp-doctor

In-process authentication/protocol fixture:

    cargo run -p dragonforge-test-lab -- mcp-fixture

Start gateway:

    cargo run -p dragonforge-test-lab -- mcp-serve

Custom loopback port:

    cargo run -p dragonforge-test-lab -- mcp-serve --bind 127.0.0.1:45890

## Example setup

PowerShell:

    $env:DRAGONFORGE_MCP_TOKEN = "<at-least-32-random-characters>"
    $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = "https://github.com/djames1987/DragonForge-Test-Lab"

    cargo run -p dragonforge-test-lab -- mcp-doctor
    cargo run -p dragonforge-test-lab -- mcp-serve

After use:

    Remove-Item Env:\DRAGONFORGE_MCP_TOKEN
    Remove-Item Env:\DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES

## Validation

Run:

    .\scripts\test-phase9.ps1

The validation performs:

1. cargo fmt;
2. strict Clippy;
3. full workspace tests;
4. MCP host readiness;
5. MCP doctor;
6. in-process unauthorized/discovery fixture;
7. real loopback HTTP gateway startup;
8. unauthenticated 401 verification;
9. modern `server/discover`;
10. modern `tools/list`;
11. modern `dragonforge_lab_status`;
12. legacy `initialize`;
13. real MCP `dragonforge_job_submit`;
14. asynchronous status polling;
15. result retrieval;
16. SHA-256 artifact metadata retrieval;
17. GitHub-aware native worker regression.

The validation script creates a temporary random token only when the operator has not supplied one. The token is not written to the transcript.

## Security boundaries

Phase 9 intentionally does not provide:

- non-loopback listener binding;
- unauthenticated MCP tools;
- arbitrary shell commands;
- arbitrary PowerShell;
- arbitrary executable names or process arguments;
- arbitrary Cargo commands;
- unrestricted repository URLs;
- filesystem browsing;
- artifact-content download;
- secret retrieval;
- firewall/router/VM mutation through MCP;
- generic distributed-node command execution.

The MCP gateway is a high-level typed test interface, not a remote administration API.
