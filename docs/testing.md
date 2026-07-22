# Testing Stackport

## Automated Tests

```sh
cargo test
npm test --prefix packages/typescript
python3 -m unittest discover -s packages/python/tests
```

## Manual Verification

These commands exercise the built CLI and the same Rust engine used by all SDKs.

```sh
cargo run -q -p stackport-cli -- validate fixtures/manifest.basic.json
cargo run -q -p stackport-cli -- import vercel fixtures/vercel.project.json
cargo run -q -p stackport-cli -- import supabase fixtures/supabase.project.json
cargo run -q -p stackport-cli -- analyze fixtures/manifest.basic.json --target vercel
cargo run -q -p stackport-cli -- plan fixtures/manifest.basic.json --target render --include web
cargo run -q -p stackport-cli -- diff fixtures/manifest.basic.json fixtures/state.basic.json
cargo run -q -p stackport-cli -- apply fixtures/plan.render.partial.json --dry-run
cargo run -q -p stackport-cli -- rpc --once '{"jsonrpc":"2.0","id":"manual-1","method":"stackport.version","params":{}}'
cargo run -q -p stackport-cli -- stack validate fixtures/stack.app.yaml --target production
cargo run -q -p stackport-cli -- stack manifest fixtures/stack.app.yaml --target production
cargo run -q -p stackport-cli -- stack plan fixtures/stack.app.yaml --target production
cargo run -q -p stackport-cli -- stack plan fixtures/stack.app.yaml --target production --provider-details
cargo run -q -p stackport-cli -- providers show railway
```

Expected behavior:

- Validation returns `{ "valid": true }`.
- Vercel import creates a web service, domain resource, and secret references.
- Supabase import creates database, auth, storage bucket, and function resources.
- Vercel analysis marks the database as unsupported because Vercel lacks Postgres.
- Partial Render plan includes only `web` and warns that `database` is outside scope.
- Diff reports update/create/delete against the stale fixture state.
- Dry-run apply returns a planned create for `web`.
- RPC version returns `rpc_version: "2026-07-22"`.
- Stack YAML validation succeeds and target-specific manifest generation maps `service:web` to Railway for production.
- Provider details show the adapter contract, auth methods, resource lifecycle support, state ID format, and no plaintext secret storage.
