# Writing an LSP plugin

Five LSP plugins ship today (Rust, Python, Zig, Ruby, C#). They all implement the same minimal protocol: NDJSON over stdio, three methods (`initialize`, `parse`, `shutdown`).

The LSP plugin is what gives users red squigglies in their editor. It is the most visible product surface. If a kit ships without an LSP, users get no in-editor feedback even when verification works.

## Scope

A Sugar LSP plugin is **not** a full Language Server Protocol implementation. It is a plugin that the editor's existing language server delegates to. The host language server (rust-analyzer, pylsp, gopls, etc.) handles the bulk of LSP work; the Sugar plugin handles the contract verification slice.

This means:

- You don't implement `textDocument/completion`, `textDocument/hover`, `textDocument/definition`, or any of the other heavy-duty LSP methods.
- You implement the three methods needed for Sugar's contract-verification feedback.
- You ride on top of the host language server's existing infrastructure.

The plugin's deliverable is: "given a parse of the user's code, return the contract violations as LSP diagnostics."

## The protocol

NDJSON (newline-delimited JSON) over stdio. The plugin reads requests from stdin, writes responses to stdout, both line-delimited.

### `initialize`

Request:

```json
{"method": "initialize", "id": 1, "params": {
  "rootUri": "file:///path/to/project",
  "kit": "rust",
  "capabilities": {}
}}
```

Response:

```json
{"id": 1, "result": {
  "supported_kit": "rust",
  "supported_protocol_cid": "blake3-512:b0f2030d...",
  "capabilities": ["parse", "lift", "diagnose"]
}}
```

The plugin reports the kit it implements and the protocol catalog CID it conforms to. Editor integration uses this to gate features (e.g., "show squigglies only if the plugin's protocol CID matches the install's").

### `parse`

Request:

```json
{"method": "parse", "id": 2, "params": {
  "uri": "file:///path/to/file.rs",
  "text": "fn add_one(x: i32) -> i32 {\n    x + 1\n}\n",
  "version": 5
}}
```

Response:

```json
{"id": 2, "result": {
  "diagnostics": [
    {
      "range": {"start": {"line": 0, "character": 12}, "end": {"line": 0, "character": 14}},
      "severity": "error",
      "code": "SUGAR_E001",
      "message": "contract precondition unmet: x must be >= 0",
      "source": "sugar"
    }
  ],
  "lifted_contracts": 0,
  "discharged_at_tier_1": 0,
  "discharged_at_tier_2": 0,
  "requires_tier_3": 0
}}
```

The plugin parses the file's text, runs lift adapters where applicable, and returns LSP-shaped diagnostics. Diagnostics use 0-indexed line/character positions per LSP convention.

### `shutdown`

Request:

```json
{"method": "shutdown", "id": 3}
```

Response:

```json
{"id": 3, "result": null}
```

The plugin flushes any in-progress work, releases resources, and exits cleanly.

## Editor integration

The editor doesn't talk to the plugin directly. The host language server (or a thin extension) does. Three integration patterns:

### Pattern A: dedicated extension per editor

Examples: VSCode extension, JetBrains plugin, Neovim plugin. The extension spawns the LSP plugin as a subprocess and forwards diagnostics into the editor's diagnostic display.

This is the typical path. Extensions exist (or are planned) per shipping kit.

### Pattern B: ride on host language server

Some host language servers support plugin protocols (rust-analyzer's procedural macro server, pylsp's plugin system). The Sugar plugin can register as a host language server plugin instead of a standalone LSP.

This is more efficient (no separate process) but couples to the host language server's plugin API.

### Pattern C: shell out from the linter

Some editors run linters as standalone processes (eslint, ruff, golangci-lint). The Sugar plugin can integrate as a custom linter rule that shells out and returns diagnostics in the linter's output format.

This is simpler but limited to editors with linter integration.

## The lift-and-diagnose loop

What the plugin actually does on `parse`:

1. **Parse the source file** using the host language's compiler API (or a vendored parser).
2. **Walk the AST** for annotations recognized by the kit's lift adapters.
3. **Canonicalize** each recognized annotation to canonical IR.
4. **Hash and look up** in the local memento cache: does a contract memento with this CID exist?
5. **For each call site**, run the three-tier handshake (most call sites discharge at Tier 1 against cached mementos).
6. **Collect violations** as LSP diagnostics.

This is essentially `sugar prove --file` per file, scoped to in-flight edits.

## Cache hygiene

The plugin runs frequently (on every save, on every keystroke depending on editor settings). Don't re-canonicalize from scratch every invocation:

- **Cache canonical IR by source-text hash.** If the file's text hasn't changed, return cached canonical IR.
- **Cache lift-adapter output by AST-hash.** If the AST is the same, the lift output is the same.
- **Cache handshake results.** Once a `(post, pre)` pair is discharged at Tier 2, subsequent encounters at the same pair stay discharged.

The plugin's responsiveness is what users perceive. A 50ms response is fine; a 5-second response is unusable.

## Diagnostic granularity

LSP diagnostics carry severity, source, code, message, and range. Sugar diagnostics use:

- **Severity**: `error` (contract violated), `warning` (contract requires Tier 3 fallback), `information` (lifted contract), `hint` (suggestion).
- **Source**: always `"sugar"`. Editors filter by source for "show only Sugar issues."
- **Code**: stable error code, e.g., `SUGAR_E001`. See [`docs/reference/error-codes.md`](../reference/error-codes.md).
- **Message**: human-readable, includes the contract that was violated. Include the source-library annotation that gave rise to the contract, so users see "your `@Min(0)` is violated" rather than "atomic ge violated."
- **Range**: the source range the violation applies to.

Quick fixes (LSP `codeAction`) are valuable for common patterns:

- "Add `@NotNull` to align with caller's contract."
- "Bridge `my_fn` to `ref-parseInt-v1`."
- "Lift this annotation library by adding `sugar-lift-bean-validation`."

Quick fix protocol is more involved than `parse`; ship `parse` first, add code actions later.

## Performance targets

- `initialize`: < 100ms.
- `parse` on a 1000-line file with 50 annotations: < 200ms. Hot path; cache aggressively.
- Memory: plugin process should stay under 200MB for typical projects.

## Testing the plugin

Three layers:

1. **Unit tests** for protocol marshaling: feed JSON over stdin, expect JSON over stdout.
2. **Integration tests** that drive the plugin against fixture files; assert diagnostics match expected.
3. **End-to-end editor tests** that boot the editor with the plugin loaded and assert squigglies appear at expected positions. These are slow; run on a separate cadence.

## Shipping checklist

- [ ] `initialize`, `parse`, `shutdown` all implemented.
- [ ] NDJSON parsing handles partial reads, large messages, embedded newlines in strings.
- [ ] Caches canonical IR, lift output, handshake results.
- [ ] Reports protocol CID matching the install.
- [ ] At least one editor integration (VSCode extension, Neovim plugin, etc.).
- [ ] Tutorial documentation in `docs/how-to/ide-integration/` (per editor).
- [ ] Performance verified: 1000-line file < 200ms.

## When this is done

Users who install the kit and the editor extension see red squigglies on contract violations. The product surface that differentiates "this verifies my code" from "this prints discharge fractions in CI" exists.

## Read next

- [docs/how-to/ide-integration/](../how-to/ide-integration/): per-editor wire-up for shipping plugins.
- [docs/reference/lsp-plugin-protocol.md](../reference/lsp-plugin-protocol.md) (when written): the NDJSON protocol reference.
