# Provider API Contracts

Stackport treats each provider adapter as a small contract around an official
API surface. The core engine does not execute HTTP requests yet, but it can now
produce the API operation template that an adapter will use for read/import,
plan, create, update, and delete.

## Vercel

- Protocol: REST
- Base URL: `https://api.vercel.com`
- Auth: `Authorization: Bearer ${VERCEL_TOKEN}`
- Resource mappings:
  - `web_service` -> project
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
  - `database` -> project database
  - `auth` -> auth config
  - `storage_bucket` -> storage bucket
  - `function` -> Edge Function
  - `secret` -> project secret
- Primary docs:
  - `https://supabase.com/docs/reference/api/introduction`
  - `https://supabase.com/docs/reference/api/getting-started`
  - `https://supabase.com/changelog/33720-deploy-and-update-edge-functions-using-the-management-api`

## Neon

- Protocol: REST
- Base URL: `https://console.neon.tech/api/v2`
- Auth: `Authorization: Bearer ${NEON_API_KEY}`
- Resource mappings:
  - `database` -> project, branch, database, role, connection URI
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
provider IDs, provider resource names, fingerprints, and secret references.

