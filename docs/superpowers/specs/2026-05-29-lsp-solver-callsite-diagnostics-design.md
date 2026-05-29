# LSP Solver Callsite Diagnostics Design

Date: 2026-05-29

## Goal

Make the first LSP product slice show solver failures at the source callsite of the broken contract.

The gap is not contract discovery and not more lifting. The project already has
the pieces up through solver results. The missing product path is taking a
failed verifier obligation and projecting it into the IDE as a precise
diagnostic.

This PR is limited to the Rust/linkerd/LSP vertical slice. Per-language kit sweeps come after this proves the end-to-end diagnostic path.

Language ownership is strict:

- the Rust kit produces Rust facts from Rust source;
- the Python kit produces Python facts from Python source;
- the Java kit produces Java facts from Java source, in Java;
- the Rust CLI is language-agnostic coordinator/verifier infrastructure;
- the shared coordinator and linkerd do not parse host-language source or learn host-language framework semantics.

## Problem

The current architecture has the right upstream data, but drops the location before it reaches the editor:

- kit/lift output can emit `callSiteLocus` on call edges;
- `provekit-linker` receives that locus in `LinkerCallEdge.call_site_locus_json`;
- the solver verdict is converted into `LinkerError`;
- `LinkerError` preserves only `file`;
- `provekit-lsp` therefore publishes daemon diagnostics at `(0,0)..(0,1)`.

That produces a file-level marker, not the red squiggle users expect.

## Chosen Approach

Propagate the callsite locus through the existing Rust daemon path.

1. Extend `LinkerError` with an optional callsite range/locus payload derived from `LinkerCallEdge.call_site_locus_json`.
2. Include that range in `parseFile` and `getDiagnostics` JSON responses from `provekit-linkerd`.
3. Teach `provekit-lsp` to convert daemon ranges into LSP ranges instead of using the file-start fallback.
4. Map solver implication failures to `provekit.lsp.implication_failed` with source `provekit` and severity error.
5. Keep parse/lift failures, unresolved symbols, and undecidable solver results on their own diagnostic codes.

The design keeps lifting as upstream fact production. The product signal is the
solver/verifier result projected back to a source call expression.

This does not move Rust parsing into linkerd as a general pattern. The Rust
vertical slice uses the existing Rust kit path because this PR proves the
diagnostic last mile. Follow-on kit work keeps each language's analysis inside
that kit's implementation language.

## Alternatives Considered

1. Add a separate LSP-side callsite index.

   This would let the editor repair missing daemon locations, but it duplicates routing and risks divergence from linkerd's solver input.

2. Push all diagnostic construction into each language kit.

   This would give kits maximum control, but it would make solver diagnostics inconsistent and force every kit to understand verifier result shaping.

3. Propagate solver diagnostic loci through linkerd.

   This is the selected approach. The same component that owns the solver decision also owns the call edge that caused it, so it can preserve the source span without inventing a second lookup surface.

## Data Flow

```text
source call expression
  -> Rust lifter call edge with callSiteLocus
  -> linkerd project union
  -> linker solver obligation: caller post implies callee pre
  -> LinkerError with callsite locus
  -> parseFile/getDiagnostics JSON
  -> provekit-lsp Diagnostic at the callsite range
  -> editor red squiggle
```

## Range Rules

The initial accepted locus shape is:

```json
{"file": "/abs/path/src/lib.rs", "line": 20, "column": 17}
```

The linker treats `line` as 1-based and `column` as 0-based, matching the existing Rust lifter locus. The LSP server converts to 0-based LSP positions.

If the locus also carries `endLine`/`endColumn`, the LSP uses it. If it only carries a start point, the LSP marks a one-character range. If no usable locus exists, the LSP may fall back to `(0,0)..(0,1)`, but tests for this PR must prove the solver-failure path does not use that fallback.

## Diagnostic Rules

Solver-rejected implication:

- code: `provekit.lsp.implication_failed`
- source: `provekit`
- severity: error
- range: callsite expression or callee token
- message: explains that the callee precondition is not established at this callsite
- data: includes at least target symbol, source contract CID, reason, and raw callsite locus

Unresolved symbol:

- code: `provekit.lsp.unresolved_symbol`
- severity: warning
- range: callsite if available

Undecidable implication:

- code: `provekit.lsp.unprovable_obligation`
- severity: warning
- range: callsite if available

## Testing

Add tests at the narrowest useful layers:

1. `provekit-linker` unit test: an unsatisfied solver obligation preserves callsite locus in `LinkerError`.
2. `provekit-linkerd` test: `parseFile` returns a diagnostic containing the callsite locus/range.
3. `provekit-lsp` test: daemon diagnostic range converts to the expected LSP line/character instead of `(0,0)`.

The end-to-end fixture should use the existing floor shape: a satisfied call, a violated call such as `checkPositive(-1)`, and a loop/top fallback that does not emit a false positive.

## Out Of Scope

- Implementing all per-language kit call-edge emitters.
- Replacing legacy `parse` with `analyzeDocument`.
- Building a VS Code extension.
- Changing solver semantics.
- Treating lift gaps as solver failures.
- Adding Java, Python, TypeScript, or any other host-language parsing to the Rust coordinator or linkerd.

## Success Criteria

- A broken Rust contract implication produces a red squiggle at the callsite.
- The LSP diagnostic no longer uses the file-start fallback for solver implication failures.
- The diagnostic code is stable and matches the shared LSP protocol.
- Existing daemon and LSP tests still pass.
