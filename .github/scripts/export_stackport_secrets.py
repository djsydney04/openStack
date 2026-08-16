#!/usr/bin/env python3
import os
import pathlib
import runpy


os.environ.setdefault(
    "OPENMANIFEST_SECRETS_JSON", os.environ.get("STACKPORT_SECRETS_JSON", "")
)
runpy.run_path(
    pathlib.Path(__file__).with_name("export_openmanifest_secrets.py"),
    run_name="__main__",
)
