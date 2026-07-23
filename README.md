# Stackport

Stackport is simple infrastructure as code for application platforms. A single
YAML file describes services, databases, environments, domains, functions, and
secrets. The Rust engine turns that desired state into an ordered plan, calls
provider APIs, and records enough state to update or destroy the resources later.

The current adapters are Vercel, Supabase, Neon, and Railway.

## Quick Start

```sh
cargo build --workspace
target/debug/stackport stack validate examples/railway.stack.yaml --target production
target/debug/stackport stack plan examples/railway.stack.yaml --target production
target/debug/stackport stack plan examples/railway.stack.yaml --target production --provider-requests
```

Planning is credential-free and never contacts a provider. To apply a plan:

```sh
export RAILWAY_TOKEN=...
export API_TOKEN=...
export SESSION_SECRET=...
target/debug/stackport providers doctor railway --execute
target/debug/stackport stack apply examples/railway.stack.yaml --target production --auto-approve
```

Approved apply writes `.stackport/state.json`. Commit the YAML, but do not
commit the state file unless your team has explicitly chosen Git as its state
backend.

## Automatic Apply in CI

The reusable GitHub Actions workflow produces a credential-free plan on pull
requests and can apply the merged YAML from the repository's default branch.
The apply job supports GitHub Environment approval, serial execution, protected
secret injection, and durable secret-free state on a dedicated branch.

See [`docs/ci-cd.md`](docs/ci-cd.md) for the consumer workflow and production
setup.

## The YAML Model

```yaml
version: stackport/app/v1alpha1
app:
  name: my-app

targets:
  preview:
    provider: vercel
  production:
    provider: railway

services:
  web:
    source:
      repository: owner/repository
    build:
      command: npm run build
    run:
      command: npm start
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

Dependencies are explicit, plans are topologically ordered, and dependency IDs
returned by one API call are passed to later calls in memory. Secret values are
resolved only at the HTTP boundary and are not serialized into plans, reports,
or state.

Complete examples:

- [`examples/vercel.stack.yaml`](examples/vercel.stack.yaml)
- [`examples/supabase.stack.yaml`](examples/supabase.stack.yaml)
- [`examples/neon.stack.yaml`](examples/neon.stack.yaml)
- [`examples/railway.stack.yaml`](examples/railway.stack.yaml)

## Provider Coverage

| Provider | Executable resources | Explicit limits |
| --- | --- | --- |
| Vercel | projects/services, environment variables, domains, deployments | deploy hooks remain manual |
| Supabase | projects/databases, Auth config, Storage buckets, Edge Functions, project secrets | Auth deletion is not exposed; Storage uses a service-role key |
| Neon | projects/databases, branches, roles | role update and connection-string creation are not lifecycle operations |
| Railway | projects, environments, services, variables, domains | Railway Postgres template deployment remains manual |

`providers show <name>` returns the machine-readable resource, lifecycle, auth,
API, state, and secret contract for an adapter.

## Architecture

- `crates/stackport-core`: graph validation, planning, provider contracts,
  request compilation, HTTP execution, drift, state, and JSON-RPC handlers.
- `crates/stackport-cli`: human CLI and line-delimited JSON-RPC subprocess.
- `crates/stackport-sdk`: native Rust API.
- `packages/typescript`: Node.js SDK over the Rust subprocess protocol.
- `packages/python`: Python SDK over the same protocol.

TypeScript and Python do not reimplement planning or provider behavior.

## Documentation

Start with [`docs/README.md`](docs/README.md). It links the architecture, YAML
reference, provider contracts, state and secret model, SDK protocol, adapter
development guide, and test procedures.
