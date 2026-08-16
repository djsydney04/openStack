import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createOpenManifestClient, type Manifest } from "../src/index.ts";

const manifestPath = new URL("../../../fixtures/manifest.basic.json", import.meta.url);

test("typescript sdk calls the rust rpc engine", async () => {
  const client = createOpenManifestClient({
    command: "cargo",
    args: ["run", "-q", "-p", "openmanifest-cli", "--bin", "openmanifest", "--", "rpc"],
  });
  try {
    const manifest = JSON.parse(await readFile(manifestPath, "utf8")) as Manifest;
    const version = await client.version();
    assert.equal(version.rpc_version, "2026-08-14");

    const validation = await client.validate(manifest);
    assert.equal((validation as { valid: boolean }).valid, true);

    const plan = await client.plan(manifest, "render", { include_resources: ["web"] });
    assert.equal((plan as { partial: boolean }).partial, true);

    const appManifest = {
      version: "openmanifest/app/v1alpha1",
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
    const appValidation = await client.validateAppManifest(appManifest, "production");
    assert.equal((appValidation as { valid: boolean }).valid, true);
    const resourceManifest = await client.compileAppManifest(appManifest, "production");
    assert.equal(resourceManifest.resources.find((resource) => resource.id === "service:web")?.provider, "railway");
    const railway = await client.provider("railway");
    assert.equal((railway as { secrets: { stores_plaintext_in_state: boolean } }).secrets.stores_plaintext_in_state, false);
    const probePlan = await client.providerProbePlan("railway");
    assert.equal((probePlan as { requests: unknown[] }).requests.length, 1);
    const providerPlan = await client.providerExecutionPlan(resourceManifest, "railway");
    assert.equal((providerPlan as { steps: unknown[] }).steps.length, resourceManifest.resources.length);
    assert.equal(
      (providerPlan as { steps: Array<{ resource_id: string; api_operation?: { graphql_operation?: string } }> }).steps.find(
        (step) => step.resource_id === "service:web",
      )?.api_operation?.graphql_operation,
      "serviceCreate",
    );
    const readPlan = await client.providerReadPlan(resourceManifest, "railway");
    assert.ok((readPlan as { requests: unknown[] }).requests.length > 0);
    const importPlan = await client.providerImportPlan(resourceManifest, "railway");
    assert.ok((importPlan as { requests: unknown[] }).requests.length > 0);
  } finally {
    client.close();
  }
});
