# Testing OpenManifest

The test strategy separates deterministic engine behavior, real HTTP encoding,
SDK integration, manual CLI behavior, and opt-in live provider access.

## Full Local Suite

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
npm test --prefix packages/typescript
python3 -m unittest discover -s packages/python/tests
bash tests/openmanifest-ci-workflow.sh
```

`provider_contracts.rs` independently verifies all four providers at six
levels:

- every advertised lifecycle has a concrete API operation;
- every provider example validates;
- every example compiles without unresolved dependency IDs;
- every create flow executes through a strict provider-aware fake;
- every destroy flow uses prior state and reverse dependency order;
- all access probes travel through the real `reqwest` transport to localhost.

The core suite separately verifies secret resolution, response redaction,
state-aware planning, drift, JSON-RPC, real JSON HTTP requests, and real Supabase
multipart uploads.

`openmanifest-ci-workflow.sh` creates a real temporary bare Git remote and exercises state
branch creation, update, retrieval, remote-failure handling, state validation,
multiline secret export, and plan gating. It never contacts a provider.

Repository CI also calls the reusable workflow against the Vercel example with
apply disabled. That hosted smoke test covers `workflow_call`, both checkouts,
state loading, a release build, CLI planning, summary generation, and artifact
upload without provider credentials.

## Provider Examples

```sh
openmanifest validate examples/vercel.openmanifest.yaml --target production
openmanifest plan examples/vercel.openmanifest.yaml --target production --provider-requests

openmanifest validate examples/supabase.openmanifest.yaml --target production
openmanifest plan examples/supabase.openmanifest.yaml --target production --provider-requests

openmanifest validate examples/neon.openmanifest.yaml --target production
openmanifest plan examples/neon.openmanifest.yaml --target production --provider-requests

openmanifest validate examples/railway.openmanifest.yaml --target production
openmanifest plan examples/railway.openmanifest.yaml --target production --provider-requests
```

Each request plan should report `"executable": true`, no warnings, no unresolved
identifiers, and no plaintext token or secret.

## Read-Only Live Tests

Set only the credential for the provider being tested:

```sh
VERCEL_TOKEN=... cargo test -p openmanifest-core --test live_provider_smoke vercel_live_access_probe -- --ignored --nocapture
SUPABASE_ACCESS_TOKEN=... cargo test -p openmanifest-core --test live_provider_smoke supabase_live_access_probe -- --ignored --nocapture
NEON_API_KEY=... cargo test -p openmanifest-core --test live_provider_smoke neon_live_access_probe -- --ignored --nocapture
RAILWAY_TOKEN=... cargo test -p openmanifest-core --test live_provider_smoke railway_live_access_probe -- --ignored --nocapture
```

Equivalent installed CLI commands:

```sh
openmanifest providers doctor
openmanifest providers doctor vercel --execute
```

These probes perform only an identity or minimal list query. Reports include
provider, status, HTTP status, and message. They omit observed account data.

## Manual Apply Test

Use a disposable provider project or account. First inspect the exact request
plan, then apply, rerun to verify no-op behavior, change one non-secret field to
verify update, and finally test destroy only after reviewing deletions.

```sh
openmanifest plan openmanifest.yaml --target test --provider-requests
openmanifest apply openmanifest.yaml --target test --state /tmp/openmanifest-test-state.json --auto-approve
openmanifest plan openmanifest.yaml --target test --state /tmp/openmanifest-test-state.json --refresh
```

Live mutation tests are intentionally not in CI because they cost money and can
delete real resources. Record the provider, disposable account, command, and
result when performing a release qualification.

## Manual CLI and RPC Checks

```sh
openmanifest validate examples/vercel.openmanifest.yaml --target production
openmanifest compile examples/vercel.openmanifest.yaml --target production
openmanifest plan examples/vercel.openmanifest.yaml --target production --provider-requests
openmanifest providers doctor
openmanifest rpc --once '{"jsonrpc":"2.0","id":"manual-1","method":"openmanifest.version","params":{}}'
```
