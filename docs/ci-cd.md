# GitHub Actions CI/CD

OpenManifest can plan every pull request and apply the merged YAML from GitHub
Actions. The reusable workflow in this repository keeps those two trust levels
separate:

- pull requests validate and plan without provider credentials or mutations;
- pushes to the caller repository's default branch may apply;
- a GitHub Environment can require approval before the apply job starts;
- apply jobs are serialized per repository;
- secret-free state is committed to a dedicated `openmanifest-state` branch.

## Consumer Workflow

Add this workflow to the repository that owns `openmanifest.yaml`:

```yaml
name: OpenManifest

on:
  pull_request:
    paths:
      - openmanifest.yaml
      - .github/workflows/openmanifest.yml
  push:
    branches: [main]
    paths:
      - openmanifest.yaml
      - .github/workflows/openmanifest.yml
  workflow_dispatch:

jobs:
  infrastructure:
    permissions:
      contents: write
    uses: djsydney04/openStack/.github/workflows/openmanifest.yml@main
    with:
      manifest_file: openmanifest.yaml
      target: production
      state_key: production
      environment: production
```

Use a release tag or full commit SHA instead of `main` once OpenManifest publishes
versioned releases. A called workflow cannot increase its token permissions, so
the calling job must grant `contents: write` for the state branch. The plan job
downgrades its own token to `contents: read`.

The reusable workflow applies only when the caller event is a push or manual
dispatch on the caller repository's default branch. A feature-branch push can
plan but cannot apply, even if its caller workflow is misconfigured.

## Protect Production

In the application repository, open **Settings > Environments** and create the
`production` environment before enabling the workflow.

1. Add required reviewers and enable **Prevent self-review** where your GitHub
   plan supports those controls.
2. Restrict deployment branches to the default branch.
3. Add an environment secret named `OPENMANIFEST_SECRETS_JSON`.

The secret is one JSON object whose keys become environment variables only in
the apply job:

```json
{
  "VERCEL_TOKEN": "...",
  "SUPABASE_ACCESS_TOKEN": "...",
  "SUPABASE_SERVICE_ROLE_KEY": "...",
  "NEON_API_KEY": "...",
  "RAILWAY_TOKEN": "...",
  "SESSION_SECRET": "..."
}
```

Include only values needed by that application manifest. Environment secrets are released to
the job after environment protection rules pass. If an environment secret and
a caller-passed repository secret have the same name, GitHub uses the
environment secret.

As a fallback, a repository or organization secret can be passed explicitly:

```yaml
    secrets:
      OPENMANIFEST_SECRETS_JSON: ${{ secrets.OPENMANIFEST_SECRETS_JSON }}
```

Environment storage is preferred because it places provider credentials behind
the deployment gate. Never store the JSON in the YAML file, state branch, plan
artifact, or workflow source.

## What Happens

On a pull request, OpenManifest loads prior state, validates the YAML, computes the
resource plan, compiles exact provider requests, and rejects a plan with manual,
unsupported, or unresolved operations. The workflow summary shows action counts
and warnings, and a 14-day artifact contains the validation and both plans.

After merge, the push workflow repeats validation against the merged commit and
current state. It then runs `openmanifest apply --auto-approve`. This flag is safe only
because the merge review and protected environment are the approval boundary.
The apply report and generated state snapshot are retained for 30 days.

State is published even when an apply fails after some successful requests. The
Rust engine advances state only for successful provider operations, so saving
that partial state prevents the next run from blindly recreating them. The job
still fails and must be investigated before retrying.

## State Branch

The first apply creates an orphan `openmanifest-state` branch containing
`states/production.json`. Use a unique `state_key` for each independently
managed application manifest, such as `staging` and `production`.

State contains provider IDs, desired fingerprints, last-applied resource
snapshots, and secret references. It is not encrypted and must never contain
tokens or secret values. A normal fast-forward push protects against stale
writes, while workflow concurrency prevents two applies in the same repository
from running together.

The Actions token must be able to push this branch. If branch rules cover every
branch, exempt the state branch or grant the workflow the required bypass. Do
not manually edit state. Use Git history for diagnosis, then reconcile the YAML,
provider, and last known state before retrying.

## Rollback and Recovery

A rollback is a new desired-state change: revert the YAML in a pull request,
review the destroy or update plan, and merge it. Reverting a state commit alone
does not undo provider changes and can cause duplicate creates.

If a run fails:

1. inspect the apply artifact and the provider dashboard;
2. confirm whether the state branch recorded successful operations;
3. fix the YAML, credentials, or provider condition in a new pull request;
4. merge or manually dispatch the default-branch workflow after review.

If provider operations succeeded but publishing the state branch failed, do not
immediately rerun apply against old state. Download `state.json` from the apply
artifact, compare it with the provider results, and publish that generated file
with `openmanifest-state.sh write` after fixing branch access. This is recovery of
engine-produced state, not a hand edit.

Live provider mutations can cost money or delete resources. Start with a
disposable provider project and a non-production target before enabling this for
production.
