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

    const stackSpec = {
      version: "stackport/app/v1alpha1",
      app: { name: "sdk-stack" },
      targets: {
        preview: { provider: "vercel" },
        production: { provider: "railway" },
      },
      services: {
        web: {
          build: { framework: "nextjs", command: "npm run build" },
          env: {
            DATABASE_URL: { secret: "DATABASE_URL" },
          },
        },
      },
      databases: {
        primary: { provider: "neon", engine: "postgres" },
      },
      secrets: {
        DATABASE_URL: { from: "neon:primary:DATABASE_URL" },
      },
    };
    const stackValidation = await client.validateStackSpec(stackSpec, "production");
    assert.equal((stackValidation as { valid: boolean }).valid, true);
    const stackManifest = await client.stackSpecToManifest(stackSpec, "production");
    assert.equal(stackManifest.resources.find((resource) => resource.id === "service:web")?.provider, "railway");
  } finally {
    client.close();
  }
});
