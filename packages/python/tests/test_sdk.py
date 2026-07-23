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
            self.assertEqual(version["rpc_version"], "2026-07-23")
            self.assertIs(client.validate(manifest)["valid"], True)
            plan = client.plan(
                manifest,
                "render",
                {"include_resources": ["web"], "exclude_resources": []},
            )
            self.assertIs(plan["partial"], True)

            stack_spec = {
                "version": "stackport/app/v1alpha1",
                "app": {"name": "sdk-stack"},
                "targets": {
                    "preview": {"provider": "vercel"},
                    "production": {"provider": "railway"},
                },
                "services": {
                    "web": {
                        "build": {"framework": "nextjs", "command": "npm run build"},
                        "env": {"DATABASE_URL": {"secret": "DATABASE_URL"}},
                    }
                },
                "databases": {
                    "primary": {"provider": "neon", "engine": "postgres"},
                },
                "secrets": {
                    "DATABASE_URL": {"from": "neon:primary:DATABASE_URL"},
                },
            }
            self.assertIs(
                client.validate_stack_spec(stack_spec, "production")["valid"],
                True,
            )
            stack_manifest = client.stack_spec_to_manifest(stack_spec, "production")
            web = next(
                resource
                for resource in stack_manifest["resources"]
                if resource["id"] == "service:web"
            )
            self.assertEqual(web["provider"], "railway")
            railway = client.provider("railway")
            self.assertIs(railway["secrets"]["stores_plaintext_in_state"], False)
            probe_plan = client.provider_probe_plan("railway")
            self.assertEqual(len(probe_plan["requests"]), 1)
            provider_plan = client.provider_execution_plan(stack_manifest, "railway")
            self.assertEqual(len(provider_plan["steps"]), len(stack_manifest["resources"]))
            web_step = next(
                step
                for step in provider_plan["steps"]
                if step["resource_id"] == "service:web"
            )
            self.assertEqual(
                web_step["api_operation"]["graphql_operation"],
                "serviceCreate",
            )
            read_plan = client.provider_read_plan(stack_manifest, "railway")
            self.assertGreater(len(read_plan["requests"]), 0)
            import_plan = client.provider_import_plan(stack_manifest, "railway")
            self.assertGreater(len(import_plan["requests"]), 0)
        finally:
            client.close()


if __name__ == "__main__":
    unittest.main()
