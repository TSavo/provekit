# Operand-binding sidecar schema for body synthesis

**Date:** 2026-05-16
**Status:** LOCKED. Substrate-level ruling. All lift kits and realize plugins implementing operand-binding-from-context (A24) MUST conform to this schema.
**Authority:** T Savo (architect ruling), via Opus independent retry on a question where both Sir and Kit disclosed locality bias and landed on the same answer for distinct structural reasons.

## TL;DR

The operand-binding sidecar that flows from lift kits through the lower request to realize plugins is shaped as a flat list of (position, symbol) tuples with integer-array paths:

```json
"operand_bindings": [
  { "position": [0, 0], "symbol": "denom" },
  { "position": [0, 1], "symbol": "0" },
  { "position": [1], "symbol": "-1" }
]
```

- `position`: integer array. Each integer is an index into a `concept_name.args` array, walked from term_shape's root. NOT a string-encoded path.
- `symbol`: string. The source-program identifier or literal value at that position.
- Lives on the realize request, NOT in the bind payload. Non-hashed.

This schema is the operand-binding sidecar's canonical form. Parallel-tree shapes (α-options) and flat-positionless lists (γ-options) are both architecturally rejected.

## The architectural question

The γ canonical-form ruling (`docs/plans/2026-05-16-canonical-term-shape-form.md`) locked term_shape's operand slots as bare empty objects `{}`. Operand identity is derivable from context (function-level signature + scope + slot position), but not from term_shape's bytes alone.

A24 implements operand-binding-from-context via a non-hashed sidecar that carries the symbol-at-position mapping. The question: what's the sidecar's schema?

## Options considered

### Option α: parallel-tree mirroring term_shape

```json
"operand_bindings": { "args": [
  { "args": ["denom", "0"] },
  "-1",
  { "args": [...] }
]}
```

The sidecar mirrors term_shape's recursive structure. Realize walks term_shape and operand_bindings in lockstep.

**Rejected.** Three blocking concerns:

1. **Redundancy against term_shape**: the tree structure is ALREADY in term_shape's hashed bytes. The sidecar re-encodes it. Same structural smell that rejected sparse/expanded forms in the γ canonical-form ruling.

2. **Sidecar-pattern precedent contradicts α**: the Platform-Semantics-via-LossRecord ruling's sidecar (`docs/plans/2026-05-16-platform-semantics-via-loss-records.md`) is `HashMap<Cid, PlatformSemanticTag>`. Flat keyed map, not parallel tree. The substrate has already chosen β-shape at the platform-semantics sidecar layer; α would diverge from the established sidecar pattern.

3. **Forward-compat with γ's polymorphic extension**: when bare-`{}` grows to `{"sort": "concept:<cid>"}` for polymorphic ops (per the γ ruling's deferred-extension trigger), sorts flow into the hashed channel; symbols should stay in the non-hashed sidecar. α re-entangles them at the leaf, defeating the decoupling the sidecar architecture exists to enable.

### Option γ-flat-positionless: flat pre-order list

```json
"operand_bindings": ["denom", "0", "-1", ...]
```

A flat list of symbols in pre-order traversal of term_shape's leaves, without explicit positions.

**Rejected.** A missing or extra leaf silently shifts all subsequent indexes, producing wrong-but-well-formed realize output. Misalignment surfaces only as semantic errors at runtime, not at parse/structure time. Under Supra omnia rectum, explicit positions surface misalignment immediately and are worth the byte cost.

### Option β-string-paths: flat list with string-encoded paths

```json
"operand_bindings": [
  { "position": "args[0].args[0]", "symbol": "denom" },
  ...
]
```

A variant of the locked schema with positions as path strings.

**Rejected.** String parsing is gratuitous drift surface that no architect tightening can fully constrain across two lifters. Different lifters might emit `args[0].args[0]` vs `[0][0]` vs `0.0` vs other conventions; federation byte-identity breaks within two lifters on syntax skew alone. Integer arrays are the canonical, deterministic, byte-stable form.

### Option β: flat list of (integer-array-position, symbol) tuples (LOCKED)

See the TL;DR. Locked as the canonical sidecar shape.

## The seven-point reasoning chain

The locked decision rests on these structural arguments, each of which holds independent of the others. The decision is robust to any single argument being weaker than claimed.

**1. Redundancy (decisive against α).** term_shape encodes the tree structure in its hashed bytes. α's parallel-tree re-encodes that structure in the sidecar. β encodes ONLY what's not in term_shape: the leaf symbols.

**2. Sidecar-pattern precedent.** The platform-semantics sidecar on main is `HashMap<Cid, PlatformSemanticTag>`. A flat keyed map. The substrate has already chosen β-shape for sidecars at the platform-semantics layer. α diverges from the established pattern.

**3. Forward-compat with γ's polymorphic extension.** When `{}` grows to `{"sort": "concept:<cid>"}`, sorts flow to term_shape's hashed leaves; symbols stay in β's non-hashed list. The two channels evolve independently. α re-entangles them at the leaf, defeating the decoupling.

**4. Federation byte-identity + diagnostics.** Both achieve byte-identity given canonical args order. But β yields interpretable divergence diagnostics (`position [0,1]: Rust='x', Python='y'`); α's parallel-tree diff is opaque.

**5. Realize-side completeness gate (Supra omnia rectum).** β's two-pass (build path-to-symbol map, then resolve leaves) enables a free completeness cross-check: every term_shape leaf has a binding; every binding's path resolves to a leaf. Misalignment surfaces at parse time. α has no equivalent gate; misalignments would surface later as wrong-but-well-formed realize output. The substrate's first principle prefers explicit-cross-check over implicit-trust-the-structure.

**6. Wire-format diff stability.** When a kit reorders args (e.g., `[lhs, rhs]` to `[rhs, lhs]` as a substrate-decision), β's position list invalidates surgically; α's parallel tree requires walking both trees to see what moved.

**7. Anti-positionless-list rejection.** The γ-flat-positionless option (flat pre-order list without explicit positions) was considered and rejected. A missing or extra leaf silently shifts all subsequent indexes. Explicit positions surface misalignment immediately.

## The exact schema (canonical form)

```json
{
  "operand_bindings": [
    { "position": [<int>, <int>, ...], "symbol": "<string>" },
    ...
  ]
}
```

### Position field

- Type: array of non-negative integers
- Semantics: each integer is an index into the `args` array at the corresponding nesting level of term_shape, walked from the root
- Examples:
  - `[]` (empty array): the root operand (used when term_shape's root is itself a bare operand slot, which is structurally rare but possible)
  - `[0]`: the first arg of term_shape's root operation
  - `[0, 1]`: the second arg of the first arg of term_shape's root operation (e.g., the RHS of the first operand of a binary op)
  - `[1, 0, 2]`: deeper nesting, walked left-to-right

### Symbol field

- Type: string
- Semantics: the source-program identifier (param name, let-binding name) OR the canonical string form of the literal value at that position
- Literal values: emit canonical string form per the source language's lexical convention. For integers: `"0"`, `"-1"`, `"42"`. For booleans: `"true"`, `"false"`. For strings: include surrounding quotes per the source language convention. For other literals: TBD per language-specific lift kit emission.

### Position uniqueness

- Each `position` in operand_bindings MUST be unique
- Lift kits MUST emit bindings in position order (lexicographic by integer array) for byte-stable wire format

### Channel location

- `operand_bindings` MUST live on the realize request alongside (but distinct from) the bind payload
- `operand_bindings` MUST NOT enter the bind CID hash
- Realize plugins read both term_shape (from bind payload) AND operand_bindings (from realize request sidecar) during body synthesis

## Lift-side emission requirements

Each lift kit (Rust walk_rpc.rs, Python bind_lifter.py, and all future lift kits) MUST:

1. Walk the source AST in the same pass that constructs term_shape
2. At each operand-slot position (a position that would be a bare `{}` in term_shape), record the source-program symbol referenced at that AST position
3. Emit operand_bindings as a flat list ordered by position
4. Use integer-array paths (NOT string-encoded paths)
5. Use canonical string form for literal values (per the symbol field semantics above)

For literals: emit the source literal's value as a string per the source language's canonical lexical form. Don't rewrite literals or normalize across languages at the lift stage; that's a transport-gap-memento concern, not a sidecar-emission concern.

For identifiers: emit the source-program identifier verbatim. Don't normalize naming conventions; non-hashed channel means convention differences don't break federation byte-identity at the algebra layer.

## Realize-side consumption requirements

Each realize plugin MUST:

1. Read both term_shape (from bind payload) and operand_bindings (from realize request sidecar)
2. Pre-process operand_bindings into a `HashMap<Vec<usize>, String>` (position → symbol)
3. Walk term_shape during body synthesis; at each operand slot, look up the symbol by position
4. **Completeness gate**: assert that every term_shape leaf operand slot has a corresponding entry in the path-to-symbol map; assert that every entry in the map resolves to an actual term_shape leaf. Refuse synthesis on misalignment with a clear diagnostic (e.g., `OperandBindingMisalignment { missing_positions: [...], extra_positions: [...] }`).

The completeness gate is load-bearing per Supra omnia rectum: shipping body synthesis with silent operand-binding misalignment violates substrate correctness. The gate must be present, not deferred.

## Federation byte-identity requirements

For the same algebra lifted from different source languages, the operand_bindings list MUST be byte-identical in:

- Position arrays (canonical args ordering must match across lifters)
- Symbol strings (when source programs share identifier names, the bindings should match)
- Order of entries (position-sorted)

When source programs use different identifier names (e.g., Rust `safe_divide_then_double` vs Python `safeDivideThenDouble`), the operand_bindings WILL diverge in symbol strings. This is acceptable: operand_bindings is non-hashed; symbol divergence does not break bind CID federation. The realize plugin can use whichever symbol convention is appropriate for its target language.

## Non-goals

- NO string-encoded paths anywhere. Integer arrays only.
- NO parallel-tree mirroring of term_shape. Flat list only.
- NO flat-positionless lists. Explicit positions required.
- NO operand_bindings in the bind payload. Sidecar-only.
- NO normalization of symbol names across languages at the lift stage. Per-language verbatim.

## Cross-references

- `docs/plans/2026-05-16-canonical-term-shape-form.md`: the γ ruling that established bare-`{}` operand slots. This ruling completes γ by providing the non-hashed channel for operand identity.
- `docs/plans/2026-05-16-platform-semantics-via-loss-records.md`: the platform-semantics ruling that established the non-hashed-sidecar architectural pattern this ruling extends.
- `docs/plans/2026-05-16-gamma-postmerge-audit.md`: the audit that surfaced A19/A20/A21/A22; A24's gap surfaced from the seam 3 routing fix's empirical verification (post A14+A15+A16 merge).

## Implementation guidance for A24+A25

### A24 scope (operand-binding-from-context derivation)

- `implementations/rust/provekit-walk/src/bin/walk_rpc.rs`: walk AST in the same pass as term_shape construction; record (position, symbol) tuples for each operand slot; emit operand_bindings on the lift output
- `implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/bind_lifter.py`: same shape on Python side
- `implementations/rust/libprovekit/src/core/lower_plugin.rs`: extend RealizeRequest to carry operand_bindings (non-hashed); thread from lift output through lower request to realize plugin
- `implementations/python/provekit-realize-python-core/src/provekit_realize_python_core/realizer.py`: consume operand_bindings in body synthesis; replace the current positional-fallback logic; implement the completeness gate

### A25 scope (function-name non-hashed sidecar channel)

The function-name sidecar field rides the same non-hashed channel pattern as operand_bindings. Extend RealizeRequest with `source_function_name: Option<String>` (non-hashed); lift kits emit it from AST; realize plugin uses it for the `def`/`fn` emission, falling back to the existing `_provekit_synth` placeholder when absent.

Combined PR: A24 + A25 share the non-hashed-sidecar-on-RealizeRequest architecture. Implement together. Single dispatch.

## Acceptance criteria

When A24+A25 lands:

1. The seam 3 fixture (`safe_divide_then_double`) synthesizes semantically correct Python:
   ```python
   def safe_divide_then_double(num, denom):
       if denom == 0:
           return -1
       else:
           q = num // denom
           if q < 0:
               return -1
           else:
               return q * 2
   ```
2. Federation byte-identity at the algebra layer (bind CID) holds for the same algebra across lifters regardless of source identifier names.
3. The completeness gate refuses synthesis on operand_bindings misalignment with a clear diagnostic.
4. All existing tests stay green.
5. New regression tests cover: positive (correct symbol binding), discrimination (different symbol bindings produce different realized source), structural (nested operand positions resolve correctly).
