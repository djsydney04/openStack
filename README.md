# Stackport

Stackport is a provider-neutral portability toolkit for application stacks.

The repository is organized around one implementation of migration logic:

- `crates/stackport-core`: Rust core engine and native Rust SDK surface.
- `crates/stackport-cli`: CLI and versioned subprocess JSON-RPC server.
- `packages/typescript`: TypeScript SDK that calls the CLI RPC protocol.
- `packages/python`: Python SDK that calls the CLI RPC protocol.

The TypeScript and Python SDKs intentionally do not reimplement Stackport's
graph, validation, analysis, planning, diffing, or apply logic. They launch the
Rust CLI as a subprocess and exchange versioned JSON-RPC messages.

## Quick Start

```sh
cargo run -p stackport-cli -- validate fixtures/manifest.basic.json
cargo run -p stackport-cli -- import vercel fixtures/vercel.project.json
cargo run -p stackport-cli -- analyze fixtures/manifest.basic.json --target render
cargo run -p stackport-cli -- plan fixtures/manifest.basic.json --target render
```

## App-Layer IaC YAML

Stackport also supports a simple YAML authoring format for app-layer
infrastructure. The YAML describes services, databases, secrets, domains, and
deployment targets. Changing the selected target changes deployment details
without rewriting provider-specific manifests.

```sh
cargo run -p stackport-cli -- stack validate fixtures/stack.app.yaml --target production
cargo run -p stackport-cli -- stack manifest fixtures/stack.app.yaml --target production
cargo run -p stackport-cli -- stack plan fixtures/stack.app.yaml --target production
cargo run -p stackport-cli -- stack plan fixtures/stack.app.yaml --target production --provider-details
cargo run -p stackport-cli -- providers show railway
```

Provider definitions are part of the product contract. Each provider declares
auth methods, resource mappings, read/import/plan/apply/delete support, state ID
policy, and secret handling. Vercel, Supabase, Neon, and Railway are registered
first; Render, Fly, and Netlify can be added behind the same contract.
See `docs/provider-api-contracts.md` for the docs-backed API surfaces that each
adapter will call.

Example:

```yaml
version: stackport/app/v1alpha1
app:
  name: demo-stack
targets:
  preview:
    provider: vercel
  production:
    provider: railway
services:
  web:
    build:
      framework: nextjs
      command: npm run build
    env:
      DATABASE_URL:
        secret: DATABASE_URL
databases:
  primary:
    provider: neon
    engine: postgres
secrets:
  DATABASE_URL:
    from: neon:primary:DATABASE_URL
```

## RPC

Start a line-delimited JSON-RPC subprocess:

```sh
cargo run -p stackport-cli -- rpc
```

Each line is a request:

```json
{"jsonrpc":"2.0","id":"1","method":"stackport.version","params":{}}
```

Each response includes either `result` or `error`.

## Security Model

Stackport manifests reference secrets by name. Importers reject likely secret
values and preserve only secret references. Apply is dry-run only until a caller
provides a concrete provider adapter from the Rust SDK.
