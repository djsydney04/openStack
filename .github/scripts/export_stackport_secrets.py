#!/usr/bin/env python3
import json
import os
import re
import sys
import uuid


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(2)


raw = os.environ.get("STACKPORT_SECRETS_JSON", "").strip()
if not raw:
    raise SystemExit(0)

environment_file = os.environ.get("GITHUB_ENV")
if not environment_file:
    fail("GITHUB_ENV is required")

try:
    secrets = json.loads(raw)
except json.JSONDecodeError as error:
    fail(f"STACKPORT_SECRETS_JSON is not valid JSON: {error}")

if not isinstance(secrets, dict):
    fail("STACKPORT_SECRETS_JSON must be a JSON object")

for name, value in secrets.items():
    if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
        fail(f"invalid environment variable name in STACKPORT_SECRETS_JSON: {name!r}")
    if not isinstance(value, str):
        fail(f"secret {name!r} must be a string")

with open(environment_file, "a", encoding="utf-8") as handle:
    for name, value in secrets.items():
        masked = value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        print(f"::add-mask::{masked}")
        delimiter = f"STACKPORT_{uuid.uuid4().hex}"
        while delimiter in value.splitlines():
            delimiter = f"STACKPORT_{uuid.uuid4().hex}"
        handle.write(f"{name}<<{delimiter}\n{value}\n{delimiter}\n")
