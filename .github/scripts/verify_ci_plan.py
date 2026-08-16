#!/usr/bin/env python3
import html
import json
import pathlib
import sys


if len(sys.argv) != 4:
    raise SystemExit("usage: verify_ci_plan.py <plan.json> <provider-requests.json> <summary.md>")


def load_object(path: pathlib.Path, label: str) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"unable to read {label}: {error}") from error
    if not isinstance(value, dict):
        raise SystemExit(f"{label} must be a JSON object")
    return value


def summary_text(value: object) -> str:
    text = str(value).replace("\r", " ").replace("\n", " ").replace("`", "'")
    return html.escape(text, quote=False)


plan_path = pathlib.Path(sys.argv[1])
requests_path = pathlib.Path(sys.argv[2])
summary_path = pathlib.Path(sys.argv[3])
plan = load_object(plan_path, "OpenManifest plan")
requests = load_object(requests_path, "provider request plan")

steps = plan.get("steps", [])
provider_requests = requests.get("requests", [])
warnings = requests.get("warnings", [])
if not isinstance(steps, list):
    raise SystemExit("OpenManifest plan steps must be an array")
if not isinstance(provider_requests, list):
    raise SystemExit("provider request plan requests must be an array")
if not isinstance(warnings, list):
    raise SystemExit("provider request plan warnings must be an array")

actions: dict[str, int] = {}
for step in steps:
    if not isinstance(step, dict):
        raise SystemExit("each OpenManifest plan step must be an object")
    action = summary_text(step.get("action", "unknown"))
    actions[action] = actions.get(action, 0) + 1

with summary_path.open("a", encoding="utf-8") as summary:
    summary.write("## OpenManifest plan\n\n")
    target_provider = summary_text(plan.get("target_provider", "unknown"))
    summary.write(f"- Target provider: `{target_provider}`\n")
    summary.write(f"- Partial plan: `{str(plan.get('partial', False)).lower()}`\n")
    summary.write(f"- Provider requests: `{len(provider_requests)}`\n")
    summary.write(f"- Executable: `{str(requests.get('executable', False)).lower()}`\n")
    if actions:
        formatted = ", ".join(f"{name}: {count}" for name, count in sorted(actions.items()))
        summary.write(f"- Actions: {formatted}\n")
    if warnings:
        summary.write("\n### Warnings\n\n")
        for warning in warnings:
            summary.write(f"- {summary_text(warning)}\n")

if requests.get("executable") is True:
    raise SystemExit(0)

print("OpenManifest provider request plan is not executable.", file=sys.stderr)
for warning in warnings:
    print(f"warning: {warning}", file=sys.stderr)
for request in provider_requests:
    if not isinstance(request, dict):
        print("warning: malformed provider request", file=sys.stderr)
        continue
    unresolved = request.get("unresolved_identifiers", [])
    if unresolved:
        if not isinstance(unresolved, list):
            print(
                f"{request.get('resource_id', 'unknown')}: malformed unresolved identifiers",
                file=sys.stderr,
            )
            continue
        print(
            f"{request.get('resource_id', 'unknown')}: "
            f"unresolved {', '.join(str(value) for value in unresolved)}",
            file=sys.stderr,
        )
raise SystemExit(1)
