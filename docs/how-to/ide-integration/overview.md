# IDE integration: overview

Sugar LSP plugins shipping today:

| Kit | Plugin | Status |
|---|---|---|
| Rust | `provekit-lsp-rust` | shipping in the current v1.6.2 tree |
| Python | `provekit-lsp-py` | shipping in the current v1.6.2 tree |
| Zig | `provekit-lift-zig --rpc` | shipping in the current v1.6.2 tree |
| Ruby | `provekit-lsp-ruby` | shipping in the current v1.6.2 tree |
| C# | `Provekit.Lsp.Plugin` | shipping in the current v1.6.2 tree |
| TypeScript | | planned |
| Go | | planned |
| C++ | | planned |
| Java / JVM | | planned |
| Swift | | planned |

If your kit is not in the shipping list, the in-editor squigglies are not yet available. The CLI loop (`cargo provekit-lift && provekit prove`) still works.

## What an LSP plugin does

The plugin is invoked by your editor (or your editor's host language server). It walks source files for Sugar-relevant annotations, runs the kit's lift adapters, performs the three-tier handshake, and returns LSP diagnostics. The result is in-editor squigglies on contract violations.

Per [`../../contributing/writing-an-LSP-plugin.md`](../../contributing/writing-an-LSP-plugin.md), the plugin protocol is small (NDJSON over stdio, three methods: `initialize`, `parse`, `shutdown`). Most editors don't need a custom integration; they integrate via existing LSP infrastructure.

## Editor support matrix

For each plugin, the editor wire-ups documented:

| Plugin × Editor | VSCode | Neovim | JetBrains | Emacs |
|---|---|---|---|---|
| Rust | ✓ extension | ✓ via nvim-lspconfig | ✓ rust-analyzer plugin | ✓ via lsp-mode |
| Python | ✓ extension | ✓ via nvim-lspconfig | ✓ PyCharm plugin | ✓ via lsp-mode |
| Zig | ✓ extension | ✓ | ✓ via Zig plugin | ✓ |
| Ruby | ✓ extension | ✓ | ✓ via RubyMine | ✓ |
| C# | ✓ extension | ✓ | ✓ Rider plugin | ✓ |

Per-editor configuration lives in:

- [`vscode.md`](vscode.md)
- [`neovim.md`](neovim.md)
- [`jetbrains.md`](jetbrains.md)
- [`emacs.md`](emacs.md)

Each per-editor doc walks: install the editor extension, configure the path to the plugin binary, restart the LSP, see squigglies.

## How to verify the LSP is working

After installation:

1. Open a file with annotations the kit recognizes (e.g., a Rust file with `proptest!` blocks, a Python file with `pydantic.BaseModel`, etc.).
2. Wait briefly for the LSP to parse.
3. The editor's diagnostic display should show Sugar as a diagnostic source.
4. If a contract is unmet (e.g., `@Min(0)` is violated by a constant `-1`), the editor should display a red squiggle with a message including `provekit`.

If no squigglies appear:

- The LSP plugin's binary may not be on the PATH that the editor is using.
- The kit's protocol catalog CID may not match the install (run `provekit verify-protocol` to check).
- The LSP plugin's process may have crashed; check the editor's LSP log.
- The annotation library you're using may not have a shipping lift adapter.

For specific troubleshooting, see [`../debugging-a-failed-handshake.md`](../debugging-a-failed-handshake.md).

## Diagnostic conventions

Sugar diagnostics use:

- **Severity:**
  - `error`: contract is provably violated.
  - `warning`: contract requires Tier 3 (Z3 fallback) and didn't discharge in the LSP timeout.
  - `information`: lifted contract; no violation.
  - `hint`: suggestion (e.g., "consider adding `@NotNull` to align with caller's contract").
- **Source:** always `"provekit"`. Filter by source for "show only Sugar issues."
- **Code:** stable error code, e.g., `PROVEKIT_E001`. See [`../../reference/error-codes.md`](../../reference/error-codes.md).
- **Message:** human-readable. Includes the contract that was violated and (where possible) the source-library annotation that gave rise to the contract.

Quick fixes (LSP `codeAction`) are forwards-looking; not all kits ship them yet. When they do, common patterns:

- "Add `@NotNull` to align with caller's contract."
- "Bind to reference contract `ref-X-v1`."
- "Mark as Tier 3 (allow this call site to fall through to solver)."

## Performance expectations

After warmup, the LSP should respond to `parse` requests in under 200ms for a typical 1000-line file. Cold-start (first parse after editor startup) may be slower due to lattice loading.

If your LSP is consistently slower than this, one of:

- The codebase has unusually many or complex annotations.
- The lattice is empty (cold start; see [`../../explanation/cold-start.md`](../../explanation/cold-start.md)).
- The LSP plugin process is bottlenecked on Tier 3 fallback (Z3 invocations).
- There's a bug; file an issue.

Performance tuning options live in `provekit.config.yaml`:

```yaml
lsp:
  parse_timeout_ms: 200
  tier3_max_invocations_per_parse: 3   # cap Tier 3 calls per request
  cache_size: 10000                     # max cached canonical IR per session
```

## Shared concerns across editors

- **PATH.** Make sure the editor (which inherits PATH from the launching shell, or has its own PATH) can find the LSP plugin binary. Restarting the editor after PATH changes is often necessary.
- **Permissions.** The plugin reads source files; make sure read permissions are correct.
- **Workspace boundaries.** The LSP plugin should be aware of the workspace root (typically inferred from `Cargo.toml`, `package.json`, etc.). If the wrong root is selected, lift adapters may not find the right files.
- **Hot reload.** When the protocol catalog version changes, restart the LSP. The plugin caches the protocol CID at startup; running an LSP against a mismatched protocol can produce cryptic errors.

## Read next

- [vscode.md](vscode.md): VSCode integration.
- [neovim.md](neovim.md): Neovim integration.
- [jetbrains.md](jetbrains.md): JetBrains family.
- [emacs.md](emacs.md): Emacs integration.
- [`../../contributing/writing-an-LSP-plugin.md`](../../contributing/writing-an-LSP-plugin.md): for porting to a new editor or kit.
