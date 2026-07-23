#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: stackport-state.sh read <state-key> <output> | write <state-key> <input>" >&2
  exit 2
}

command_name="${1:-}"
state_key="${2:-}"
state_file="${3:-}"
[[ $# -eq 3 && -n "$command_name" && -n "$state_key" && -n "$state_file" ]] || usage

if [[ ! "$state_key" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
  echo "invalid Stackport state key: $state_key" >&2
  exit 2
fi

state_branch="${STACKPORT_STATE_BRANCH:-stackport-state}"
if [[ ! "$state_branch" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] \
  || [[ "$state_branch" == *".."* ]] \
  || [[ "$state_branch" == *"//"* ]] \
  || ! git check-ref-format "refs/heads/$state_branch" >/dev/null 2>&1; then
  echo "invalid Stackport state branch: $state_branch" >&2
  exit 2
fi

repository="${GITHUB_WORKSPACE:-$(git rev-parse --show-toplevel)}"
remote_ref="refs/remotes/origin/$state_branch"
state_path="states/$state_key.json"

fetch_state_branch() {
  git -C "$repository" update-ref -d "$remote_ref" 2>/dev/null || true

  set +e
  git -C "$repository" ls-remote --quiet --exit-code --heads origin \
    "refs/heads/$state_branch" >/dev/null 2>&1
  remote_status=$?
  set -e

  case "$remote_status" in
    0) ;;
    2) return 1 ;;
    *)
      echo "Unable to inspect remote Stackport state branch '$state_branch'." >&2
      return 3
      ;;
  esac

  if ! git -C "$repository" fetch --quiet --no-tags origin \
    "refs/heads/$state_branch:$remote_ref"; then
    echo "Unable to fetch remote Stackport state branch '$state_branch'." >&2
    return 3
  fi
}

read_state() {
  rm -f "$state_file" "$state_file.tmp"
  mkdir -p "$(dirname "$state_file")"
  if fetch_state_branch; then
    :
  else
    fetch_status=$?
    if [[ "$fetch_status" -eq 1 ]]; then
      echo "No remote Stackport state branch found; planning from empty state."
      return 0
    fi
    return "$fetch_status"
  fi
  if ! git -C "$repository" cat-file -e "$remote_ref:$state_path" 2>/dev/null; then
    echo "No state found for key '$state_key'; planning from empty state."
    return 0
  fi
  git -C "$repository" show "$remote_ref:$state_path" >"$state_file.tmp"
  mv "$state_file.tmp" "$state_file"
  echo "Loaded Stackport state '$state_key' from '$state_branch'."
}

validate_state() {
  python3 - "$state_file" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
with path.open(encoding="utf-8") as handle:
    state = json.load(handle)
if state.get("schema_version") != "stackport-state/v1":
    raise SystemExit("state schema_version must be stackport-state/v1")
if not isinstance(state.get("resources"), list):
    raise SystemExit("state resources must be an array")
PY
}

write_state() {
  [[ -s "$state_file" ]] || {
    echo "Stackport state file is missing or empty: $state_file" >&2
    exit 2
  }
  validate_state

  worktree="$(mktemp -d "${RUNNER_TEMP:-/tmp}/stackport-state.XXXXXX")"
  cleanup() {
    git -C "$repository" worktree remove --force "$worktree" >/dev/null 2>&1 || true
    rm -rf "$worktree"
  }
  trap cleanup EXIT

  if fetch_state_branch; then
    git -C "$repository" worktree add --quiet --detach "$worktree" "$remote_ref"
  else
    fetch_status=$?
    if [[ "$fetch_status" -ne 1 ]]; then
      return "$fetch_status"
    fi
    git -C "$repository" worktree add --quiet --detach "$worktree" HEAD
    git -C "$worktree" checkout --quiet --orphan "$state_branch"
    git -C "$worktree" rm -rf . >/dev/null
    printf '%s\n' \
      '# Stackport State' \
      '' \
      'This branch is managed by Stackport CI. State files contain provider identifiers and secret references, not secret values.' \
      >"$worktree/README.md"
  fi

  mkdir -p "$worktree/$(dirname "$state_path")"
  cp "$state_file" "$worktree/$state_path"
  git -C "$worktree" add README.md "$state_path" 2>/dev/null \
    || git -C "$worktree" add "$state_path"
  if git -C "$worktree" diff --cached --quiet; then
    echo "Stackport state '$state_key' is unchanged."
    return 0
  fi

  git -C "$worktree" config user.name "stackport-ci"
  git -C "$worktree" config user.email "stackport-ci@users.noreply.github.com"
  git -C "$worktree" commit --quiet -m "Update Stackport state: $state_key"
  git -C "$worktree" push --quiet origin "HEAD:refs/heads/$state_branch"
  echo "Published Stackport state '$state_key' to '$state_branch'."
}

case "$command_name" in
  read) read_state ;;
  write) write_state ;;
  *) usage ;;
esac
