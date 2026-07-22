# Stackport

Stackport is a provider-neutral portability toolkit for application stacks.

The repository is organized around one implementation of migration logic:

- `crates/stackport-core`: Rust core engine and native Rust SDK surface.
- `crates/stackport-cli`: CLI and versioned subprocess JSON-RPC server.
- `packages/typescript`: TypeScript SDK that calls the CLI RPC protocol.
- `packages/python`: Python SDK that calls the CLI RPC protocol.

The TypeScript and Python SDKs intentionally do not reimplement Stackport's
graph, validation, analysis, planning, diffing, or apply logic. They launch the
Rust CLI as a subprocess and exchange versioned JSON-RPC messages.

## Quick Start

```sh
cargo run -p stackport-cli -- validate fixtures/manifest.basic.json
cargo run -p stackport-cli -- import vercel fixtures/vercel.project.json
cargo run -p stackport-cli -- analyze fixtures/manifest.basic.json --target render
cargo run -p stackport-cli -- plan fixtures/manifest.basic.json --target render
```

## RPC

Start a line-delimited JSON-RPC subprocess:

```sh
cargo run -p stackport-cli -- rpc
```

Each line is a request:

```json
{"jsonrpc":"2.0","id":"1","method":"stackport.version","params":{}}
```

Each response includes either `result` or `error`.

## Security Model

Stackport manifests reference secrets by name. Importers reject likely secret
values and preserve only secret references. Apply is dry-run only until a caller
provides a concrete provider adapter from the Rust SDK.

