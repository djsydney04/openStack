#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary="$(mktemp -d "${TMPDIR:-/tmp}/stackport-ci-test.XXXXXX")"
cleanup() {
  rm -rf "$temporary"
}
trap cleanup EXIT

origin="$temporary/origin.git"
repository="$temporary/repository"
git init --quiet --bare "$origin"
git init --quiet "$repository"
git -C "$repository" config user.name "Stackport Test"
git -C "$repository" config user.email "stackport-test@example.com"
git -C "$repository" checkout --quiet -b main
printf '%s\n' '# Test repository' >"$repository/README.md"
git -C "$repository" add README.md
git -C "$repository" commit --quiet -m "Initial commit"
git -C "$repository" remote add origin "$origin"
git -C "$repository" push --quiet -u origin main

empty_state="$temporary/empty.json"
GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" read production "$empty_state"
if [[ -e "$empty_state" ]]; then
  echo "missing remote state unexpectedly produced a file" >&2
  exit 1
fi

state="$temporary/state.json"
cat >"$state" <<'JSON'
{
  "schema_version": "stackport-state/v1",
  "provider": "vercel",
  "providers": [],
  "resources": []
}
JSON

GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" write production "$state"
loaded="$temporary/loaded.json"
GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" read production "$loaded"
cmp "$state" "$loaded"

git -C "$repository" remote set-url origin "$temporary/missing.git"
if GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" read production "$loaded"; then
  echo "state read treated a remote failure as empty state" >&2
  exit 1
fi
git -C "$repository" remote set-url origin "$origin"

python3 - "$state" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
state = json.loads(path.read_text())
state["provider"] = "railway"
path.write_text(json.dumps(state, indent=2) + "\n")
PY
GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" write production "$state"
GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" read production "$loaded"
cmp "$state" "$loaded"

if GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" write '../unsafe' "$state"; then
  echo "unsafe state key was accepted" >&2
  exit 1
fi

invalid_state="$temporary/invalid-state.json"
printf '%s\n' '{"schema_version":"unknown","resources":[]}' >"$invalid_state"
if GITHUB_WORKSPACE="$repository" \
  "$root/.github/scripts/stackport-state.sh" write invalid "$invalid_state"; then
  echo "invalid state schema was accepted" >&2
  exit 1
fi

github_env="$temporary/github-env"
mask_output="$temporary/mask-output"
STACKPORT_SECRETS_JSON='{"VERCEL_TOKEN":"token-value","MULTILINE":"line one\nline two"}' \
  GITHUB_ENV="$github_env" \
  python3 "$root/.github/scripts/export_stackport_secrets.py" >"$mask_output"
grep -q '^VERCEL_TOKEN<<STACKPORT_' "$github_env"
grep -q '^MULTILINE<<STACKPORT_' "$github_env"
grep -q '^::add-mask::token-value$' "$mask_output"

if STACKPORT_SECRETS_JSON='["not-an-object"]' GITHUB_ENV="$github_env" \
  python3 "$root/.github/scripts/export_stackport_secrets.py"; then
  echo "invalid secret JSON was accepted" >&2
  exit 1
fi

plan="$temporary/plan.json"
requests="$temporary/requests.json"
summary="$temporary/summary.md"
cat >"$plan" <<'JSON'
{"target_provider":"vercel","partial":false,"steps":[{"action":"create"}]}
JSON
cat >"$requests" <<'JSON'
{"requests":[{"resource_id":"service:web"}],"warnings":[],"executable":true}
JSON
python3 "$root/.github/scripts/verify_ci_plan.py" "$plan" "$requests" "$summary"
grep -q "Executable: \`true\`" "$summary"

cat >"$requests" <<'JSON'
{"requests":[{"resource_id":"service:web","unresolved_identifiers":["projectId"]}],"warnings":["missing project"],"executable":false}
JSON
if python3 "$root/.github/scripts/verify_ci_plan.py" "$plan" "$requests" "$summary"; then
  echo "non-executable provider plan was accepted" >&2
  exit 1
fi

printf '%s\n' '[]' >"$requests"
if python3 "$root/.github/scripts/verify_ci_plan.py" "$plan" "$requests" "$summary"; then
  echo "non-object provider plan was accepted" >&2
  exit 1
fi

echo "Stackport CI helper integration test passed."
