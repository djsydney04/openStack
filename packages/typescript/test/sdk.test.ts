import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createStackportClient, type Manifest } from "../src/index.ts";

const manifestPath = new URL("../../../fixtures/manifest.basic.json", import.meta.url);

test("typescript sdk calls the rust rpc engine", async () => {
  const client = createStackportClient({
    command: "cargo",
    args: ["run", "-q", "-p", "stackport-cli", "--", "rpc"],
  });
  try {
    const manifest = JSON.parse(await readFile(manifestPath, "utf8")) as Manifest;
    const version = await client.version();
    assert.equal(version.rpc_version, "2026-07-22");

    const validation = await client.validate(manifest);
    assert.equal((validation as { valid: boolean }).valid, true);

    const plan = await client.plan(manifest, "render", { include_resources: ["web"] });
    assert.equal((plan as { partial: boolean }).partial, true);
  } finally {
    client.close();
  }
});
