# RPC and SDKs

The CLI exposes line-delimited JSON-RPC 2.0 over stdin/stdout. Protocol version
`2026-08-14` is returned by `openmanifest.version`. One input line produces one
output line; diagnostics go to stderr.

```sh
openmanifest rpc
```

```json
{"jsonrpc":"2.0","id":"1","method":"openmanifest.version","params":{}}
```

## Methods

| Method | Effect |
| --- | --- |
| `openmanifest.version` | Protocol and engine versions |
| `manifest.validate` | Validate neutral manifest |
| `appManifest.validate` | Validate an OpenManifest YAML object for a target |
| `appManifest.compile` | Compile an OpenManifest object into the neutral resource graph |
| `plan.create` | State-aware ordered migration plan |
| `state.diff` | Manifest/state difference |
| `providers.list`, `providers.show` | Provider contracts |
| `providers.probePlan`, `providers.probe` | Read-only provider access check |
| `providers.executionPlan` | Resource-to-provider operation mapping |
| `providers.requestPlan` | Credential-free HTTP/GraphQL request plan |
| `providers.readPlan`, `providers.importPlan` | Credential-free observation plans |
| `providers.read`, `providers.import` | Execute read-only observations |
| `providers.refreshPlan` | Read drift and reconcile a plan |
| `providers.apply` | Execute mutations; requires `confirm: true` |
| `import.vercel`, `import.supabase` | Convert importer fixtures |

## TypeScript

```ts
import { createOpenManifestClient } from "@openmanifest/sdk";

const client = createOpenManifestClient();
try {
  const doctor = await client.providerProbePlan("vercel");
  const manifest = await client.compileAppManifest(spec, "production");
  const plan = await client.providerRequestPlan(manifest, "vercel");
} finally {
  client.close();
}
```

## Python

```python
from openmanifest_sdk import OpenManifestClient

client = OpenManifestClient()
try:
    doctor = client.provider_probe_plan("supabase")
    manifest = client.compile_app_manifest(spec, "production")
    plan = client.provider_request_plan(manifest, "supabase")
finally:
    client.close()
```

The SDKs intentionally expose JSON-compatible results while the schema is
alpha. They must add ergonomic types without reproducing Rust planning logic.
