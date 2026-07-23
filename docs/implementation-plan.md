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
- Provider adapter contracts for auth, resource mapping, read/import, plan, apply, state, and secret policy.
- Docs-backed provider API operation templates for each adapter contract.
- JSON-RPC subprocess protocol.
- Security and secret handling.
- Automated tests plus a real manual CLI/SDK verification flow.

## Acceptance Criteria

- A manifest can be validated with graph and secret checks.
- Vercel and Supabase source payloads can be imported into provider-neutral manifests.
- A manifest can be analyzed against target provider capabilities.
- A partial migration plan can be generated for selected resources.
- State can be diffed against a desired manifest.
- State-aware planning emits create, update, delete, replace, no-op, manual, or unsupported actions.
- Typed provider request plans can be inspected without resolving credentials.
- Read/import requests execute against provider APIs and return redacted observations.
- Refreshed plans compare safe neutral fields from provider observations with desired YAML and surface provider-side drift.
- Apply executes create/update/delete operations and atomically advances state only for successful resources.
- Provider definitions exist for Vercel, Supabase, Neon, and Railway.
- Provider execution plans map neutral resources to platform resources before apply.
- Provider execution plans include the concrete REST or GraphQL requests adapters will call.
- Vercel, Supabase, Neon, and Railway requests cover the resource families declared in their provider contracts.
- Provider tokens, secret values, and connection strings never enter plans, reports, or state.
- Rust, TypeScript, and Python SDKs call the same core engine.
- Tests cover core validation, importers, state-aware planning, request building, real HTTP transport, redaction, RPC, and SDK wrappers.
- Manual verification exercises CLI, JSON-RPC, TypeScript SDK, and Python SDK.
