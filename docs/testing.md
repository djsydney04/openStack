# Testing Stackport

The test strategy separates deterministic engine behavior, real HTTP encoding,
SDK integration, manual CLI behavior, and opt-in live provider access.

## Full Local Suite

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
npm test --prefix packages/typescript
python3 -m unittest discover -s packages/python/tests
bash tests/ci-workflow.sh
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

`ci-workflow.sh` creates a real temporary bare Git remote and exercises state
branch creation, update, retrieval, remote-failure handling, state validation,
multiline secret export, and plan gating. It never contacts a provider.

## Provider Examples

```sh
stackport stack validate examples/vercel.stack.yaml --target production
stackport stack plan examples/vercel.stack.yaml --target production --provider-requests

stackport stack validate examples/supabase.stack.yaml --target production
stackport stack plan examples/supabase.stack.yaml --target production --provider-requests

stackport stack validate examples/neon.stack.yaml --target production
stackport stack plan examples/neon.stack.yaml --target production --provider-requests

stackport stack validate examples/railway.stack.yaml --target production
stackport stack plan examples/railway.stack.yaml --target production --provider-requests
```

Each request plan should report `"executable": true`, no warnings, no unresolved
identifiers, and no plaintext token or secret.

## Read-Only Live Tests

Set only the credential for the provider being tested:

```sh
VERCEL_TOKEN=... cargo test -p stackport-core --test live_provider_smoke vercel_live_access_probe -- --ignored --nocapture
SUPABASE_ACCESS_TOKEN=... cargo test -p stackport-core --test live_provider_smoke supabase_live_access_probe -- --ignored --nocapture
NEON_API_KEY=... cargo test -p stackport-core --test live_provider_smoke neon_live_access_probe -- --ignored --nocapture
RAILWAY_TOKEN=... cargo test -p stackport-core --test live_provider_smoke railway_live_access_probe -- --ignored --nocapture
```

Equivalent installed CLI commands:

```sh
stackport providers doctor
stackport providers doctor vercel --execute
```

These probes perform only an identity or minimal list query. Reports include
provider, status, HTTP status, and message. They omit observed account data.

## Manual Apply Test

Use a disposable provider project or account. First inspect the exact request
plan, then apply, rerun to verify no-op behavior, change one non-secret field to
verify update, and finally test destroy only after reviewing deletions.

```sh
stackport stack plan stack.yaml --target test --provider-requests
stackport stack apply stack.yaml --target test --state /tmp/stackport-test-state.json --auto-approve
stackport stack plan stack.yaml --target test --state /tmp/stackport-test-state.json --refresh
```

Live mutation tests are intentionally not in CI because they cost money and can
delete real resources. Record the provider, disposable account, command, and
result when performing a release qualification.

## Legacy Fixture Checks

```sh
stackport validate fixtures/manifest.basic.json
stackport import vercel fixtures/vercel.project.json
stackport import supabase fixtures/supabase.project.json
stackport diff fixtures/manifest.basic.json fixtures/state.basic.json
stackport rpc --once '{"jsonrpc":"2.0","id":"manual-1","method":"stackport.version","params":{}}'
```
