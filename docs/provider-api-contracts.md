# Provider API Contracts

Provider definitions are machine-readable contracts in `providers.rs`. The
runtime compiles them into REST or GraphQL requests and shares auth resolution,
dependency ID substitution, fail-fast apply, independent observations,
redaction, and state advancement across every adapter.

`openmanifest providers show <provider>` prints the full contract. `openmanifest
providers doctor [provider]` prints the harmless access probe; add `--execute`
to perform it.

## Support Matrix

| Provider resource | Read | Import discovery | Create | Update | Delete |
| --- | ---: | ---: | ---: | ---: | ---: |
| Vercel project/service | yes | yes | yes | yes | yes |
| Vercel environment variable | yes | yes | yes | yes | yes |
| Vercel domain | yes | yes | yes | verify | yes |
| Vercel deployment | yes | yes | yes | no | yes |
| Vercel deploy hook | no | no | manual | manual | manual |
| Supabase project/database | yes | yes | yes | yes | yes |
| Supabase Auth config | yes | yes | configure | yes | no |
| Supabase Storage bucket | yes | yes | yes | yes | yes |
| Supabase Edge Function | yes | yes | deploy | redeploy | yes |
| Supabase project secret | names | names | yes | upsert | yes |
| Neon project/database | yes | yes | yes | yes | yes |
| Neon branch | yes | yes | yes | yes | yes |
| Neon role | yes | yes | yes | no | yes |
| Neon connection reference | verify | yes | no | no | no |
| Railway project | yes | yes | yes | yes | yes |
| Railway environment | yes | yes | yes | yes | yes |
| Railway service | yes | yes | yes | yes | yes |
| Railway variable | unrendered | names | upsert | upsert | yes |
| Railway domain | yes | yes | yes | yes | yes |
| Railway Postgres template | no | no | manual | manual | manual |

Import is currently discovery output. It does not silently adopt resources into
the state file.

## Vercel

- REST base: `https://api.vercel.com`
- Auth: `Authorization: Bearer ${VERCEL_TOKEN}`
- Access probe: `GET /v2/user`
- State identity: project/deployment/domain/env IDs returned by the API
- Official references:
  - `https://vercel.com/docs/rest-api`
  - `https://vercel.com/docs/environment-variables/sensitive-environment-variables`
  - `https://vercel.com/docs/rest-api/reference/endpoints/projects/verify-project-domain`

A service creates or updates a Vercel project. Its environment map becomes an
upsert request after the project ID is known. Domain requests depend on that
same project response. Decrypted environment reads are never requested.

## Supabase

- Management REST base: `https://api.supabase.com`
- Management auth: `Authorization: Bearer ${SUPABASE_ACCESS_TOKEN}`
- Storage auth: bearer plus `apikey` from `SUPABASE_SERVICE_ROLE_KEY`
- Access probe: `GET /v1/projects`
- Official references:
  - `https://supabase.com/docs/reference/api/introduction`
  - `https://supabase.com/docs/reference/api/getting-started`
  - `https://supabase.com/docs/reference/javascript/storage-createbucket`
  - `https://supabase.com/changelog/33720-deploy-and-update-edge-functions-using-the-management-api`

Creating a project returns its `ref`. Dependent Auth, Function, and secret calls
use that ref. Storage calls derive `https://<ref>.supabase.co` and use the
project Storage API. Edge Function deploy uses `multipart/form-data`; source
files are read only during execution. Project secret upsert sends an array of
`{name,value}` objects and deletion sends an array of names.

## Neon

- REST base: `https://console.neon.tech/api/v2`
- Auth: `Authorization: Bearer ${NEON_API_KEY}`
- Access probe: `GET /projects?limit=1`
- Official references:
  - `https://api-docs.neon.tech/reference/getting-started-with-neon-api`
  - `https://api-docs.neon.tech/reference/createproject`
  - `https://api-docs.neon.tech/reference/createprojectbranch`
  - `https://api-docs.neon.tech/reference/getconnectionuri`

Project creation captures both project and default branch IDs. A later branch
uses the project ID; a role can depend on that branch and project. Returned
connection URIs and role credentials are runtime-only sensitive values.

## Railway

- GraphQL endpoint: `https://backboard.railway.com/graphql/v2`
- Account auth: `Authorization: Bearer ${RAILWAY_TOKEN}`
- Project auth: `Project-Access-Token: ${RAILWAY_PROJECT_TOKEN}`
- Access probe: account-token `me` query; the doctor intentionally requires
  `RAILWAY_TOKEN`
- Official references:
  - `https://docs.railway.com/integrations/api`
  - `https://docs.railway.com/integrations/api/api-cookbook`
  - `https://docs.railway.com/integrations/api/manage-projects`
  - `https://docs.railway.com/integrations/api/manage-variables`
  - `https://docs.railway.com/integrations/api/manage-domains`

A complete application manifest can create a project, then an environment, then a service,
then service settings, variables, and domains. The runtime substitutes project,
environment, and service IDs into GraphQL variables. Variables resolve secret
references immediately before `variableCollectionUpsert`. Project creation and
account-wide project listing require `RAILWAY_TOKEN`; project-scoped operations
may use a project token.

## Contract Invariants

1. Credential-free request plans are serializable and contain environment
   variable names, never values.
2. Provider config is internal and never appears in an API body.
3. Apply stops after the first failed mutation.
4. State advances only for successful operations.
5. Delete uses last-applied properties and reverse dependency order.
6. Observed secret-like response fields are redacted recursively.
