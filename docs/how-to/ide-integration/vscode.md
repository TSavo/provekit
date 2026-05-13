# VSCode integration

VSCode is the most-deployed editor for ProvekIt LSPs. The official extensions live in the marketplace under the `provekit` publisher (when published). Each shipping kit has its own extension.

## Extensions per shipping kit

| Kit | Extension ID | Marketplace name |
|---|---|---|
| Rust | `provekit.rust-extension` | "ProvekIt for Rust" |
| Python | `provekit.python-extension` | "ProvekIt for Python" |
| Zig | `provekit.zig-extension` | "ProvekIt for Zig" |
| Ruby | `provekit.ruby-extension` | "ProvekIt for Ruby" |
| C# | `provekit.csharp-extension` | "ProvekIt for C#" |

> Extension IDs and marketplace names are placeholder; actual identifiers will be set when the extensions are published. Until then, the extensions live as workspace-local installs (`.vsix` files in `tools/vscode/`).

## Installation (when extensions ship to the marketplace)

```sh
code --install-extension provekit.rust-extension
code --install-extension provekit.python-extension
# (etc, one per language you use)
```

Or via the VSCode UI: Extensions tab → search "ProvekIt" → Install.

## Installation (workspace-local)

Until the extensions ship to the marketplace:

1. From the repository root, build the extension VSIX:
   ```sh
   cd tools/vscode/<kit-name>
   npm install
   npm run package
   # produces provekit-<kit>-<version>.vsix
   ```
2. Install the VSIX in VSCode:
   ```sh
   code --install-extension provekit-rust-1.1.0.vsix
   ```
3. Reload VSCode.

## Configuration

After installation, the extension exposes settings in `.vscode/settings.json` (workspace) or globally:

```json
{
  "provekit.rust.serverPath": "/path/to/provekit-lsp-rust",
  "provekit.rust.serverArgs": [],
  "provekit.rust.protocolVersion": "1.1.0",
  "provekit.rust.diagnosticsEnabled": true,
  "provekit.rust.tier3Timeout": 5000
}
```

The `serverPath` is typically auto-detected (the extension shells out to find `provekit-lsp-rust` on PATH, falling back to a vendored binary). Override only if the auto-detect fails.

Setting names per kit follow the convention `provekit.<kit>.<setting>`.

## Verifying it works

Open a Rust file with proptest annotations:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn parse_int_is_total(s in "[0-9]{1,10}") {
        let _ = s.parse::<i32>();
    }
}
```

After a brief delay (the LSP is loading the lattice), you should see:

- ProvekIt as a source in the Problems tab.
- Hovering over `s.parse::<i32>()` shows the contract: "for short numeric strings, no panic occurs."
- A small ProvekIt indicator in the status bar (showing discharge fraction).

If you don't see these:

1. Open the Output panel → ProvekIt LSP. Read the log.
2. Common causes:
   - The LSP binary isn't on PATH and the auto-detect failed.
   - The protocol version mismatches your install.
   - The annotation library isn't recognized (the lift adapter isn't registered).
3. Check the troubleshooting section below.

## Troubleshooting

### Squigglies don't appear

- Check `provekit verify-protocol` from a terminal. If this fails, the install is broken.
- Check the Output panel → ProvekIt LSP for errors.
- Verify the file extension matches what the lift adapter expects (e.g., `.rs` for Rust, `.py` for Python).

### LSP repeatedly restarts

- The plugin process is crashing. The Output panel will have stack traces.
- Common cause: a malformed `.proof` in the workspace's dependency tree. Run `provekit verify --check-proofs` from the terminal to find the culprit.

### Diagnostics are stale

- Diagnostics may not refresh when files outside the current workspace change. Run "Restart Language Server" from the command palette.

### Performance is bad

- Open the Output panel → ProvekIt LSP. Look for log entries indicating Tier 3 invocations are slow.
- Lower `tier3Timeout` to fail fast on slow Tier 3 cases.
- Check `provekit prove` from the terminal; if Tier 3 is dominating, the lattice is cold.

## Quick fixes (LSP code actions)

When the extension supports code actions for a kit:

- Place cursor on a contract violation diagnostic.
- Use Cmd+. (Ctrl+.) to invoke the quick fix menu.
- Common actions:
  - "Add `@NotNull`" / "Add `requires(x >= 0)`" / etc.
  - "Bind to reference contract `ref-X-v1`."

Quick fix support varies by kit version. Check the extension's CHANGELOG.

## Per-language specifics

### Rust

The Rust extension reads `Cargo.toml` to determine the workspace root. Lift adapters for `proptest`, `contracts` are auto-loaded.

The extension can be configured to run alongside rust-analyzer (default; ProvekIt diagnostics appear separately) or to delegate to rust-analyzer for parsing (lower memory usage).

### Python

The Python extension reads `pyproject.toml` to determine the project root. Lift adapters for `pydantic` are auto-loaded.

For best results, enable Pyright or pylsp alongside ProvekIt. ProvekIt focuses on contract verification; type checking is the host language server's job.

### Zig

The Zig extension reads `build.zig.zon`. Comment-based annotations (`//provekit:contract`, etc.) are recognized.

### Ruby

The Ruby extension reads `Gemfile`. Requires Ruby 3+.

### C#

The C# extension integrates with the .NET project structure. Lift adapters for `DataAnnotations` and `LINQ` are auto-loaded.

## Read next

- [overview.md](overview.md): IDE integration matrix.
- [neovim.md](neovim.md): Neovim equivalent.
- [`../debugging-a-failed-handshake.md`](../debugging-a-failed-handshake.md) (when squigglies aren't right).
- [`../../contributing/writing-an-LSP-plugin.md`](../../contributing/writing-an-LSP-plugin.md): porting to a new editor or kit.
