# ProvekIt Language Server

VS Code (and any LSP-aware editor) integration for ProvekIt. Surfaces:

- **"Proven wrong" diagnostics**: invariants that fail verification
  appear as red squiggles in the editor with the Z3 witness inline.
- **"What must be true here" hover**: hovering anywhere shows every
  invariant whose locus contains the hovered line.

## Architecture

The LSP is a thin shell. All protocol logic lives in `src/verifier/`
behind the `verifyProject()` function. The LSP:

1. Watches document open/save events.
2. Calls `verifyProject(projectRoot, options)` on save.
3. Filters the resulting `ValidityReport` to per-document rows via
   `rowsForFile()`.
4. Emits LSP `Diagnostic[]` for every row whose status is not "holds".
5. On hover, calls `rowsAtLine()` to find invariants containing the
   hovered position; renders them as a markdown panel.

When new spec pieces land (full Ed25519 signature verification,
extension-protocol resolver wired into the verifier, additional
verdict statuses), the LSP inherits without changes: the verifier
picks them up.

## Running

The LSP is built as a Node.js process. Editors launch it via stdio:

```bash
node ./lib/lsp/server.js
```

(Assumes the project's TypeScript has been compiled. During
development, `npx tsx src/lsp/server.ts` works.)

## VS Code integration

A separate VS Code extension wraps this server. The extension:

- Activates on TypeScript/JavaScript files in projects with a
  `.provekit/` directory.
- Spawns the LSP server.
- Forwards LSP messages.

That extension is a small piece of UI plumbing on top of this
server; the substrate is here.

## Independence from the protocol

This LSP imports `verifyProject` from the reference TypeScript
implementation in this repo. An alternative LSP could implement
ProvekIt's protocol from spec alone (no provekit dependency); see
`protocol/specs/2026-04-30-lsp-from-protocol.md` (in flight) for the
investigation. Both shapes are valid; this LSP is the convenience
implementation that ships with the kit.
