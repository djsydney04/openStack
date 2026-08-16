# YAML Reference

The authoring schema is `openmanifest/app/v1alpha1`. Unknown provider properties
are preserved as JSON-compatible values, while the common service and database
fields stay portable.

## Top Level

| Field | Required | Meaning |
| --- | --- | --- |
| `version` | yes | Must be `openmanifest/app/v1alpha1` |
| `app` | yes | Application name, description, and tags |
| `targets` | for target commands | Named provider destinations |
| `services` | no | Web services with source, build, run, env, and deploy settings |
| `databases` | no | Managed Postgres resources |
| `secrets` | no | Names and runtime secret sources, never values |
| `domains` | no | Named custom domains attached to services |
| `resources` | no | Typed provider resources not covered by shorthand sections |

## Targets

```yaml
targets:
  production:
    provider: railway
    region: us-west1
    environment: production
    config:
      project_id: existing-project-id
    api_url: http://127.0.0.1:8080
```

`config` contains non-secret provider identifiers. Sensitive-looking keys are
rejected. `api_url` overrides the provider API base and is intended for local
testing or compatible gateways. Vercel accepts `team_id` for team-scoped API
calls. Supabase project creation requires `organization_slug`. Railway can use
`project_id` and `environment_id` when those resources already exist outside
the current create plan.

## Services

```yaml
services:
  web:
    provider: railway
    source:
      path: .
      repository: owner/repository
    build:
      framework: nextjs
      command: npm run build
      output: .next
    run:
      command: npm start
    depends_on:
      - database:primary
      - project:app
    env:
      NODE_ENV: production
      SESSION_SECRET:
        secret: SESSION_SECRET
    deploy:
      replicas: 2
      health_check: /health
      domains:
        - app.example.com
    config: {}
```

The generated ID is `service:<name>`. A dependency containing `:` is used as
written; a shorthand dependency such as `primary` becomes `database:primary`.
Each deploy domain becomes a separate `domain:*` resource that depends on the
service.

## Databases

```yaml
databases:
  primary:
    provider: neon
    engine: postgres
    version: "16"
    plan: free
    region: aws-us-east-2
    password_secret: DATABASE_PASSWORD
    config: {}
```

The generated ID is `database:<name>`. `password_secret` becomes a runtime
`${secret:...}` reference and is never written into state.

## Secrets

```yaml
secrets:
  SESSION_SECRET:
    from: env:SESSION_SECRET
    description: Session signing key
```

`from` is a resolver reference, not a value. Current execution supports `env:*`
directly and provider-produced references such as `neon:primary:DATABASE_URL`
when an earlier request returns that runtime value.

## Generic Resources

```yaml
resources:
  bucket:avatars:
    kind: storage_bucket
    provider: supabase
    capabilities:
      - object_storage
    depends_on:
      - database:primary
    properties:
      name: avatars
      public: false
    config: {}
```

If a key already contains `:`, it becomes the exact resource ID. Otherwise the
kind prefix is added. `config` is internal routing data; it is available to URL
and identifier compilation but is removed from provider request bodies.

Resource kinds: `project`, `environment`, `web_service`, `static_site`,
`build`, `deploy_hook`, `database`, `database_branch`, `database_role`,
`connection_string`, `auth`, `storage_bucket`, `function`, `secret`, and
`domain`.

Capabilities: `build`, `serverless_functions`, `edge_functions`, `postgres`,
`auth`, `object_storage`, `secrets`, `custom_domains`, and `cron`.

## Commands

```sh
openmanifest validate openmanifest.yaml --target production
openmanifest compile openmanifest.yaml --target production
openmanifest plan openmanifest.yaml --target production
openmanifest plan openmanifest.yaml --target production --provider-requests
openmanifest apply openmanifest.yaml --target production
openmanifest apply openmanifest.yaml --target production --auto-approve
```

Unapproved apply prints the request plan and performs no network calls.
