# RPC and SDKs

The CLI exposes line-delimited JSON-RPC 2.0 over stdin/stdout. Protocol version
`2026-07-23` is returned by `stackport.version`. One input line produces one
output line; diagnostics go to stderr.

```sh
stackport rpc
```

```json
{"jsonrpc":"2.0","id":"1","method":"stackport.version","params":{}}
```

## Methods

| Method | Effect |
| --- | --- |
| `stackport.version` | Protocol and engine versions |
| `manifest.validate` | Validate neutral manifest |
| `stackSpec.validate` | Validate YAML object for a target |
| `stackSpec.toManifest` | Convert YAML object to neutral manifest |
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
import { createStackportClient } from "@stackport/sdk";

const client = createStackportClient();
try {
  const doctor = await client.providerProbePlan("vercel");
  const manifest = await client.stackSpecToManifest(spec, "production");
  const plan = await client.providerRequestPlan(manifest, "vercel");
} finally {
  client.close();
}
```

## Python

```python
from stackport_sdk import StackportClient

client = StackportClient()
try:
    doctor = client.provider_probe_plan("supabase")
    manifest = client.stack_spec_to_manifest(spec, "production")
    plan = client.provider_request_plan(manifest, "supabase")
finally:
    client.close()
```

The SDKs intentionally expose JSON-compatible results while the schema is
alpha. They must add ergonomic types without reproducing Rust planning logic.
