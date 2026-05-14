# Bind-IR Lift-Result Shape (`bind-ir/1`)

**Status:** v1.1.0 normative draft for the `ir-document.ir[]` payload that lift plugins return when invoked from `provekit bind`.
**Date:** 2026-05-13
**Author:** T Savo

**Related:**
- `2026-04-30-lift-plugin-protocol.md` (the JSON-RPC `lift` method, the `ir-document` outer shape, LEGACY-RETAINED under PEP 1.7.0 §0.4)
- `2026-05-12-plugin-protocol.md` (PEP 1.7.0; `kind = "lift"` mementos)
- `2026-05-12-trinity-java-roundtrip-transport-gaps.md` (the trinity round-trip whose lift legs consume this shape)
- `2026-05-13-body-template-memento.md` (downstream realize plugins emit bodies for the concepts named in this shape)

## §0. Purpose

`provekit bind` runs the eight-verb pipeline (Lift, Cluster, Name, Scope, Cluster, Identify, Realize, Witness). Verbs 2 through 7 are language-agnostic: they consume already-lifted records and produce concept clusters, scope assignments, identification edges, and discharge verdicts. Verb 1 (Lift) is language-specific.

Per Supra omnia, rectum and the algebra-is-the-portable-thing thesis, Verb 1 MUST be a content-addressed plugin: zero language knowledge in the CLI core. Any language with a `kind = "lift"` plugin registered through PEP 1.7.0 (`2026-05-12-plugin-protocol.md`) can drive bind by emitting the `ir-document` shape this spec defines.

This spec defines the JSON shape every entry of `ir-document.ir[]` MUST take when the lift kit is invoked by bind. The wire envelope (`ir-document = { kind: "ir-document", ir: [...], diagnostics: [...] }`) is unchanged from `2026-04-30-lift-plugin-protocol.md`. Only the per-entry payload is pinned here.

## §1. Wire shape

```cddl
; A lift-result entry produced for the bind pipeline.
; Locked JCS key order: alphabetical within the object.

bind-lift-entry = {
  attr_post:          tstr / null,    ; LEGACY compatibility postcondition text; see §1.1
  attr_pre:           tstr / null,    ; LEGACY compatibility precondition text; see §1.1
  concept_annotation: tstr / null,    ; the `// concept: NAME` annotation, NAME only (no prefix)
  file:               tstr,           ; project-root-relative path of the source file
  fn_line:            uint,           ; 1-based line number of the `fn` keyword
  fn_name:            tstr,           ; bare function identifier
  kind:               "bind-lift-entry",
  param_names:        [* tstr],       ; parameter identifiers in declaration order
  param_types:        [* tstr],       ; source-language type names, same order as param_names
  return_type:        tstr,           ; source-language return type, "()" for unit
  term_shape:         term-shape-doc, ; structural fingerprint per §2
  term_shape_cid:     tstr,           ; "blake3-512:<128hex>" of canonical term_shape JCS bytes
  witnesses:          [* bind-contract-witness-entry]
}

bind-contract-witness-entry = {
  col:                     uint / null, ; 0-based byte column of the source surface, if known
  confidence_basis_points: uint / null, ; prior confidence, defaults from source_kind
  extension_fields:        {* tstr => any},
  line:                    uint / null, ; 1-based line of the source surface, if known
  predicate:               ir-formula / null,
  predicate_text:          tstr / null, ; compatibility text form when no IR formula is available
  role:                    "pre" / "post" / "inv" / tstr,
  source_kind:             source-kind
}

source-kind = "annotation"
            / "test-assertion"
            / "type-signature"
            / "docstring"
            / "loop-invariant"
            / "implicit-effect"
            / "native-surface"
            / "structural-synthesis"
            / "empirical-witness"
            / "review-comment"
            / tstr
```

### §1.1 Field semantics

| Field                | Required | Meaning |
|----------------------|----------|---------|
| `attr_pre`           | yes      | LEGACY compatibility precondition text extracted from older annotation-only lift kits. New producers SHOULD emit `null` and place all contract evidence in `witnesses[]`. Consumers MUST use this field only when `witnesses[]` is empty. |
| `attr_post`          | yes      | LEGACY compatibility postcondition text extracted from older annotation-only lift kits. New producers SHOULD emit `null` and place all contract evidence in `witnesses[]`. Consumers MUST use this field only when `witnesses[]` is empty. |
| `concept_annotation` | yes      | The NAME from a `// concept: NAME` (or language-equivalent) comment immediately preceding the function. The kit MUST strip the `concept:` prefix; producers emit `identity`, not `concept:identity`. `null` when absent. |
| `file`               | yes      | Path relative to the project root (the `workspace_root` lift-params field). Forward slashes only; the kit MUST normalize. |
| `fn_line`            | yes      | The 1-based line number of the function declaration (the line containing the `fn`/`def`/method keyword). |
| `fn_name`            | yes      | The function identifier exactly as it appears in source (no module qualification). |
| `kind`               | yes      | MUST be the literal `"bind-lift-entry"`. The discriminator that lets future bind-IR shapes coexist in the same `ir-document`. |
| `param_names`        | yes      | Parameter names in declaration order. Methods receive an implicit `__self` element first. Anonymous patterns receive a stable placeholder like `__arg{i}`. |
| `param_types`        | yes      | Source-language type names, same order as `param_names`. The kit MUST emit at least the source-language name; conversions to other languages are realize-time. |
| `return_type`        | yes      | The source-language return type, or `"()"` for unit/void. |
| `term_shape`         | yes      | A language-neutral structural fingerprint of the function body, defined per §2. |
| `term_shape_cid`     | yes      | `"blake3-512:" + hex(BLAKE3-512(JCS-canonical bytes of `term_shape`))`. Used as the bucket key for clustering. |
| `witnesses`          | yes      | Authoritative contract witnesses already married to this function/concept site. Each witness is promoted directly to an `EvidenceMemento` by cmd_bind. Legacy `attr_pre` / `attr_post` producers MAY leave this empty; cmd_bind auto-promotes those fields as `source_kind = "annotation"` witnesses for backward compatibility. |

### §1.2 Contract witness semantics

`bind-contract-witness-entry.source_kind` MUST use the existing `EvidenceMemento.source_kind` vocabulary from `2026-05-13-compound-contract-memento.md` §10. Lift kits MUST NOT invent a parallel bind-only source-kind enum. Unknown future labels are carried as open extensions and map to `SourceKind::Other`.

`predicate` is preferred when the lifter has an `IrFormula`. `predicate_text` is the compatibility surface for existing annotation strings and native extractor ecosystems that have not yet lowered their predicate into IR. When both are present, `predicate` is authoritative for evidence minting and `predicate_text` is retained only for source re-emission surfaces.

When `witnesses[]` is non-empty, it is the complete contract evidence set for the bind entry. `attr_pre` and `attr_post` MUST NOT add predicates, override predicates, or alter the composed contract in a conforming consumer. They are ignored except for diagnostics. A producer that still has only legacy annotation strings MAY set `witnesses[] = []`; the consumer compatibility shim then lifts `attr_pre` / `attr_post` as annotation witnesses.

### §1.3 What is OUT OF scope for this entry

- **Unmarried test streams.** Test-derived, native-surface, docstring, and type-signature producers that have not yet identified the target function/concept MAY still emit their own evidence surfaces. This bind entry carries the married form: the lifter has already associated the witness with the function named by `file` + `fn_name`.
- **The full IR algebra term.** This entry is the BIND surface (clustering + naming + scoping); the full algebra term used by transport is requested separately via the realize-plugin protocol (`provekit.plugin.invoke` with `method: "lift"` on the realize side, see `2026-05-12-plugin-protocol.md` §4.2.2 and the body-template realize plugins).
- **Concept-shape catalog matches.** The kit MUST emit `term_shape` + `term_shape_cid`; the catalog match is performed by cmd_bind (Verb 6: Identify), not the kit.

## §2. Term shape

A term shape is a language-neutral fingerprint of the function body sufficient to cluster structurally-identical functions across languages.

```cddl
term-shape-doc = {
  kind:         term-shape-kind,
  ? cond:       term-shape-doc,    ; for `if` and `while`
  ? then:       term-shape-doc,    ; for `if`
  ? else:       term-shape-doc,    ; for `if` with explicit else branch
  ? body:       term-shape-doc,    ; for `while` and `for`
  ? stmts:      [* term-shape-doc],; for `body` and `block`
  ? op:         tstr               ; for `rel` and `bin`
}

term-shape-kind = "body"
                / "block"
                / "if"
                / "while"
                / "for"
                / "exit"          ; return, break, continue
                / "assign"
                / "let"
                / "rel"           ; relational binary op (==, !=, <, <=, >, >=)
                / "bin"           ; arithmetic/other binary op (+, -, *, /, %)
                / "call"
                / "opaque"
                / tstr            ; open extension; cmd_bind buckets unknown kinds by CID
```

The canonical labels MUST be emitted by every kit when the body matches the pattern. A kit MAY emit additional labels under §1.1's open-extension rule; cmd_bind treats them as opaque clusters keyed on `term_shape_cid`.

The kit MUST emit `term_shape` in a form whose JCS-canonical bytes are stable across runs. Specifically:
- Object keys in alphabetical order (per `2026-04-30-canonicalization-grammar.md`).
- Arrays in source order (slot 0 = first child, etc.).
- Optional fields MUST be omitted entirely (not set to `null`) when absent.

## §3. Example

A Rust `pub fn identity(x: i32) -> i32 { x }` lifted with the annotation `// concept: identity` produces:

```json
{
  "attr_post": null,
  "attr_pre": null,
  "concept_annotation": "identity",
  "file": "src/lib.rs",
  "fn_line": 4,
  "fn_name": "identity",
  "kind": "bind-lift-entry",
  "param_names": ["x"],
  "param_types": ["i32"],
  "return_type": "i32",
  "term_shape": { "kind": "body", "stmts": [{ "kind": "opaque" }] },
  "term_shape_cid": "blake3-512:...",
  "witnesses": []
}
```

Wrapped in the standard `ir-document` envelope:

```json
{
  "kind": "ir-document",
  "ir": [ /* one bind-lift-entry per function */ ],
  "diagnostics": []
}
```

## §4. Compatibility

- A `kind = "lift"` PEP 1.7.0 plugin MAY emit the bind-lift-entry shape alongside other entry kinds in the same `ir-document.ir[]`. cmd_bind selects only entries with `kind = "bind-lift-entry"` for the eight-verb pipeline.
- The `provekit-lift` Rust binary (which emits `proof-envelope` results for `provekit prove`) is a DIFFERENT lift surface. Bind kits and prove kits MAY share an implementation but MUST honor the surface the caller requested through `lift-params.surface`.
- Future entry kinds for unmatched evidence streams MAY extend the bind surface without breaking this v1.1.0 entry shape. Once a producer has married evidence to a function/concept site, it SHOULD use `witnesses[]`.

## §5. Refusal vs gap

If a lift kit cannot produce a bind-lift-entry for a function it visited (parse error, unsupported syntax, missing AST visibility), it SHOULD emit a `diagnostics` entry rather than a refused RPC, and SHOULD omit the unliftable function from `ir.[]`. cmd_bind treats absence as `not lifted` and emits a `bind-lift-skipped` gap record per `body-template-memento.md` §5.

A kit that cannot run at all (the binary is missing, the protocol-version handshake fails) refuses per `2026-04-30-lift-plugin-protocol.md`; cmd_bind records this as a `kit-plugin-unavailable` gap, NOT as a substrate bug. Per Supra omnia, rectum, kit unavailability is a precise extension request, not a hidden error.
