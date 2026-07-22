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
  importVercel(project: JsonValue): Promise<Manifest>;
  importSupabase(project: JsonValue): Promise<Manifest>;
  analyze(manifest: Manifest, targetProvider: string): Promise<JsonValue>;
  plan(manifest: Manifest, targetProvider: string, scope?: MigrationScope): Promise<JsonValue>;
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
    importVercel: (project) => request<Manifest>("import.vercel", project),
    importSupabase: (project) => request<Manifest>("import.supabase", project),
    analyze: (manifest, targetProvider) =>
      request<JsonValue>("portability.analyze", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
      }),
    plan: (manifest, targetProvider, scope) =>
      request<JsonValue>("plan.create", {
        manifest: manifest as unknown as JsonValue,
        target_provider: targetProvider,
        ...(scope ? { scope: scope as unknown as JsonValue } : {}),
      }),
    diff: (manifest, state) =>
      request<JsonValue>("state.diff", {
        manifest: manifest as unknown as JsonValue,
        state,
      }),
    dryRun: (plan) => request<JsonValue>("apply.dryRun", { plan, dry_run: true }),
  };
}
