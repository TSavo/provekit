# Sugar LSP from Protocol Spec — Sufficiency Analysis

**Date:** 2026-04-30
**Status:** Investigation note. Companion to the `scripts/lsp-from-spec/` prototype.
**Question being answered:** Can a Language Server Protocol (LSP) implementation surface Sugar-aware diagnostics, hovers, and navigation by reading the protocol spec stack alone, without depending on the `sugar` reference implementation?
**Short answer:** Yes for the read-only capabilities (hover, diagnostics, document symbols, semantic tokens, code lens). Partial for navigation (go-to-definition, find-references, code actions) — the gaps are listed in §6.

## 1. Why this matters

If the protocol stack is sufficient, the architectural promise of Sugar holds: any IDE vendor, any language server, any third-party tooling can implement Sugar-aware features by reading the published specs and pinning a protocol-catalog CID. The reference implementation in `src/` is one realization, not the source of truth. If the protocol stack is *not* sufficient, then the framework is in practice a single-implementation product with extra documentation, and the "protocol-leads" posture is aspirational.

This doc is the audit. We walk through every LSP capability, identify which spec sections supply the necessary information, and flag the gaps.

## 2. The spec surface area available to an LSP author

The seven authoritative protocol specs (CIDs from the v1.0.0 catalog, `2026-04-30-protocol-versioning.md`):

| # | Spec | CID | What it gives an LSP |
|---|---|---|---|
| 1 | `2026-04-30-ir-formal-grammar.md` | `0c394dbb0bc6da2b` | EBNF for the kit-emitted IR JSON. Production rules per node kind. Locked key order. Strict-mode constraints. |
| 2 | `2026-04-30-canonicalization-grammar.md` | `cb2367c97b57ba05` | CDDL for the post-pass-6 canonical AST. Pseudocode for passes 1..6. ABNF for JCS encoding per §7.3. CBOR encoding per §7.2. SHA-256-prefix-16 hash construction per §9. |
| 3 | `2026-04-30-memento-envelope-grammar.md` | `68f4b1cc55c01667` | CDDL for memento envelopes by role (catalog, property, bridge, verdict, audit, deprecation). Referent constraints. Derived-field rules. |
| 4 | `2026-04-30-signatures-and-non-repudiation.md` | `9b9f86ec1795ff90` | Ed25519 signing surface. Key memento shape. Canonical bytes signed. |
| 5 | `2026-04-30-chain-validity-and-fail-closed.md` | `7d7777ef5b0017fe` | Datalog for chain validity. Reject cases R1..R15. Verifier policy P1..P8. Validity-report shape. |
| 6 | `2026-04-30-ir-extension-protocol.md` | `c48b69c15e1eb7e9` | Bootstrap core (sorts, predicates, connectives). Extension declaration CDDL. Resolver semantics. |
| 7 | `2026-04-30-protocol-versioning.md` | self | Catalog CID = protocol version. Conformance declarations. |

Plus the architectural background (`semantic-envelope`, `supply-chain-via-semantic-envelope`).

This is what an LSP implementer reads. Nothing else is required by the protocol-leads posture.

## 3. Capability-by-capability sufficiency table

For each LSP capability commonly expected of a language server, we enumerate (a) what the LSP needs to compute, (b) which spec section supplies it, and (c) whether the spec is sufficient or has a gap.

### 3.1 Diagnostics (`textDocument/publishDiagnostics`)

**Need:** Validate that an IR-JSON document conforms to the kit-emit grammar. Report violations as squiggles with file/line/column.

**Spec source:** ir-formal-grammar §"Determinism rules" lists all rules with sufficient precision (closed objects, locked key order, no whitespace, no NaN/Infinity, JSON-standard escaping). The reference parser at `src/ir/grammar/parse.ts` is described as exposing `GrammarParseError` with JSON Pointer + expected/actual; an LSP author has all the primitives to build an equivalent.

**Sufficiency:** **Sufficient.** Strict-mode constraints (key order, locked predicate names, locked sort names) are explicit in §"Strict mode."

**Additional:** chain-validity reject cases R1..R15 give canned diagnostic messages once the LSP understands which validity rule failed. ir-extension-protocol §5 gives the fail-closed cases for unresolved extensions.

### 3.2 Hover (`textDocument/hover`)

**Need:** When the user hovers over a piece of an IR formula in a `.sugar/invariants/<id>.json` file, surface useful info: node kind, sort, semantic content if it's an extension, propertyHash CID for the enclosing formula.

**Spec source:**
- Node kind / sort / locked key order: ir-formal-grammar productions.
- propertyHash for the formula: canonicalization §3 pipeline + §9 hash construction.
- If the predicate or constructor name is an extension: ir-extension-protocol §3 (extension declaration CDDL) + §2.4 (which names are bootstrap vs extension).

**Sufficiency:** **Sufficient for the bootstrap-only case.** For extensions, the LSP needs to resolve the name to an extension-declaration memento (§3.3 below).

**Worked example:** see §4 below.

### 3.3 Go-to-definition (`textDocument/definition`)

**Need:** Jump from a predicate or constructor name in IR JSON to its definition.

**Spec source:**
- Bootstrap names: canonicalization §4 CDDL `StandardPredicate` and `StandardSort` are the closed lists. The LSP "definition" target is the protocol spec's CDDL entry — i.e. the spec itself, addressed by CID.
- Extension names: each extension declaration is a memento with a CID. The "definition" target is the extension declaration memento.

**Gap:** The extension *resolver scope* is implicit. ir-extension-protocol §5.3 says collisions in the resolver's scope fail closed but does not specify how an LSP discovers which memento store is in-scope. Workspace-relative? Package-declared in `package.json`? An environment variable? The LSP needs a directory convention. **G1.**

### 3.4 Find-references (`textDocument/references`)

**Need:** Find every IR formula in the workspace that uses a given predicate, constructor, sort, or extension CID.

**Spec source:** Trivially derivable by parsing every `.sugar/invariants/<id>.json` (per ir-formal-grammar) and walking the AST.

**Sufficiency:** **Sufficient.** The LSP just needs to discover the workspace's invariant files. Convention is implicit but standard ("`.sugar/invariants/`"); spec edit candidate **G2** would write this layout into the protocol.

### 3.5 Autocomplete (`textDocument/completion`)

**Need:** Suggest predicate names, sort names, node kinds, constructor names as the user types.

**Spec source:** Bootstrap completions are static, sourced from canonicalization §4 CDDL closed lists. Extension completions come from the resolver scope (§3.3, gap G1).

**Sufficiency:** **Sufficient for bootstrap.** Extensions inherit gap G1.

### 3.6 Code lens (`textDocument/codeLens`)

**Need:** Inline annotations like "propertyHash: `e04b7cc4...`" or "verdict: holds (3 mins ago)" above each property declaration.

**Spec source:** propertyHash from canonicalization §3+§9. Verdict shape from memento-envelope-grammar's verdict role. Recency/timestamp from `iso8601` field on the verdict memento.

**Sufficiency:** **Sufficient.** The LSP must implement passes 1..6 and pass 7 encoding to compute propertyHash, but the pseudocode in canonicalization §8 is explicit and direct.

### 3.7 Semantic tokens (`textDocument/semanticTokens`)

**Need:** Color predicate names, sort names, quantifiers, constants distinctly.

**Spec source:** Closed lists in canonicalization §4 CDDL classify names by category. The token type for each node kind is derivable from ir-formal-grammar's productions.

**Sufficiency:** **Sufficient.**

### 3.8 Document symbols (`textDocument/documentSymbol`)

**Need:** Outline view: list of property declarations and bridge declarations in the file.

**Spec source:** ir-formal-grammar §"Declarations" enumerates exactly two top-level declaration kinds with named fields. Outline construction is a one-pass walk.

**Sufficiency:** **Sufficient.**

### 3.9 Code actions (`textDocument/codeAction`)

**Need:** Quick fixes — "convert `==` to `=`" (predicate alias resolution), "wrap in forall," "remove implies and rewrite as or."

**Spec source:** canonicalization §8.2 ALIAS_TABLE gives the rewrite for `==` → `=`. Pass 4 gives the implies → or(not, c) rewrite. Pass 5 NEGATE_PREDICATE gives `not(=)` → `≠`.

**Sufficiency:** **Sufficient for the rewrites the canonicalizer already specifies.** For richer refactors (introducing a quantifier, splitting a conjunction), the spec is silent — but those are beyond what a "Sugar-aware LSP" needs to commit to.

## 4. Worked example: hovering over `parseInt(s)`

User code (TypeScript, an annotated invariant):

```ts
property("parseInt_returns_int", forAll(String, (s) => isInt(parseInt(s))))
```

Resulting kit-emitted IR JSON (per ir-formal-grammar):

```json
[{"kind":"property","name":"parseInt_returns_int","formula":{"kind":"forall","sort":{"kind":"primitive","name":"String"},"predicate":{"kind":"lambda","varName":"_x0","sort":{"kind":"primitive","name":"String"},"body":{"kind":"atomic","predicate":"isInt","args":[{"kind":"ctor","name":"parseInt","args":[{"kind":"var","name":"_x0","sort":{"kind":"primitive","name":"String"}}],"sort":{"kind":"primitive","name":"Int"}}]}}}}]
```

User hovers over `parseInt`. The LSP, parsing per ir-formal-grammar §"Terms" / `CtorTerm`, identifies the position is inside a `ctor` node with `name = "parseInt"`. It now needs to surface useful info:

| Info | Sourced from |
|---|---|
| **Node kind: constructor** | ir-formal-grammar §"CtorTerm" production. |
| **Return sort: Int (primitive)** | The same `ctor` node carries `sort: {kind:"primitive", name:"Int"}`. |
| **Argument sort: String (primitive)** | The lone arg's `sort` field. |
| **Bootstrap or extension?** | ir-extension-protocol §2.4 lists `parseInt` explicitly under "What's NOT bootstrapping." Therefore: extension. |
| **Extension declaration CID** | Resolved from the active extension memento store (gap G1). The extension declaration carries the formal semantics, the SMT-LIB theory reference, and the compiler compatibility list. |
| **Enclosing formula's propertyHash** | Computed by running the formula through canonicalization passes 1..6 (canonicalization §8), encoding per §7.2 (CBOR) or §7.3 (JCS), then SHA-256-prefix-16 per §9. |
| **Fail-closed status** | If the LSP cannot resolve the `parseInt` extension memento in its scope, chain-validity §R8 (or analog for unresolved extension) fires. The LSP surfaces a diagnostic: "extension `parseInt` not resolvable in current scope." |

A useful hover card might render as:

```
parseInt  (constructor, extension)
  signature: parseInt(String) -> Int
  declared in: extension-declaration sha256:abcd1234...
  enclosing property: parseInt_returns_int
  propertyHash (jcs-rfc8785): sha256:e04b7cc466911b1d
```

This single hover exercises four of seven specs (ir-formal-grammar, canonicalization, ir-extension-protocol, chain-validity). The fifth (memento-envelope) is hit if the user expands the extension declaration.

## 5. The prototype

`scripts/lsp-from-spec/` demonstrates the simplest end-to-end path:

1. Read a fake `.sugar/invariants/<id>.json` file.
2. Parse it per ir-formal-grammar (inline parser, no `sugar` import).
3. Compute the propertyHash per canonicalization §3 + §7.3 (JCS, simplified — see §6 below for what's elided) + §9.
4. Surface hover info containing the CID over LSP `textDocument/hover`.

The prototype **does not** attempt the full canonicalization pipeline; it uses an input that is already canonical (an empty `and()` simplifies to `true` — passes 1..6 are essentially identity over `Atomic("true", [])`). This is enough to demonstrate the path; growing it to handle arbitrary input requires implementing passes 1..6 in canonicalization §8, which is straightforward but mechanical.

## 6. Honest gaps — concrete spec-edit candidates

These are places where a from-spec implementer hits an interpretive question and has to either guess or read the reference TypeScript. Each is a concrete follow-up edit candidate.

**G1. Extension resolver scope is unspecified.** ir-extension-protocol §5.3 mandates fail-closed on collisions in "the resolver's scope" without defining what that scope is. An LSP needs to know: workspace-relative directory? `package.json` field? Per-project memento store? This blocks go-to-definition, autocomplete, and the parseInt hover example above.

**G2. Workspace layout convention is implicit.** The protocol nowhere mandates `.sugar/invariants/<id>.json` as the on-disk convention. The reference tooling uses it; a spec-only LSP author has to read the reference code or guess. Spec edit: a one-section "Workspace conventions" entry naming the directory.

**G3. CDDL/EBNF mismatch on IR formula reference.** memento-envelope-grammar imports `SugarIrFormula` (CDDL rule name `ir-formula`) "by name" from ir-formal-grammar, but ir-formal-grammar specifies the IR JSON as EBNF, not CDDL. A CDDL validator can't import an EBNF rule. The protocol either needs a CDDL translation of the IR JSON grammar or an explicit alias section.

**G4. Canonicalization §17 alignment items mean the reference TS does not yet match the protocol.** A from-spec LSP that computes propertyHash per the spec will produce hashes that DIFFER from the reference TS in three documented cases:
- spec defaults to `cbor-rfc8949`, TS reference emits `jcs-rfc8785`;
- spec emits bignum integers as decimal digits, TS reference emits `"bigint:N"` strings;
- spec requires `Const{sort:Real, value:3.0}` to render with decimal point or exponent, TS reference emits `3`.

This is the protocol-leads posture working as intended (the TS reference is a follow-up), but a spec-only implementer should be told directly: "if your hashes differ from TS for these three cases, the TS is the side that needs alignment, not yours."

**G5. No top-level pipeline diagram.** A reader entering the spec stack cold must traverse 4-5 documents to assemble the dataflow `kit emits IR JSON → canonicalize → propertyHash → memento envelope → chain-validity gate`. Each spec links forward, but there is no single page showing the full chain. Spec edit candidate: an architectural index doc, or a §0 to each spec showing where it sits in the chain.

**G6. Verifier-policy override semantics are summary-only in this analysis.** chain-validity §P1..P8 enumerate eight policies a verifier may override. An LSP that surfaces verdict info needs to know which policies are in effect; the spec is precise about the policy shapes but does not specify how a verifier publishes its policy stack to consumers. This is fine for a read-only LSP (display the verdict; don't second-guess the policy) but blocks code-action recipes like "this verdict held only because policy P3 was active — view P3."

**G7. Encoding-form selection is a workspace-level choice.** canonicalization §7 specifies two `canonicalForm` values (`cbor-rfc8949`, `jcs-rfc8785`) with the explicit rule that hashes are NOT cross-comparable. An LSP must know which form a workspace uses. Per spec, this lives on the verdict memento, but for hover-time computation (before any verdict exists), the LSP needs a workspace default. Spec edit candidate: name the workspace-level field that pins `canonicalForm`.

## 7. Sufficiency conclusion

The protocol stack is **sufficient for a read-only Sugar-aware LSP** providing diagnostics, hover, document symbols, semantic tokens, and code lens for IR JSON files, **modulo the seven gaps in §6**. None of the gaps is architectural; each is an under-specified detail that becomes a one-paragraph spec edit.

Navigation features (go-to-definition, find-references, code actions involving extensions) inherit gap G1 and require resolver-scope clarification before a clean implementation is possible.

Hashing-bearing features (the parseInt example's propertyHash, code lens annotations) work today but produce hashes that intentionally diverge from the current TS reference per gap G4. This is a feature of the protocol-leads posture, not a bug; it must be communicated clearly to spec-only implementers.

The architectural claim — *the protocol is what an implementer needs* — survives this audit. The spec is a few honest edits away from being a complete IDE-implementation contract, and crucially, a third-party LSP author working from the spec alone never needs to read `src/`.

## 8. Cross-references

- `2026-04-30-ir-formal-grammar.md`
- `2026-04-30-canonicalization-grammar.md`
- `2026-04-30-memento-envelope-grammar.md`
- `2026-04-30-signatures-and-non-repudiation.md`
- `2026-04-30-chain-validity-and-fail-closed.md`
- `2026-04-30-ir-extension-protocol.md`
- `2026-04-30-protocol-versioning.md`
- `2026-04-29-the-semantic-envelope.md`
- `2026-04-29-supply-chain-via-semantic-envelope.md`
- companion prototype: `scripts/lsp-from-spec/`
