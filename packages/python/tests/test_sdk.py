import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "packages/python"))

from stackport_sdk import StackportClient


class StackportSdkTest(unittest.TestCase):
    def test_python_sdk_calls_rust_rpc_engine(self):
        client = StackportClient(
            command="cargo",
            args=("run", "-q", "-p", "stackport-cli", "--", "rpc"),
        )
        try:
            manifest = json.loads((ROOT / "fixtures/manifest.basic.json").read_text())
            version = client.version()
            self.assertEqual(version["rpc_version"], "2026-07-22")
            self.assertIs(client.validate(manifest)["valid"], True)
            plan = client.plan(
                manifest,
                "render",
                {"include_resources": ["web"], "exclude_resources": []},
            )
            self.assertIs(plan["partial"], True)
        finally:
            client.close()


if __name__ == "__main__":
    unittest.main()
