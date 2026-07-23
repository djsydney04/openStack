# Provider API Contracts

Stackport treats each provider adapter as a small contract around an official
API surface. The core compiles these definitions into typed, inspectable request
plans and executes them through a shared HTTP transport. REST and GraphQL
adapters share auth resolution, identifier substitution, response redaction,
failure reporting, and state advancement.

The runtime has three boundaries:

1. Request planning is pure and serializable. It contains URLs, operation names,
   unresolved identifiers, and environment variable names, but no credentials.
2. Execution resolves credentials and secret references in memory and performs
   HTTP requests. Apply is fail-fast; independent read/import observations are
   not.
3. Reports contain status, provider IDs, identifiers, and redacted observed
   configuration. State advances only for successful mutations.

## Vercel

- Protocol: REST
- Base URL: `https://api.vercel.com`
- Auth: `Authorization: Bearer ${VERCEL_TOKEN}`
- Resource mappings:
  - `project` -> project
  - `web_service` -> project
  - `build` -> deployment
  - `deploy_hook` -> manual lifecycle
  - `secret` -> project environment variable
  - `domain` -> project domain
- Primary docs:
  - `https://vercel.com/docs/rest-api`
  - `https://vercel.com/docs/integrations/create-integration/vercel-api-integrations`
  - `https://vercel.com/docs/rest-api/reference/endpoints/projects/verify-project-domain`

## Supabase

- Protocol: REST
- Base URL: `https://api.supabase.com`
- Auth: `Authorization: Bearer ${SUPABASE_ACCESS_TOKEN}`
- Resource mappings:
  - `project` -> project database
  - `database` -> project database
  - `auth` -> auth config
  - `storage_bucket` -> storage bucket
  - `function` -> Edge Function
  - `secret` -> project secret
- Primary docs:
  - `https://supabase.com/docs/reference/api/introduction`
  - `https://supabase.com/docs/reference/api/getting-started`
  - `https://supabase.com/changelog/33720-deploy-and-update-edge-functions-using-the-management-api`

Edge Function create/update requests use the Management API's required
`multipart/form-data` encoding. The resource supplies `slug`, metadata such as
`entrypoint_path`, and a `files` array of local paths (or `{ path, name }`
objects); source bytes are read only at execution time.

## Neon

- Protocol: REST
- Base URL: `https://console.neon.tech/api/v2`
- Auth: `Authorization: Bearer ${NEON_API_KEY}`
- Resource mappings:
  - `project` / `database` -> project, default branch, and database
  - `database_branch` -> branch
  - `database_role` -> role
  - `connection_string` -> runtime-only connection URI reference
  - `secret` -> connection string reference
- Primary docs:
  - `https://api-docs.neon.tech/reference/getting-started-with-neon-api`
  - `https://api-docs.neon.tech/reference/createproject`
  - `https://api-docs.neon.tech/reference/getconnectionuri`

## Railway

- Protocol: GraphQL
- Base URL: `https://backboard.railway.com/graphql/v2`
- Auth:
  - `Authorization: Bearer ${RAILWAY_TOKEN}`
  - `Project-Access-Token: ${RAILWAY_PROJECT_TOKEN}`
- Resource mappings:
  - `project` -> project
  - `environment` -> environment
  - `web_service` -> service
  - `database` -> Postgres service
  - `secret` -> variable
  - `domain` -> custom domain
- Primary docs:
  - `https://docs.railway.com/integrations/api`
  - `https://docs.railway.com/integrations/api/api-cookbook`
  - `https://docs.railway.com/integrations/api/manage-variables`
  - `https://docs.railway.com/integrations/api/manage-domains`

## Secret Rule

Provider API specs may include operations that write secret values, but
Stackport state and plan output must never store those values. State stores only
provider IDs, provider resource names, identifiers, last-applied configuration,
fingerprints, and secret references. Neon connection URIs and returned provider
credentials can flow directly to a dependent request in memory and are then
discarded.
