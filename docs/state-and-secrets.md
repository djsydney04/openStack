# State and Secrets

## State Contents

The default state path is `.openmanifest/state.json`. State schema
`openmanifest-state/v1` records:

- neutral resource ID and provider
- provider resource type and stable provider ID
- non-secret provider identifiers needed by later reads, updates, or deletes
- desired fingerprint
- last-applied neutral resource snapshot
- secret names and resolver references

It does not record provider tokens, submitted secret values, connection URIs,
returned role passwords, or decrypted environment variables.

The last-applied snapshot is required for reliable deletion. Once a resource is
removed from YAML, OpenManifest still needs its old bucket name, function slug,
variable name, dependencies, and provider metadata to compile the destroy call.

## Apply and Drift

`openmanifest plan` compares desired fingerprints with local state. `openmanifest plan
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

## CI State Backend

The reusable GitHub Actions workflow stores state in a dedicated orphan branch,
`openmanifest-state`, at `states/<state-key>.json`. This keeps state durable across
ephemeral runners without mixing generated state into the application branch.
The workflow serializes apply jobs per repository and uses a normal
fast-forward push, so a stale writer fails instead of overwriting newer state.

This branch is not an encrypted secret store. It has the same visibility and
retention characteristics as the repository and contains provider identifiers
and secret references. Configure it according to the repository's retention and
access policy. See [GitHub Actions CI/CD](ci-cd.md) for setup and recovery.

## Operational Limits

- Local state uses atomic replacement but has no distributed lock.
- The Git branch CI backend has workflow-level serialization and stale-write
  protection, but it is not a general distributed lock or encrypted backend.
- Provider-side deletion is real and can cascade. Always review the printed
  request plan before `--auto-approve`.
- Supabase Storage operations require `SUPABASE_SERVICE_ROLE_KEY` in addition
  to the Management API token.
