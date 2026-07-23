# Stackport Developer Documentation

| Document | Purpose |
| --- | --- |
| [Architecture](architecture.md) | Engine boundaries and plan/apply data flow |
| [YAML reference](yaml-reference.md) | Complete `stackport/app/v1alpha1` authoring contract |
| [Provider API contracts](provider-api-contracts.md) | Supported providers, resources, auth, and lifecycle limits |
| [State and secrets](state-and-secrets.md) | State contents, drift, deletion, and secret handling |
| [RPC and SDKs](rpc-and-sdks.md) | Versioned subprocess protocol and SDK examples |
| [Provider development](provider-development.md) | Adding and testing an adapter |
| [Testing](testing.md) | Automated, local HTTP, live read-only, and manual checks |

The fastest way to understand the product is to read the YAML reference, run a
provider example through `stack validate` and `stack plan --provider-requests`,
then follow that request through `provider_runtime.rs`.
