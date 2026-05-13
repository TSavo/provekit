# LSP Plugin Protocol

ProvekIt editor plugins speak a small line-delimited JSON protocol to kit-local helpers. The editor owns Language Server Protocol wiring; the helper owns ProvekIt parsing, lifting, and handshake checks.

## Transport

- One JSON message per line on stdin/stdout.
- Messages are UTF-8.
- Each message has an `id`, `method`, and optional `params`.
- Responses carry the same `id` with either `result` or `error`.

## Required Methods

| Method | Purpose |
|---|---|
| `initialize` | Return helper version, protocol catalog CID, supported source surfaces, and supported diagnostics. |
| `parse` | Parse one document snapshot and return lifted claims, diagnostics, and handshake results. |
| `shutdown` | Flush caches and exit cleanly. |

## `parse` Inputs

`parse` receives:

- document URI;
- language or kit key;
- full source text or a content hash plus cached text agreement;
- editor version metadata;
- optional project roots and policy paths.

The helper should cache by source-text hash, AST hash, and `(postCid, preCid)` handshake pair.

## Diagnostics

Diagnostics use source `provekit`, stable codes from [error codes](error-codes.md), and source ranges from the editor's document snapshot. The helper should report the protocol catalog CID it used so editor logs can diagnose catalog mismatch.

## Read Next

- [Writing an LSP plugin](../contributing/writing-an-LSP-plugin.md).
- [IDE integration overview](../how-to/ide-integration/overview.md).
- [Debugging a failed handshake](../how-to/debugging-a-failed-handshake.md).
