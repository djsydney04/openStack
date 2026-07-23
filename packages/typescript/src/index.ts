import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface StackportClientOptions {
  command?: string;
  args?: string[];
}

export interface JsonRpcError {
  code: number;
  message: string;
}

export interface JsonRpcResponse<T> {
  jsonrpc: "2.0";
  id: string;
  result?: T;
  error?: JsonRpcError;
}

export interface Manifest {
  schema_version: string;
  application: {
    name: string;
    description?: string | null;
    tags?: string[];
  };
  resources: Resource[];
  variables?: Record<string, { description?: string | null; secret_ref?: string | null; default?: string | null }>;
}

export interface Resource {
  id: string;
  kind: string;
  provider?: string | null;
  capabilities?: string[];
  depends_on?: string[];
  properties?: JsonValue;
}

export interface MigrationScope {
  include_resources?: string[];
  exclude_resources?: string[];
}

export interface StackportVersion {
  rpc_version: string;
  engine: string;
}

export interface StackportClient {
  close(): void;
  version(): Promise<StackportVersion>;
  validate(manifest: Manifest): Promise<JsonValue>;
  validateStackSpec(spec: JsonValue, target?: string): Promise<JsonValue>;
  stackSpecToManifest(spec: JsonValue, target?: string): Promise<Manifest>;
  providers(): Promise<JsonValue>;
  provider(provider: string): Promise<JsonValue>;
  providerProbePlan(provider?: string, contexts?: JsonValue): Promise<JsonValue>;
  probeProviderAccess(provider?: string, contexts?: JsonValue): Promise<JsonValue>;
  providerExecutionPlan(manifest: Manifest, targetProvider: string, state?: JsonValue): Promise<JsonValue>;
  providerRequestPlan(
    manifest: Manifest,
    targetProvider: string,
    contexts?: JsonValue,
    scope?: MigrationScope,
    state?: JsonValue,
  ): Promise<JsonValue>;
  providerReadPlan(
    manifest: Manifest,
    targetProvider: string,
    contexts?: JsonValue,
    state?: JsonValue,
  ): Promise<JsonValue>;
  providerImportPlan(
    manifest: Manifest,
    targetProvider: string,
    contexts?: JsonValue,
  ): Promise<JsonValue>;
  readProviderState(
    manifest: Manifest,
    targetProvider: string,
    contexts: JsonValue,
    state?: JsonValue,
  ): Promise<JsonValue>;
  importProviderState(
    manifest: Manifest,
    targetProvider: string,
    contexts: JsonValue,
  ): Promise<JsonValue>;
  refreshProviderPlan(
    manifest: Manifest,
    targetProvider: string,
    contexts: JsonValue,
    state: JsonValue,
    scope?: MigrationScope,
  ): Promise<JsonValue>;
  applyProviderPlan(
    manifest: Manifest,
    targetProvider: string,
    contexts: JsonValue,
    confirm: boolean,
    scope?: MigrationScope,
    state?: JsonValue,
  ): Promise<JsonValue>;
  importVercel(project: JsonValue): Promise<Manifest>;
  importSupabase(project: JsonValue): Promise<Manifest>;
  analyze(manifest: Manifest, targetProvider: string): Promise<JsonValue>;
  plan(manifest: Manifest, targetProvider: string, scope?: MigrationScope, state?: JsonValue): Promise<JsonValue>;
  diff(manifest: Manifest, state: JsonValue): Promise<JsonValue>;
  dryRun(plan: JsonValue): Promise<JsonValue>;
}

export function createStackportClient(options: StackportClientOptions = {}): StackportClient {
  const command = options.command ?? "stackport";
  const args = options.args ?? ["rpc"];
  const child = spawn(command, args, {
    stdio: ["pipe", "pipe", "pipe"],
  });
  const responses = new Map<string, { resolve: (value: unknown) => void; reject: (reason: Error) => void }>();
  let nextId = 1;

  const reader = createInterface({ input: child.stdout });
  reader.on("line", (line) => {
    const response = JSON.parse(line) as JsonRpcResponse<unknown>;
    const pending = responses.get(response.id);
    if (!pending) return;
    responses.delete(response.id);
    if (response.error) {
      pending.reject(new Error(response.error.message));
    } else {
      pending.resolve(response.result);
    }
  });

  child.on("exit", (code) => {
    for (const [, pending] of responses) {
      pending.reject(new Error(`stackport subprocess exited with code ${code}`));
    }
    responses.clear();
  });

  function request<T>(method: string, params: JsonValue): Promise<T> {
    if (!child.stdin.writable) {
      return Promise.reject(new Error("stackport subprocess stdin is closed"));
    }
    const id = String(nextId++);
    const payload = JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n";
    return new Promise<T>((resolve, reject) => {
      responses.set(id, { resolve: resolve as (value: unknown) => void, reject });
      child.stdin.write(payload);
    });
  }

  return {
    close() {
      child.stdin.end();
      child.kill();
    },
    version: () => request<StackportVersion>("stackport.version", {}),
    validate: (manifest) => request<JsonValue>("manifest.validate", manifest as unknown as JsonValue),
    validateStackSpec: (spec, target) =>
      request<JsonValue>("stackSpec.validate", {
        spec,
        ...(target ? { target } : {}),
      }),
    stackSpecToManifest: (spec, target) =>
      request<Manifest>("stackSpec.toManifest", {
        spec,
        ...(target ? { target } : {}),
      }),
    providers: () => request<JsonValue>("providers.list", {}),
    provider: (provider) => request<JsonValue>("providers.show", { provider }),
    providerProbePlan: (provider, contexts) =>
      request<JsonValue>("providers.probePlan", {
        ...(provider ? { provider } : {}),
        ...(contexts ? { contexts } : {}),
      }),
    probeProviderAccess: (provider, contexts) =>
      request<JsonValue>("providers.probe", {
        ...(provider ? { provider } : {}),
        ...(contexts ? { contexts } : {}),
      }),
    providerExecutionPlan: (manifest, targetProvider, state) =>
      request<JsonValue>("providers.executionPlan", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        ...(state ? { state } : {}),
      }),
    providerRequestPlan: (manifest, targetProvider, contexts, scope, state) =>
      request<JsonValue>("providers.requestPlan", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        ...(contexts ? { contexts } : {}),
        ...(scope ? { scope: scope as unknown as JsonValue } : {}),
        ...(state ? { state } : {}),
      }),
    providerReadPlan: (manifest, targetProvider, contexts, state) =>
      request<JsonValue>("providers.readPlan", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        ...(contexts ? { contexts } : {}),
        ...(state ? { state } : {}),
      }),
    providerImportPlan: (manifest, targetProvider, contexts) =>
      request<JsonValue>("providers.importPlan", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        ...(contexts ? { contexts } : {}),
      }),
    readProviderState: (manifest, targetProvider, contexts, state) =>
      request<JsonValue>("providers.read", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        contexts,
        ...(state ? { state } : {}),
      }),
    importProviderState: (manifest, targetProvider, contexts) =>
      request<JsonValue>("providers.import", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        contexts,
      }),
    refreshProviderPlan: (manifest, targetProvider, contexts, state, scope) =>
      request<JsonValue>("providers.refreshPlan", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        contexts,
        state,
        ...(scope ? { scope: scope as unknown as JsonValue } : {}),
      }),
    applyProviderPlan: (manifest, targetProvider, contexts, confirm, scope, state) =>
      request<JsonValue>("providers.apply", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        contexts,
        confirm,
        ...(scope ? { scope: scope as unknown as JsonValue } : {}),
        ...(state ? { state } : {}),
      }),
    importVercel: (project) => request<Manifest>("import.vercel", project),
    importSupabase: (project) => request<Manifest>("import.supabase", project),
    analyze: (manifest, targetProvider) =>
      request<JsonValue>("portability.analyze", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
      }),
    plan: (manifest, targetProvider, scope, state) =>
      request<JsonValue>("plan.create", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        ...(scope ? { scope: scope as unknown as JsonValue } : {}),
        ...(state ? { state } : {}),
      }),
    diff: (manifest, state) =>
      request<JsonValue>("state.diff", {
        manifest: manifest as unknown as JsonValue,
        state,
      }),
    dryRun: (plan) => request<JsonValue>("apply.dryRun", { plan, dry_run: true }),
  };
}
