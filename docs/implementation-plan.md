# Stackport Implementation Plan

## Scope

Build a first complete Stackport implementation from the PRD summary:

- Rust core engine and CLI.
- Rust SDK/native access.
- TypeScript SDK over versioned subprocess RPC.
- Python SDK over versioned subprocess RPC.
- Vercel and Supabase importers.
- Provider-neutral manifests.
- Capability and compatibility modeling.
- Partial migrations.
- Rust adapter traits.
- JSON-RPC subprocess protocol.
- Security and secret handling.
- Automated tests plus a real manual CLI/SDK verification flow.

## Acceptance Criteria

- A manifest can be validated with graph and secret checks.
- Vercel and Supabase source payloads can be imported into provider-neutral manifests.
- A manifest can be analyzed against target provider capabilities.
- A partial migration plan can be generated for selected resources.
- State can be diffed against a desired manifest.
- Apply logic can run as a dry run through the adapter interface.
- Rust, TypeScript, and Python SDKs call the same core engine.
- Tests cover core validation, importers, planning, RPC, and SDK wrappers.
- Manual verification exercises CLI, JSON-RPC, TypeScript SDK, and Python SDK.

