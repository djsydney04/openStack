from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from typing import Any


class StackportError(RuntimeError):
    pass


@dataclass
class StackportClient:
    command: str = "stackport"
    args: tuple[str, ...] = ("rpc",)

    def __post_init__(self) -> None:
        self._next_id = 1
        self._process = subprocess.Popen(
            [self.command, *self.args],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def close(self) -> None:
        if self._process.stdin:
            self._process.stdin.close()
        self._process.terminate()
        try:
            self._process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self._process.kill()
        if self._process.stdout:
            self._process.stdout.close()
        if self._process.stderr:
            self._process.stderr.close()

    def version(self) -> dict[str, Any]:
        return self.request("stackport.version", {})

    def validate(self, manifest: dict[str, Any]) -> dict[str, Any]:
        return self.request("manifest.validate", manifest)

    def validate_stack_spec(
        self, spec: dict[str, Any], target: str | None = None
    ) -> dict[str, Any]:
        params: dict[str, Any] = {"spec": spec}
        if target is not None:
            params["target"] = target
        return self.request("stackSpec.validate", params)

    def stack_spec_to_manifest(
        self, spec: dict[str, Any], target: str | None = None
    ) -> dict[str, Any]:
        params: dict[str, Any] = {"spec": spec}
        if target is not None:
            params["target"] = target
        return self.request("stackSpec.toManifest", params)

    def providers(self) -> dict[str, Any]:
        return self.request("providers.list", {})

    def provider(self, provider: str) -> dict[str, Any]:
        return self.request("providers.show", {"provider": provider})

    def provider_execution_plan(
        self, manifest: dict[str, Any], target_provider: str
    ) -> dict[str, Any]:
        return self.request(
            "providers.executionPlan",
            {"manifest": manifest, "target_provider": target_provider},
        )

    def import_vercel(self, project: dict[str, Any]) -> dict[str, Any]:
        return self.request("import.vercel", project)

    def import_supabase(self, project: dict[str, Any]) -> dict[str, Any]:
        return self.request("import.supabase", project)

    def analyze(self, manifest: dict[str, Any], target_provider: str) -> dict[str, Any]:
        return self.request(
            "portability.analyze",
            {"manifest": manifest, "target_provider": target_provider},
        )

    def plan(
        self,
        manifest: dict[str, Any],
        target_provider: str,
        scope: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {
            "manifest": manifest,
            "target_provider": target_provider,
        }
        if scope is not None:
            params["scope"] = scope
        return self.request("plan.create", params)

    def diff(self, manifest: dict[str, Any], state: dict[str, Any]) -> dict[str, Any]:
        return self.request("state.diff", {"manifest": manifest, "state": state})

    def dry_run(self, plan: dict[str, Any]) -> dict[str, Any]:
        return self.request("apply.dryRun", {"plan": plan, "dry_run": True})

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if self._process.stdin is None or self._process.stdout is None:
            raise StackportError("stackport subprocess is not connected")
        request_id = str(self._next_id)
        self._next_id += 1
        payload = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        }
        self._process.stdin.write(json.dumps(payload) + "\n")
        self._process.stdin.flush()
        line = self._process.stdout.readline()
        if not line:
            stderr = self._process.stderr.read() if self._process.stderr else ""
            raise StackportError(f"stackport subprocess produced no response: {stderr}")
        response = json.loads(line)
        if response.get("error"):
            raise StackportError(response["error"]["message"])
        return response["result"]
