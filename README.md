# OpenManifest

OpenManifest is simple infrastructure as code for application platforms. A single
YAML file describes services, databases, environments, domains, functions, and
secrets. The Rust engine turns that desired state into an ordered plan, calls
provider APIs, and records enough state to update or destroy the resources later.

The current adapters are Vercel, Supabase, Neon, and Railway.

## What OpenManifest Does

1. You describe the application resources you want in one YAML file.
2. OpenManifest validates dependencies, secret references, provider settings,
   and the selected target.
3. The Rust engine compiles that YAML into a provider-neutral resource graph.
4. It compares desired resources with prior state and produces an ordered
   create, update, replace, delete, or no-op plan.
5. It compiles the exact REST or GraphQL requests for each provider without
   resolving credentials, so the plan is safe to review in pull requests.
6. After explicit approval, it resolves credentials and secrets only at the
   HTTP boundary, executes requests in dependency order, redacts responses,
   and atomically records successful resource state.
7. The same process can run in GitHub Actions: pull requests plan without
   credentials, while protected default-branch jobs can apply after merge.

OpenManifest currently manages the provider resources listed in the support
matrix below. Import is read-only discovery and does not yet adopt discovered
resources into state automatically. Local state has no distributed lock; the
included GitHub Actions state backend adds serialized apply jobs and stale-write
protection using a dedicated Git branch.

## Quick Start

```sh
cargo build --workspace
target/debug/openmanifest validate examples/railway.openmanifest.yaml --target production
target/debug/openmanifest plan examples/railway.openmanifest.yaml --target production
target/debug/openmanifest plan examples/railway.openmanifest.yaml --target production --provider-requests
```

Planning is credential-free and never contacts a provider. To apply a plan:

```sh
export RAILWAY_TOKEN=...
export API_TOKEN=...
export SESSION_SECRET=...
target/debug/openmanifest providers doctor railway --execute
target/debug/openmanifest apply examples/railway.openmanifest.yaml --target production --auto-approve
```

Approved apply writes `.openmanifest/state.json`. Commit the YAML, but do not
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
version: openmanifest/app/v1alpha1
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

- [`examples/vercel.openmanifest.yaml`](examples/vercel.openmanifest.yaml)
- [`examples/supabase.openmanifest.yaml`](examples/supabase.openmanifest.yaml)
- [`examples/neon.openmanifest.yaml`](examples/neon.openmanifest.yaml)
- [`examples/railway.openmanifest.yaml`](examples/railway.openmanifest.yaml)

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

- `crates/openmanifest-core`: graph validation, planning, provider contracts,
  request compilation, HTTP execution, drift, state, and JSON-RPC handlers.
- `crates/openmanifest-cli`: human CLI and line-delimited JSON-RPC subprocess.
- `crates/openmanifest-sdk`: native Rust API.
- `packages/typescript`: Node.js SDK over the Rust subprocess protocol.
- `packages/python`: Python SDK over the same protocol.

TypeScript and Python do not reimplement planning or provider behavior.

## Documentation

Start with [`docs/README.md`](docs/README.md). It links the architecture, YAML
reference, provider contracts, state and secret model, SDK protocol, adapter
development guide, and test procedures.

## Rename Compatibility

New manifests use `openmanifest/app/v1alpha1`, state uses
`openmanifest-state/v1`, and SDKs call `openmanifest.version`. During the rename
window, the engine also accepts the previous Stackport manifest/state versions
and version RPC method. Compatibility workflow and SDK aliases remain available
for existing source integrations; new projects should use only OpenManifest
names.
