# lsp-from-spec

Prototype Language Server demonstrating that a third-party IDE vendor can implement ProvekIt-aware features by reading the protocol spec stack alone, without depending on the `provekit` framework or its TypeScript reference implementation.

Companion to `protocol/specs/2026-04-30-lsp-from-protocol.md`.

## What this prototype does

1. **Parses** a `.provekit/invariants/<id>.json` file per `protocol/specs/2026-04-30-ir-formal-grammar.md`. The parser is inline (`parse.mjs`); it does not import from `src/`.

2. **Computes propertyHash** per `protocol/specs/2026-04-30-canonicalization-grammar.md`:
   - Passes 1..6 (de Bruijn, predicate canonicalization, sort canonicalization, implies removal, NNF, AC normalization) per §8.
   - JCS encoding per §7.3 (sorted keys, no whitespace, RFC 8785 strings, decimal-point form for `Real(N.0)`).
   - SHA-256-prefix-16 per §9.
   The implementation is inline (`canonicalize.mjs`); it does not import from `src/`.

3. **Surfaces hover info** via the Language Server Protocol: when the user hovers in a `.json` file, the server returns a markdown card naming each property declaration's name and propertyHash CID.

4. **Publishes diagnostics** when the document fails to parse against the IR grammar.

## What this prototype is NOT

- It is not a production VS Code extension. There is no client-side `package.json`, no `extension.ts`, no marketplace metadata.
- It does not implement the full canonicalization pipeline for arbitrary inputs. The implementation is correct for the inputs in `sample-invariant.json`; growing it requires only mechanical work, but the prototype's scope is the architectural demonstration.
- It does not implement the CBOR encoding (`canonicalForm = "cbor-rfc8949"`). It implements only `jcs-rfc8785`. Per the canonicalization spec §17 alignment item 1, CBOR is the spec's default and the TS reference is the side that diverges. A from-spec production LSP would implement both.
- It does not resolve extension declaration mementos. Extensions like `parseInt` would require gap **G1** from the analysis doc to be closed (workspace-relative resolver scope, currently unspecified).
- It does not validate signatures, walk the chain, or surface verdict mementos. Those features each require additional implementation work but no additional spec reading.

## Running the demo

```bash
cd scripts/lsp-from-spec
npm install     # vscode-languageserver, vscode-languageserver-textdocument
node demo.mjs   # end-to-end CLI demo
```

Expected output (abridged):

```
property "trivially_true" propertyHash: sha256:0b2e2cb911d7c3ae
property "forall_int_gt_zero" propertyHash: sha256:d922ff992e185d96
```

The hashes are computed per the protocol spec. They will **not** match what the current TypeScript reference at `src/canonicalizer/` produces, because the TS reference has three documented alignment items (canonicalization spec §17 items 2, 3, and the JCS-vs-CBOR default). This is the protocol-leads posture working as intended: the spec is authoritative, and the reference is a follow-up.

## Running the LSP server

```bash
node server.mjs --stdio
```

Connect any LSP-capable editor to stdin/stdout. Open a file matching `*.json` containing IR-grammar-conformant content; hovering surfaces the propertyHash for each property declaration.

## What it would take to grow this into a production LSP

Per the analysis doc §3, every standard LSP capability is implementable from the spec. To complete the prototype:

1. **Position-aware hover.** The current hover returns whole-file info. A production LSP would walk the JSON AST, locate the cursor's enclosing IR node, and surface info for that node specifically (per the worked example in §4 of the analysis doc).

2. **Full canonicalization pipeline.** The current pass-1..6 implementation is correct but lightly tested. A production LSP would have a fixture corpus (per canonicalization spec §11) to validate against the same goldens the reference TS validates against, modulo §17 alignment items.

3. **CBOR encoder.** Implement `canonicalForm = "cbor-rfc8949"` per §7.2. Roughly 100 lines of additional code (definite-length CBOR with sorted map keys + tag 40 for bitvectors).

4. **Extension resolver.** Once gap **G1** is closed in the spec, walk a workspace-relative directory of extension-declaration mementos and offer go-to-definition / hover for extension predicates and constructors.

5. **Memento-store integration.** For verdict-aware code lens annotations (`"holds, 3 mins ago"`), read verdict mementos per `protocol/specs/2026-04-30-memento-envelope-grammar.md` and render their `verdict` field.

6. **Chain-validity gate diagnostics.** Surface chain-validity reject cases R1..R15 (per `protocol/specs/2026-04-30-chain-validity-and-fail-closed.md`) as in-editor diagnostics.

None of these requires reading `src/`. Each is sourced entirely from the protocol spec stack, plus the seven gap edits enumerated in the analysis doc.

## Source files

| File | Spec section it implements |
|---|---|
| `parse.mjs` | ir-formal-grammar.md §"Top-level production" through §"Sorts," plus §"Determinism rules" rule 6 (closed objects). |
| `canonicalize.mjs` | canonicalization-grammar.md §3 pipeline, §6 sortKey, §7.3 JCS encoding, §8.1..§8.6 passes, §9 hash, §17 alignment items 2 and 3. |
| `demo.mjs` | End-to-end demonstration; not normative. |
| `server.mjs` | LSP transport for hover + diagnostics. Not normative. |
| `sample-invariant.json` | Two property declarations, both ir-formal-grammar conformant. |

## License

Same license as the parent repository.
