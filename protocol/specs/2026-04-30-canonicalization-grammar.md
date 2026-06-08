# Sugar Canonicalization Grammar (v1)

**Date:** 2026-04-30
**Status:** Specification of the deterministic byte sequence that produces `propertyHash` and related content-addressed memento hashes.
**Scope:** The eight-pass pipeline from `IrFormula` to canonical bytes, the JCS-JSON encoding rules applied to the resulting AST, and the BLAKE3-512 self-identifying hash contract. Conformance constraints for any new kit canonicalizer.
**Supersedes (in part):** the serialization-and-hash sections of `2026-04-29-ast-canonicalizer.md`. The structural grammar (canonical FOL AST, de Bruijn, sort/predicate canonicalization, AC, NNF) is reaffirmed; the encoding choice is changed from "CBOR preferred, JSON fallback" to "JCS-JSON locked at v1".
**Sibling spec:** `2026-04-30-ir-formal-grammar.md` describes the *kit-emitted* IR-JSON encoding. That layer is not the canonical layer. This spec begins downstream of it.

## 1. Why this spec exists

Sugar's content addressing only works if every implementation agrees on the
exact bytes hashed for a given logical claim. Today the TypeScript reference
implementation in `src/canonicalizer/` produces certain bytes; the Rust, Go,
and C++ kits agree on the *input-side* IR-JSON encoding (see sibling spec) but
do not yet implement the canonicalizer at all. The cross-language harness in
`scripts/cross-lang-equivalence/` proves IR-JSON parity, not `propertyHash`
parity.

Without a written canonicalization grammar, the existing TypeScript code IS
the spec. A future kit author writing a Rust or Go canonicalizer must
reverse-engineer the rules from the source. This spec promotes the implicit
contract to an explicit one so a second canonicalizer can be written from the
spec alone and produce byte-identical hashes.

## 2. Layering

```
       kit symbolic primitives  (per host language)
                  │
                  ▼
   ─── IR-JSON encoding (sibling spec) ───
   Declaration[] / IrFormula / IrTerm     ← byte-equal across kits today
   ───────────────────────────────────────
                  │
                  │  parse / construct as host data
                  ▼
            ╔═══════════════════════════╗
            ║  canonicalization (THIS)  ║   ← JCS-JSON (RFC 8785), pass 1 → pass 8
            ║  pass 1 ──► pass 8        ║
            ╚═══════════════════════════╝
                  │
                  ▼
            propertyHash  =  16 hex chars
                  │
                  │  wrap in memento envelope (JSON-canonical, per envelope grammar)
                  ▼
            memento body bytes  →  member CID
                  │
                  │  embed bytes (as bstr) in catalog memento; encode envelope as CBOR
                  ▼
   ─── .proof file (sibling spec: 2026-04-30-proof-file-format.md) ───
   Deterministic CBOR (RFC 8949 §4.2.1) container of catalog +
   embedded member bodies. Bytes hash to filename CID = trust root.
   ──────────────────────────────────────────────────────────────────
```

Mementos that carry a `propertyHash` (catalog entries, verdicts, evidence
envelopes) are content-addressed by the output of pass 8. Mementos cross
implementations only when the implementations agree on every pass.

**Scope boundary.** This spec governs the FOL-formula → propertyHash pipeline
(passes 1–8) and locks JCS-JSON as the encoding at pass 7. It does NOT govern:

- The memento envelope encoding around individual mementos. See
  `2026-04-30-memento-envelope-grammar.md` (§Encoding: "JSON, canonicalized").
- The `.proof` envelope used for shipping catalogs of embedded mementos
  as a single binary distribution artifact. See
  `2026-04-30-proof-file-format.md` (deterministic CBOR; bstr-embedded
  member bytes; filename = trust root).

The `.proof` envelope's CBOR layer wraps memento body bytes as opaque
byte strings; the embedded bytes are still produced by this spec's
JCS-JSON encoding and the memento envelope grammar's wrapping rules.
A future migration of pass 7 itself from JCS to CBOR would constitute
a major version bump (§13) and would re-hash every existing memento; it
is unrelated to the `.proof` envelope's CBOR choice, which is a new
layer above the memento body and does NOT change any existing CID.

## 3. Pipeline overview (eight passes)

The canonicalizer is specified as eight ordered passes. Implementations MAY
fuse passes for performance (the TypeScript reference fuses 1+2+3 into a
single walk) provided the externally observable bytes are identical to the
multi-pass form described here.

| Pass | Name | Transformation |
| --- | --- | --- |
| 1 | de Bruijn | Replace named bound variables with de Bruijn indices |
| 2 | Predicate canonicalization | Resolve aliases; sort equality args; flip ordered-comparison args so constants prefer the right |
| 3 | Sort canonicalization | Map host-specific sorts to canonical sort grammar |
| 4 | Implies removal | Rewrite `implies(a, c)` to `or(not(a), c)` |
| 5 | Negation-normal form | Push negations inward via De Morgan and predicate-specific negation |
| 6 | AC normalization | Flatten, sort, deduplicate, identity-remove `and`/`or` |
| 7 | Serialization | Encode the canonical AST as JCS-JSON (RFC 8785) |
| 8 | Hash | `"blake3-512:" + hex(BLAKE3_512(bytes))` |

Passes 1-6 are described in `2026-04-29-ast-canonicalizer.md` and are not
restated in full here; this spec adds the rules that affect the byte form
(field ordering, number encoding, string handling, BV encoding) and locks
encoding (pass 7) and hash (pass 8) at v1.

## 4. The canonical FOL AST (input to pass 7)

```ebnf
CanonicalFolAst    ::= Quantifier | Connective | Atomic

Quantifier         ::= { "kind": ("forall"|"exists"), "sort": Sort, "body": CanonicalFolAst }

Connective         ::= AndNode | OrNode | NotNode

AndNode            ::= { "kind": "and", "operands": [ CanonicalFolAst, ... ] }   /* >= 2 elements after pass 6 */
OrNode             ::= { "kind": "or",  "operands": [ CanonicalFolAst, ... ] }   /* >= 2 elements after pass 6 */
NotNode            ::= { "kind": "not", "body": CanonicalFolAst }                /* body is Atomic after pass 5 */

Atomic             ::= { "kind": "atomic", "predicate": Predicate, "args": [ Term, ... ] }

Term               ::= Var | Const | Ctor

Var                ::= { "kind": "var",   "index": Int>=0,  "sort": Sort }
Const              ::= { "kind": "const", "value": ConstValue, "sort": Sort }
Ctor               ::= { "kind": "ctor",  "name": String, "args": [ Term, ... ], "sort": Sort }

Sort               ::= PrimitiveSort | BitvecSort | SetSort | TupleSort | FunctionSort
PrimitiveSort      ::= { "kind": "primitive", "name": String }
BitvecSort         ::= { "kind": "bitvec",    "width": Int>=1 }
SetSort            ::= { "kind": "set",       "element": Sort }
TupleSort          ::= { "kind": "tuple",     "elements": [ Sort, ... ] }
FunctionSort       ::= { "kind": "function",  "domain": [ Sort, ... ], "range": Sort }

Predicate          ::= String   /* canonical name, see §6 */
ConstValue         ::= Bool | Number | String | Null | BigIntString
BigIntString       ::= "bigint:" SignedDecimalDigits
```

Notes:
- `forall` / `exists` carry no `varName`. Names are erased by pass 1.
- An `implies` node MUST NOT appear post-pass-4. Producers writing canonical
  ASTs by hand (rare) MUST run pass 4.
- A `not` wrapping a non-atomic node MUST NOT appear post-pass-5 for any
  predicate listed in §6 under "negatable predicates". For unknown
  (kit-defined) predicates, `not(atomic)` is the canonical form.
- `and` / `or` MUST contain at least two operands post-pass-6. One-operand or
  zero-operand forms are normalized away (see §7).
- Terms inside `Var.sort` and `Const.sort` and `Ctor.sort` MUST be the
  canonical sort form (§5).

## 5. Sort grammar

Canonical primitive sort names form a fixed set:

```
"Bool"    "Int"     "Real"     "String"   "Ref"
"Node"    "Edge"    "Region"   "Time"
```

Kit-defined extension primitive sorts MUST use a `<kit-namespace>:<cid>`
suffix on the `name` field (e.g. `"rust-kit:Lifetime@bafy..."`). Standard
sort names MUST NOT be redefined by kits.

Bitvector sorts use the dedicated `bitvec` discriminant carrying an integer
`width >= 1`. Unsigned and signed BV are distinguished by predicate, not by
sort: a single `bitvec` sort is consumed by both `bvult` and `bvslt`.

Set, tuple, and function sorts recursively contain canonical sorts. Tuple
elements and function domain elements are positional and MUST NOT be sorted.

## 6. Predicate grammar

Standard predicate names are atomic Unicode strings:

```
"="       /* equality (any sort) */
"≠"       /* inequality (U+2260) */
"<"       /* less-than (Ordered sort) */
"≤"       /* less-than-or-equal (U+2264) */
">"
"≥"       /* greater-than-or-equal (U+2265) */
"true"    /* nullary truth */
"false"   /* nullary falsity */
"member"  /* x in S */
"subset"  /* A subset-of-or-equal-to B */
"kind-of"           /* SAST: node has kind */
"data-flows-to"     /* SAST: data flow */
"dominates"         /* SAST: dominance */
"post-dominates"    /* SAST: post-dominance */
"on-path"           /* SAST: on path between two nodes */
"transition-from-to"/* temporal: state transition */
"bvult" "bvule" "bvugt" "bvuge"     /* unsigned BV comparisons */
"bvslt" "bvsle" "bvsgt" "bvsge"     /* signed BV comparisons */
```

The Unicode predicates `≠` (U+2260), `≤` (U+2264), and `≥` (U+2265) appear
verbatim in the JSON output as their UTF-8 byte sequences. They are NOT
escaped to `≠` etc. (see §10).

**Alias resolution (pass 2).** Host aliases canonicalize to the standard
names. The full alias table is normative:

| Alias | Canonical |
| --- | --- |
| `==`, `eq`, `equal` | `=` |
| `!=`, `notEqual`, `not-equal`, `ne` | `≠` |
| `lt`, `lessThan`, `less-than` | `<` |
| `lte`, `le`, `lessThanOrEqual`, `less-than-or-equal` | `≤` |
| `gt`, `greaterThan`, `greater-than` | `>` |
| `gte`, `ge`, `greaterThanOrEqual`, `greater-than-or-equal` | `≥` |
| `∈` (U+2208), `in` | `member` |
| `⊆` (U+2286), `subseteq` | `subset` |
| `kindOf`, `kind_of` | `kind-of` |
| `dataFlowsTo`, `data_flows_to` | `data-flows-to` |
| `postDominates`, `post_dominates` | `post-dominates` |
| `onPath`, `on_path` | `on-path` |
| `transitionFromTo`, `transition_from_to` | `transition-from-to` |

Kit-defined predicates pass through unchanged. They MUST contain a `:` to
disambiguate from typos of standard names.

**Argument normalization (pass 2).** For two-argument atomics:

- `=` and `≠` sort their arguments so `args[0]` has the lexicographically
  smaller `termSortKey` (see §6a). This collapses `=(a, b)` and `=(b, a)`
  to the same form.
- `<`, `≤`, `>`, `≥` apply "constants prefer the right": if `args[0]` is a
  `const` and `args[1]` is not, the predicate is replaced by its mirror
  (`<` flips with `>`; `≤` flips with `≥`) and arguments are swapped.
  After this rule, whenever an ordered comparison has exactly one constant
  operand, that constant is `args[1]`.

**Negatable predicates (pass 5).** During NNF, `not(atomic)` rewrites to a
naked atomic for these predicates only:

```
not(=(a, b))   ->  ≠(a, b)
not(≠(a, b))   ->  =(a, b)
not(<(a, b))   ->  ≥(a, b)
not(≤(a, b))   ->  >(a, b)
not(>(a, b))   ->  ≤(a, b)
not(≥(a, b))   ->  <(a, b)
not(true())    ->  false()
not(false())   ->  true()
```

For all other predicates (kit-defined, SAST, BV, `member`, `subset`), `not`
remains wrapped around the atomic. The `not` node IS the canonical form
when the inner predicate is not negatable.

### 6a. termSortKey (used by pass 2 equality sort, pass 6 AC sort)

`termSortKey` is the deterministic structural key applied for sorting. It
operates on canonical terms and canonical sorts. The grammar is:

```
termSortKey(Var{index, sort})       =  "var:"   <index> ":" sortKey(sort)
termSortKey(Const{value, sort})     =  "const:" sortKey(sort) ":" stringifyConst(value)
termSortKey(Ctor{name, args, sort}) =  "ctor:"  <name> ":" join(",", map(termSortKey, args))

sortKey(Primitive{name})            =  "P:" <name>
sortKey(Bitvec{width})              =  "BV:" <width>
sortKey(Set{element})               =  "S:" sortKey(element)
sortKey(Tuple{elements})            =  "T:" join(",", map(sortKey, elements))
sortKey(Function{domain, range})    =  "F:" join(",", map(sortKey, domain)) ":" sortKey(range)

stringifyConst(BigInt n)            =  "\"bigint:" <n.toString()> "\""
stringifyConst(other)               =  JSON.stringify(other)        /* per ECMA-262 */
```

Strings produced by `termSortKey` and `sortKey` are compared
lexicographically by Unicode code-point order, identical to JCS object-key
ordering (§7.3). The result of comparison MUST NOT depend on locale.

## 7. JCS-JSON encoding (pass 7)

The canonical AST is serialized as **canonical JSON per RFC 8785** (JSON
Canonicalization Scheme, JCS). This choice is the v1 lock. The 2026-04-29
ast-canonicalizer spec named CBOR (RFC 8949 §4.2) as preferred; the as-built
TypeScript implementation ships JCS, exposed via the constant
`SERIALIZATION_FORMAT = "jcs-json-rfc8785"`. Mementos produced under JCS
are NOT cross-comparable with hypothetical CBOR-encoded mementos for the
same logical claim. A future migration to CBOR would constitute a major
version bump (§13).

### 7.1. UTF-8

Output bytes are **UTF-8** (no BOM, no leading whitespace, no trailing
whitespace).

### 7.2. No HTML escaping

The serializer MUST NOT replace `<`, `>`, `&`, or U+2028 / U+2029 with
`\uXXXX` escapes. JCS §3.2.2.2 escapes only the JSON-required set:
backslash, double-quote, control characters U+0000..U+001F. The Go kit's
IR-JSON layer enforces this with `SetEscapeHTML(false)`; the canonical
layer enforces it by hand-rolled string output.

### 7.3. Object key ordering

Object members are emitted with keys sorted by **Unicode code-point order**
(equivalent to byte-order on UTF-16 surrogate-pair-aware code-point
comparison; for the keys produced by this spec, all keys are ASCII so plain
byte comparison suffices). This is RFC 8785 §3.2.3.

The canonical AST does not nest the `kind` discriminator first; *all* keys
are emitted in lex-sorted order. For example, an `atomic` node:

```json
{"args":[...],"kind":"atomic","predicate":"="}
```

Note `args` < `kind` < `predicate` lexicographically. The kit-emitted
IR-JSON layer pins `kind` first; the canonical layer does NOT. They are
different encodings.

### 7.4. Compact form

There is **no whitespace** between tokens. No spaces, no newlines, no
indentation. Member separators are `:` (single byte) and `,` (single byte).

### 7.5. String escaping

The serializer escapes the JSON-required minimal set per RFC 8785 §3.2.2.2:

```
"  -> \"
\  -> \\
control U+0000..U+001F  -> \uXXXX (lowercase hex)
all other code points   -> verbatim UTF-8 bytes
```

Specifically: `≥` (U+2265) survives canonicalization as the three UTF-8
bytes `0xE2 0x89 0xA5`. The serializer MUST NOT emit `≥`. The Unicode
predicates and any user-supplied strings (variable-bound constants of sort
`String`, kit predicate names containing non-ASCII) follow the same rule.

### 7.6. Numbers

The number rule has two layers, both normative:

**Layer A: what value the AST holds (pass 2 normalizes here).**

The IR-formula's `Const.value` lifts to one of:

- `boolean` (Bool sort)
- `number` (JS number; Int or Real sort)
- `bigint` (out-of-safe-integer Int constants; also BV constants, see §8)
- `string` (String sort)
- `null` (Ref-typed null literal)

JS has a single numeric type. To preserve byte equivalence with kits that
distinguish int and real (Rust, Go, C++):

- `Const{sort: Int, value: 3.0}` MUST serialize as `3`, not `3.0`. The
  Rust kit enforces this at the IR-emission layer (commit `2d025d7`); the
  canonical layer inherits that input. An implementation in a host with
  separate int/float types follows host conventions for integer-valued
  reals.
- `Const{sort: Real, value: 3.0}` likewise serializes as `3` under the
  current JS-driven rules (V8's `JSON.stringify(3.0)` produces `"3"`).
  The Real-vs-Int distinction is preserved by the `sort` field, not by
  the textual form of `value`. A consumer that wishes to recover the
  Real intent reads `sort.name == "Real"`.

**Layer B: how a number is rendered to bytes (RFC 8785 §3.2.2.3).**

For finite IEEE-754 doubles, the rendering is the algorithm specified in
ECMA-262 §7.1.12.1 (`Number::toString`, including Note 2 which collapses
`-0` to `"0"`). The TypeScript reference uses Node/V8's `JSON.stringify(n)`
verbatim, which RFC 8785 cites as the reference implementation. Conformance
is pinned in `equivalence.test.ts` §11 against all 24 fixtures from RFC
8785 Appendix B.

`-0` MUST normalize to `"0"`. `NaN` and `±Infinity` are not permitted in
canonical JSON; the serializer MUST throw or otherwise reject them
(RFC 8785 §3.2.2.3 final paragraph).

A kit implementing this spec in a host language other than JS MUST NOT
delegate number rendering to the host's `n.toString()` without a
§3.2.2.3 conformance suite. Go's `strconv.FormatFloat` with `'f'/'e'`
modes, Rust's `format!("{}", n)`, and Java's `Double.toString` all drift
from V8 on edge cases. The conformance test in `equivalence.test.ts §11`
is the gate; a new kit's number renderer passes when all Appendix B
fixtures match.

### 7.7. BigInts

A `bigint` value in `Const.value` serializes by safe range:

- `n` in `[Number.MIN_SAFE_INTEGER, Number.MAX_SAFE_INTEGER]` (i.e.
  fits in IEEE-754 53-bit mantissa exactly): emit as a JSON Number,
  using the same §3.2.2.3 rendering as a JS `number`.
- Otherwise: emit as a JSON String with the prefix `bigint:` and the
  base-10 signed-digit representation of the integer
  (e.g. `"bigint:340282366920938463463374607431768211456"`).

This convention is TS-specific today; a kit whose host has native
arbitrary-precision integers (Rust `BigInt`, Go `math/big.Int`) MUST
follow the same rule for cross-kit compatibility. A future revision MAY
unify the rule on a single textual form for all integer constants
regardless of magnitude (open question; see §13).

### 7.8. Booleans, null

- `true` -> `true`
- `false` -> `false`
- `null` -> `null`

No quotes, no escaping.

### 7.9. Arrays

Array element order is preserved verbatim. `and.operands` and
`or.operands` are sorted by pass 6 (§7 of `2026-04-29-ast-canonicalizer.md`,
restated below); pass 7 does NOT re-sort them. `tuple.elements`,
`function.domain`, `Atomic.args`, and `Ctor.args` are positional and
emitted in source order.

## 8. Bitvector encoding

Bitvector constants flow through `Const{value: bigint, sort: {kind:"bitvec", width: w}}`.
Pass 2 normalizes the value into `[0, 2^w)` (modular reduction). Negative
inputs become their two's-complement bit pattern: `bv(-1n, 8)` and
`bv(255n, 8)` produce identical canonical IR. This matches SMT-LIB
convention.

The serialized form follows §7.6 / §7.7: in-safe-range values render as
JSON numbers; out-of-safe-range values (any width >= 54 with a high-order
bit set) render as `"bigint:<digits>"`. Width is preserved in
`sort.width` and is the discriminator for BV-vs-Int operations.

BV ctors (`bvadd`, `bvxor`, etc.) appear as `Ctor` nodes whose `name` is
the SMT-LIB operator and whose `sort` is the result BV sort. BV
comparison predicates (`bvult`, `bvslt`, etc.) are atomic predicates per
§6 and do NOT undergo the ordered-comparison flip rule of pass 2 (the
flip rule applies to `<`, `≤`, `>`, `≥` only; BV comparisons are
distinct predicates).

## 9. Quantifier variable naming

The IR-emission layer generates fresh variable names from a thread-local
counter producing `_x0`, `_x1`, `_x2`, ... (see `src/ir/quantifiers.ts`).
The counter resets per `_resetCollector()` call.

This counter is **not visible** at the canonical layer. Pass 1 erases all
variable names and replaces them with de Bruijn indices. The
`propertyHash` is invariant under any choice of fresh-name generator; two
implementations using different counters (or no counter at all, e.g. a
language with hygienic gensym) produce identical `propertyHash`.

The counter's reset semantics matter only for memento CIDs that hash
**raw IR-JSON** (per `src/ir/symbolic/property.ts:73-83`): without the
reset, the second run of the same code emits `_x2, _x3, ...` instead of
`_x0, _x1, ...`, the IR-JSON differs, and the IR-JSON-CID drifts even
though the canonical `propertyHash` is stable. Implementations of the
canonicalization grammar do not need to track or reset the counter; that
concern lives in the IR-emission layer.

## 10. Determinism contract

For any two implementations conforming to this spec, the following MUST
hold for every logically equivalent input:

1. **Structural equivalence.** The output of pass 6 (canonical AST) is
   structurally identical: same node kinds, same field values, same
   array element order.
2. **Byte equivalence.** The output of pass 7 (JCS-JSON bytes) is
   byte-identical (same UTF-8 encoding, same length, same content).
3. **Hash equivalence.** The output of pass 8 is identical: the same
   16-character lowercase hex string.

Two formulas are *logically equivalent* (for the purposes of (1)-(3)) iff
they would canonicalize identically under a single conforming
implementation. The framework enforces this by running both formulas
through the same canonicalizer and comparing.

The contract does NOT extend to pre-pass-1 host-data structures. A Rust
struct and a Go struct expressing the same claim need not have identical
in-memory representations; only the post-pass-7 bytes must match.

## 11. Hash construction

```
canonicalAst   =  passes_1_through_6(IrFormula)
canonicalBytes =  jcs_json_serialize(canonicalAst)             /* pass 7 */
propertyHash   =  "blake3-512:" + hex(BLAKE3_512(canonicalBytes))           /* pass 8 */
```

`propertyHash` is a 16-character lowercase hexadecimal string (64 bits of
entropy). The full 512 bits of BLAKE3 are required for the corpus
scales the framework operates at; high-assurance deployments MAY adopt
the reserved 32-character form (`"blake3-512:" + hex(BLAKE3_512(canonicalBytes))`) at
spec major bump.

## 12. Conformance

Today's reality: only the TypeScript canonicalizer at
`src/canonicalizer/` implements this spec. The kits in `implementations/rust/`,
`implementations/go/`, `implementations/cpp/` implement the *IR-emission* layer (sibling spec)
but not the canonical layer. The cross-language harness in
`scripts/cross-lang-equivalence/` validates IR-JSON parity, NOT
`propertyHash` parity.

A new kit canonicalizer is conformant when:

1. **Pass-by-pass parity.** For each pass (1-6), the implementation
   produces the same canonical AST as the TypeScript reference for every
   IR-formula in the conformance corpus.
2. **JCS conformance.** The serializer passes all 24 fixtures from
   RFC 8785 Appendix B, "Number Serialization Samples", *exactly* in the
   number-rendering shape required by §7.6 Layer B. The TypeScript
   conformance pins live at `src/canonicalizer/equivalence.test.ts §11`.
3. **Cross-language `propertyHash` parity.** Every fixture in the
   canonical-layer test corpus produces the same 16-character hex
   `propertyHash` as the TypeScript reference.

The conformance corpus extends today's IR-JSON harness with a
canonical-layer column. Proposed new fixtures (open question; see §13):

- **alpha-equivalence.** `forAll(b => P(b))` and `forAll(x => P(x))` must
  hash to the same value.
- **AC reordering.** `and(p, q)` and `and(q, p)` must hash to the same value.
- **De Morgan collapse.** `not(and(p, q))` and `or(not(p), not(q))` must
  hash to the same value.
- **Equality argument sort.** `=(a, b)` and `=(b, a)` must hash to the
  same value.
- **Implies removal.** `implies(a, c)` and `or(not(a), c)` must hash to
  the same value.
- **Constants prefer right.** `lt(num(5), x)` and `gt(x, num(5))` must
  hash to the same value.
- **Unicode identifier round-trip.** A predicate name containing `≥`,
  `α`, or U+10437 (Deseret letter, beyond the BMP) round-trips through
  pass 7 unchanged.
- **Negative cases.** `forAll<Int>(P)` and `forAll<Real>(P)` must hash to
  *different* values; `=(a, b)` over Int and `=(a, b)` over Bool must
  hash to *different* values.
- **Deeply nested quantifiers.** `forall x. exists y. forall z. P(x,y,z)`
  with three levels of de Bruijn indexing.
- **Empty conjunction / disjunction.** `and()` (after AC normalization)
  collapses to `true()`; `or()` collapses to `false()`.
- **BV.** `forall<BV<32>>(x => bveq(bvxor(x, x), bv(0n, 32)))` is the
  canonical BV smoke test (already specified in
  `src/ir/symbolic/bv-cross-lang-fixture.test.ts`); a canonical-layer
  golden hash MUST be added when the first non-TS BV-aware canonicalizer
  ships.
- **Out-of-safe-range BigInt.** `Const{value: 2n**128n, sort: Int}` must
  serialize as the `bigint:` string-prefixed form in §7.7.

## 13. Versioning

This spec is **canonicalization grammar v1**. Mementos referencing v1 are
cross-comparable across all conforming implementations of v1.

Conditions that constitute a major version bump:

- Switching from JCS-JSON to CBOR (or any other encoding) for pass 7.
- Changing the BLAKE3-512 self-identifying hash contract (e.g. switching from BLAKE3 to a different algorithm or
  to a longer prefix).
- Changing the de Bruijn convention (e.g. from "innermost binder is
  index 0" to "outermost binder is index 0").
- Changing the AC-sort key (`termSortKey` / `astSortKey`) in any way
  that reorders operands.
- Changing the alias table in §6 such that a host expression that
  previously canonicalized one way now canonicalizes another.

Conditions that constitute a minor version bump:

- Adding a new standard predicate name (kits that don't recognize it
  pass through; older mementos remain comparable across older versions).
- Adding a new primitive sort name.

Conditions that constitute a patch version bump:

- Bug fixes that do NOT change canonical bytes for valid input (e.g.
  better error messages on malformed input).

A canonicalizer declares its `specVersion()` in its public interface
(see `AstCanonicalizerImpl.specVersion()` in
`src/canonicalizer/index.ts`). Consumers MUST verify compatible major
versions before composing mementos.

## 14. Open questions

These are flagged for follow-up; they are NOT settled by this spec.

- **CBOR migration (pass 7).** The 2026-04-29 spec preferred CBOR for
  individual memento bodies; the as-implemented v1 ships JCS for that
  layer. Migrating *pass 7* from JCS to CBOR would constitute a major
  version bump and would re-hash every existing memento. NOTE: the
  separate `.proof` envelope format (`2026-04-30-proof-file-format.md`)
  uses deterministic CBOR for its container, which is a new layer above
  memento bodies and does NOT settle this question. Bodies stay JCS at
  v1; a future v2 of THIS spec could switch them to CBOR independently
  of the envelope decision.
- **BigInt unification.** The split rule in §7.7 (in-safe-range as
  Number, out-of-safe-range as `"bigint:N"` string) is JS-specific. A
  unified rule (e.g. all integer constants serialize as canonical
  decimal strings, regardless of magnitude) would simplify cross-kit
  semantics. Cost: every existing memento re-hashes; a major bump.
- **Pass interleaving.** The TS reference fuses passes 1+2+3+pre-NNF.
  Implementations are free to do the same. The spec lists passes as
  separate stages for clarity; nothing requires them to be physically
  separate. This is documented as informative, not normative.
- **`bindingHash` canonical bytes.** The 2026-04-29 spec defines
  `bindingHash = "blake3-512:" + hex(BLAKE3_512(canonical-scope-bytes))` over a different
  data shape (`CanonicalScope` rather than `CanonicalFolAst`). The
  encoding rules in this spec (UTF-8, JCS object-key ordering, no HTML
  escaping, RFC 8785 §3.2.2.3 numbers) apply identically. A separate
  `bindingHash` spec is in scope for follow-up work.
- **Conformance corpus location.** Today's harness lives at
  `scripts/cross-lang-equivalence/` and tests IR-JSON parity. A
  canonical-layer harness with golden `propertyHash` values is needed.
  Proposed location: `scripts/cross-lang-canonical/` with parallel
  fixture/golden files.
- **Negative-test cardinality.** §12 lists positive equivalence
  fixtures and a few negative cases. A more thorough negative corpus
  (every pair of nodes that look similar but should hash differently)
  is needed; bounded by useful coverage rather than exhaustiveness.

## 15. Cross-references

- `2026-04-29-ast-canonicalizer.md`: structural canonicalization (passes 1-6).
  This spec inherits §"The canonical FOL AST", §"Bound-variable handling",
  §"Sort canonicalization", §"Predicate canonicalization", §"AC normalization",
  §"Negation-normal form" verbatim.
- `2026-04-30-ir-formal-grammar.md`: kit-emitted IR-JSON encoding (the input
  side, before pass 1).
- `2026-04-29-the-semantic-envelope.md`: how `propertyHash` flows into
  envelope mementos.
- `2026-04-29-supply-chain-via-semantic-envelope.md`: how matching
  `propertyHash` values across implementations enable supply-chain
  composition.
- `src/canonicalizer/serialize.ts`: TypeScript reference serializer.
- `src/canonicalizer/equivalence.test.ts §11`: RFC 8785 Appendix B
  conformance pins.
- `scripts/cross-lang-equivalence/`: current IR-JSON parity harness.
