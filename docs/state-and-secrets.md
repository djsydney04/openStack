# State and Secrets

## State Contents

The default state path is `.stackport/state.json`. State schema
`stackport-state/v1` records:

- neutral resource ID and provider
- provider resource type and stable provider ID
- non-secret provider identifiers needed by later reads, updates, or deletes
- desired fingerprint
- last-applied neutral resource snapshot
- secret names and resolver references

It does not record provider tokens, submitted secret values, connection URIs,
returned role passwords, or decrypted environment variables.

The last-applied snapshot is required for reliable deletion. Once a resource is
removed from YAML, Stackport still needs its old bucket name, function slug,
variable name, dependencies, and provider metadata to compile the destroy call.

## Apply and Drift

`stack plan` compares desired fingerprints with local state. `stack plan
--refresh` also performs provider reads and can turn a local no-op into an
update when comparable remote fields drifted. Approved apply automatically
refreshes when prior state exists and refuses to continue if a required read
fails.

Observed response fields are recursively redacted. Reads can be `in_sync`,
`drifted`, `unknown`, or `read_failed`; `unknown` means the provider response did
not expose a safely comparable field.

## Secret Boundary

Plans contain `${secret:NAME}`, the resolver reference, and required environment
variable names. At execution time:

1. auth headers are resolved from the process environment;
2. dependency-produced secret values are held in memory;
3. `${secret:NAME}` placeholders are replaced immediately before HTTP send;
4. response bodies are redacted before they become serializable reports;
5. runtime secret values are dropped after the request sequence.

Use process-level secret injection from CI or a secret manager. Do not put a
token in `targets.*.config`, resource `config`, `properties`, or checked-in
state.

## Operational Limits

- State is local and uses atomic replacement, but has no distributed lock.
- Remote state, encryption at rest, and team locking are not implemented.
- Provider-side deletion is real and can cascade. Always review the printed
  request plan before `--auto-approve`.
- Supabase Storage operations require `SUPABASE_SERVICE_ROLE_KEY` in addition
  to the Management API token.
