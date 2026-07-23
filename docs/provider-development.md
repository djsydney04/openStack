# Provider Development

Provider support is a product contract, not a collection of unrelated HTTP
calls. Add an adapter in this order.

## 1. Define the Contract

Add a `ProviderDefinition` in `providers.rs` with:

- provider name, display name, and auth environment variables
- supported capabilities
- neutral resource to provider resource mappings
- honest lifecycle flags for read, create, update, and delete
- official API operation, method, path, required identifiers, and docs URL
- state ID and secret policies

Do not mark an operation executable until request construction and a contract
test exist. A manual lifecycle is preferable to a mutation that only works for
one undocumented payload.

## 2. Compile Requests

Implement provider-specific body construction in `provider_runtime.rs`. Keep
provider routing data out of the body. Required path or GraphQL identifiers
must come from target config, prior state, resource properties, or a same-provider
dependency response binding.

Secrets remain `${secret:NAME}` placeholders during planning. Add response ID
extraction for every identifier that a later resource needs.

## 3. Add an Example

Create `examples/<provider>.stack.yaml` with every executable resource family,
real dependencies, and secret references. It must pass:

```sh
stackport stack validate examples/<provider>.stack.yaml --target production
stackport stack plan examples/<provider>.stack.yaml --target production --provider-requests
```

## 4. Test Four Layers

1. Registry test: each advertised lifecycle has an API operation.
2. Compiler test: the example has no warnings or unresolved IDs.
3. Contract transport: create and destroy flows validate URLs, GraphQL
   variables, body shape, dependency IDs, state, and redaction.
4. Real transport: localhost HTTP verifies methods, headers, JSON or multipart,
   response parsing, and sanitized reports.

Add a read-only live probe only when the provider offers a harmless identity or
list endpoint. Never make live mutation tests part of default CI.

## 5. Document Limits

Update the README coverage table and `provider-api-contracts.md`. State manual
operations, additional credentials, destructive behavior, and fields that
cannot be safely compared for drift.
