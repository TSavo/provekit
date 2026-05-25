# LSP Plugin Protocol

This page is the short operational reference for kit-local editor helpers.
The shared editor protocol is defined in
[`2026-05-25-lsp-shared-protocol.md`](../../protocol/specs/2026-05-25-lsp-shared-protocol.md).

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
| `analyzeDocument` | Analyze one document snapshot and return normalized entries, diagnostics, statuses, and optional project state. |
| `shutdown` | Flush caches and exit cleanly. |

Legacy helpers may still expose `parse` during migration. New helpers should
target `analyzeDocument` from the shared protocol.

## `analyzeDocument` Inputs

`analyzeDocument` receives:

- document URI;
- language or kit key;
- full source text or a content hash plus cached text agreement;
- editor version metadata;
- optional project roots and policy paths.

The helper should cache by source-text hash, AST hash, protocol catalog CID, and policy CIDs.

## Diagnostics

Diagnostics use source `provekit`, stable codes, and source ranges from the
editor's document snapshot. The `provekit.lsp.*` code authority is
[`2026-05-25-lsp-shared-protocol.md` §6](../../protocol/specs/2026-05-25-lsp-shared-protocol.md#6-diagnostics);
[error codes](error-codes.md) covers the broader cross-component
`PROVEKIT_*` handles. The helper should report the protocol catalog CID it used
so editor logs can diagnose catalog mismatch.

## Read Next

- [Writing an LSP plugin](../contributing/writing-an-LSP-plugin.md).
- [IDE integration overview](../how-to/ide-integration/overview.md).
- [Debugging a failed handshake](../how-to/debugging-a-failed-handshake.md).
