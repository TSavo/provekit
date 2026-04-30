# ProvekIt IR — Formal Grammar (External JSON Encoding)

**Date:** 2026-04-30
**Status:** Specification + reference parser, paired with `src/ir/grammar/parse.ts`
**Scope:** The textual JSON encoding emitted by every ProvekIt IR symbolic-primitives kit (TypeScript, Rust, Go, C++) when serializing a `Declaration[]`.

## What this document specifies

This grammar is the formal description of the **kit-emitted IR JSON** — the
textual form every kit produces from its `_resetCollector() / beginCollecting() /
property(...) / finish()` cycle. Today, four kits emit byte-identical JSON for
the same logical claim; this is enforced by the regression harness in
`scripts/cross-lang-equivalence/`. Until now there has been no formal grammar:
the contract was "whatever the kits all happen to emit."

This document promotes that implicit contract to an explicit one.

### What this is *not*

This grammar is **not** the canonical form used to compute `propertyHash`. The
canonicalizer (`src/canonicalizer/`) consumes IR values, runs them through eight
passes (de Bruijn, sort/predicate canonicalization, implies removal, NNF, AC
normalization), and then serializes the result with JCS (RFC 8785) before
hashing. The JCS form sorts object keys lexicographically; the kit-emitted form
locked here uses **insertion-order** keys (a fixed order per node kind). They
are different encodings at different layers.

```
        kit symbolic primitives (per language)
                       │
                       ▼
   ─────────  this grammar  ─────────
   kit-emitted IR JSON (Declaration[])    ← byte-equal across kits
   ──────────────────────────────────
                       │
                       ▼
            canonicalizer (passes 1..6)
                       │
                       ▼
              CanonicalFolAst
                       │
                       ▼
          JCS serialize (pass 7, RFC 8785)
                       │
                       ▼
            sha256-prefix-16 (pass 8)  =  propertyHash
```

The grammar below describes only the top arrow. The canonicalizer pipeline is
described elsewhere and is not affected by anything in this document.

## Notation

The grammar is written in EBNF with the following conventions:

- Terminals are quoted JSON literals (e.g. `"\"kind\""`).
- A literal `,` between productions denotes the JSON comma separator with
  exactly one byte (`U+002C`) and **no** surrounding whitespace.
- A literal `:` between key and value denotes the JSON name-separator with
  exactly one byte (`U+003A`) and **no** surrounding whitespace.
- `[` `]` denote JSON-array brackets; `{` `}` denote JSON-object brackets.
- `String`, `Number`, `Bool`, `Null` denote the corresponding JSON tokens (per
  RFC 8259).
- `*` means zero-or-more; `?` means optional; `|` means alternation.
- Whitespace between tokens is **not permitted** in conformant output. The
  emitted form is the compact, no-whitespace form `JSON.stringify(value)`
  produces with no `space` argument.

The grammar is *deterministic* (one parse tree per valid input) and
*reverse-deterministic* (one canonical text per valid IR value), provided the
emitter respects the locked key order specified for each node kind.

## Top-level production

```ebnf
Document    ::= "[" ( Declaration ( "," Declaration )* )? "]"

Declaration ::= ContractDeclaration
              | BridgeDeclaration
```

A document is a JSON array of declarations. Empty (`[]`) is valid.

## Declarations

### ContractDeclaration

Locked key order: `kind`, `name`, `outBinding`, `pre`, `post`, `inv`.
The `pre`, `post`, and `inv` fields are each optional but at least
one MUST be present. When present, each is an `IrFormula`. When
absent, the entire key is omitted (never emitted as `null` —
matches the JCS canonicalization rule "omit absent keys"). The
`outBinding` field is REQUIRED and names the free variable that
`post` uses to refer to the function's return value
(conventionally `"out"`).

```ebnf
ContractDeclaration ::= "{"
                          "\"kind\"" ":" "\"contract\"" ","
                          "\"name\"" ":" String ","
                          "\"outBinding\"" ":" String
                          ( "," "\"pre\"" ":" IrFormula )?
                          ( "," "\"post\"" ":" IrFormula )?
                          ( "," "\"inv\"" ":" IrFormula )?
                        "}"
```

The `post` formula's body MAY contain free occurrences of a
variable whose `name` equals `outBinding`. The verifier substitutes
the call expression's symbolic output for that variable at use
sites (per the handshake algorithm spec). All other free variables
in any of `pre`/`post`/`inv` are quantified by an enclosing
`forall` whose `varName` matches the function's parameter name; a
ContractDeclaration whose `pre`/`post`/`inv` contains a free
variable that is neither `outBinding` nor a parameter is malformed.

### BridgeDeclaration

Locked key order: `kind`, `name`, `sourceSymbol`, `sourceLayer`,
`targetContractCid`, `targetLayer`, `notes` (optional, omitted when absent).

```ebnf
BridgeDeclaration ::= "{"
                        "\"kind\"" ":" "\"bridge\"" ","
                        "\"name\"" ":" String ","
                        "\"sourceSymbol\"" ":" String ","
                        "\"sourceLayer\"" ":" String ","
                        "\"targetContractCid\"" ":" String ","
                        "\"targetLayer\"" ":" String
                        ( "," "\"notes\"" ":" String )?
                      "}"
```

The `notes` field is **omitted entirely** when undefined; it is never emitted
as `null`. (Rationale: the TS kit destructures `...(spec.notes !== undefined ? { notes } : {})`;
the Rust kit declares `notes: Option<String>` with `serde(skip_serializing_if = "Option::is_none")`.
This rule is what keeps the four kits byte-equal when bridges have no notes.)

## Formulas

### IrFormula

```ebnf
IrFormula ::= QuantifierFormula
            | ConnectiveFormula
            | AtomicFormula
```

The `kind` field is the discriminator for every formula and term node. It is
always the first key.

The maximal-uniformity rule for the IR: every node has `kind`, then `name`
(when applicable), then payload (`sort` / `body` / `args` / `operands` /
`value`). There is no `varName` (variable names use `name`); there is no
`conjuncts` / `disjuncts` / `antecedent` / `consequent` (boolean connectives
use `operands`); there is no `lambda` wrapper around a quantifier's body
(the quantifier carries its bound variable directly). The reader holds the
entire IR in their head.

### QuantifierFormula

Locked key order: `kind`, `name`, `sort`, `body`.

```ebnf
QuantifierFormula ::= "{"
                        "\"kind\"" ":" QuantifierKind ","
                        "\"name\"" ":" String ","
                        "\"sort\"" ":" Sort ","
                        "\"body\"" ":" IrFormula
                      "}"

QuantifierKind ::= "\"forall\"" | "\"exists\""
```

The `name` field is the bound variable's identifier. References to this
variable inside `body` are `VarTerm` nodes whose `name` matches.

### ConnectiveFormula

Locked key order: `kind`, `operands`.

```ebnf
ConnectiveFormula ::= "{"
                        "\"kind\"" ":" ConnectiveKind ","
                        "\"operands\"" ":" "[" IrFormula ( "," IrFormula )* "]"
                      "}"

ConnectiveKind ::= "\"and\"" | "\"or\"" | "\"not\"" | "\"implies\""
```

**Arity rules** (post-grammar):

- `not` MUST have exactly 1 operand.
- `implies` MUST have exactly 2 operands; `operands[0]` is the antecedent,
  `operands[1]` the consequent.
- `and` and `or` MUST have 2 or more operands. Empty/singleton `and`/`or` is
  not a valid IR shape; the canonicalizer's AC pass produces 2+ operands or
  collapses to a non-connective form.

Validators reject ConnectiveFormula nodes with arity violations.

### AtomicFormula

Locked key order: `kind`, `name`, `args`.

```ebnf
AtomicFormula ::= "{"
                    "\"kind\"" ":" "\"atomic\"" ","
                    "\"name\"" ":" String ","
                    "\"args\"" ":" "[" ( IrTerm ( "," IrTerm )* )? "]"
                  "}"

AtomicName ::= "\"=\"" | "\"≠\"" | "\"<\"" | "\"≤\""
             | "\">\"" | "\"≥\""
             | "\"true\"" | "\"false\""
             | "\"subset\"" | "\"member\""
             | "\"kind-of\"" | "\"data-flows-to\""
             | "\"dominates\"" | "\"post-dominates\""
             | "\"transition-from-to\"" | "\"on-path\""
             | "\"bvult\"" | "\"bvule\"" | "\"bvugt\"" | "\"bvuge\""
             | "\"bvslt\"" | "\"bvsle\"" | "\"bvsgt\"" | "\"bvsge\""
             | KitDefinedAtomicName
```

`KitDefinedAtomicName` is any String that does not collide with a built-in
atomic name. The parser does **not** reject unknown names: kits may define
new atomic predicates without rev-locking the parser. (Strict mode is
offered as a parser option; see "Strict mode" below.)

The use of `name` (not `predicate`) for the atomic's identifier matches
every other named node in the IR. The kind discriminator (`"atomic"`) carries
the information that this `name` is an atomic-predicate name; no separate
field key is needed to communicate that.

## Terms

### IrTerm

```ebnf
IrTerm ::= VarTerm | ConstTerm | CtorTerm
```

### VarTerm

Locked key order: `kind`, `name`.

```ebnf
VarTerm ::= "{"
              "\"kind\"" ":" "\"var\"" ","
              "\"name\"" ":" String
            "}"
```

A `VarTerm` carries no sort. The variable's sort is determined by the
enclosing `QuantifierFormula` whose `name` matches, or — for free variables
introduced by a contract memento's `outBinding` — by the substitution rule
at call sites (the substituted expression's sort). Producers MUST NOT add a
`sort` field; validators MUST reject `VarTerm`s with extra fields.

### ConstTerm

Locked key order: `kind`, `value`, `sort`.

```ebnf
ConstTerm ::= "{"
                "\"kind\"" ":" "\"const\"" ","
                "\"value\"" ":" ConstValue ","
                "\"sort\"" ":" Sort
              "}"

ConstValue ::= Number | String | Bool | Null
```

A `ConstTerm` is the only term kind that carries `sort`: the literal value's
type is not derivable from binding scope or signature. `Number`, `String`,
`Bool`, and `Null` are the permitted JSON value shapes; the `sort` field
disambiguates (e.g. `42` could be `Int` or `Real`).

Bigint values that exceed JavaScript's safe integer range MAY be emitted as
a JSON Number (current TS behavior) or as a String with prefix
`"bigint:<digits>"` (canonicalizer's convention). Parsers MUST accept either
shape.

### CtorTerm

Locked key order: `kind`, `name`, `args`.

```ebnf
CtorTerm ::= "{"
               "\"kind\"" ":" "\"ctor\"" ","
               "\"name\"" ":" String ","
               "\"args\"" ":" "[" ( IrTerm ( "," IrTerm )* )? "]"
             "}"
```

A `CtorTerm` carries no sort. The ctor's return sort is determined by its
declaration in a kit's bridge or extension memento (`irReturnSort` field).
Producers MUST NOT add a `sort` field; validators MUST reject `CtorTerm`s
with extra fields. Two `CtorTerm` nodes with the same `name` and `args`
must hash identically regardless of where they appear; carrying a `sort`
field would make textually-equal ctor invocations hash differently in
different scopes, which defeats the canonicalization promise.

`args` MAY be empty (a nullary constructor like `parseInt()` taking no
arguments — uncommon but permitted by the IR types).

## Sorts

### Sort

```ebnf
Sort ::= PrimitiveSort | BitvecSort | SetSort | TupleSort | FunctionSort
```

### PrimitiveSort

Locked key order: `kind`, `name`.

```ebnf
PrimitiveSort ::= "{"
                    "\"kind\"" ":" "\"primitive\"" ","
                    "\"name\"" ":" String
                  "}"
```

The grammar allows any String as a primitive sort name. The canonical built-in
names are `"Bool"`, `"Int"`, `"Real"`, `"String"`, `"Ref"`, `"Node"`, `"Edge"`,
`"Region"`, `"Time"`. Kit-defined extensions (e.g. `"Address"`) are accepted
in non-strict mode.

### BitvecSort

Locked key order: `kind`, `width`.

```ebnf
BitvecSort ::= "{"
                 "\"kind\"" ":" "\"bitvec\"" ","
                 "\"width\"" ":" PositiveInteger
               "}"

PositiveInteger ::= Number  /* must be a positive integer ≤ 2^53 - 1 */
```

### SetSort

Locked key order: `kind`, `element`.

```ebnf
SetSort ::= "{"
              "\"kind\"" ":" "\"set\"" ","
              "\"element\"" ":" Sort
            "}"
```

### TupleSort

Locked key order: `kind`, `elements`.

```ebnf
TupleSort ::= "{"
                "\"kind\"" ":" "\"tuple\"" ","
                "\"elements\"" ":" "[" ( Sort ( "," Sort )* )? "]"
              "}"
```

### FunctionSort

Locked key order: `kind`, `domain`, `range`.

```ebnf
FunctionSort ::= "{"
                   "\"kind\"" ":" "\"function\"" ","
                   "\"domain\"" ":" "[" ( Sort ( "," Sort )* )? "]" ","
                   "\"range\"" ":" Sort
                 "}"
```

## Determinism rules

These are global constraints that apply to all productions above. They are
what makes the grammar **byte-deterministic**.

1. **Key order is fixed per node kind.** The grammar above lists keys in their
   emitted order. Emitters MUST produce keys in this order. Parsers SHOULD
   accept any key order during ingest; conformant emitters never produce a
   reorder. (See "Strict mode" for a parser option that enforces emit order.)

2. **No whitespace.** No spaces, tabs, or newlines between tokens. JSON
   permits whitespace; the kit-emit form does not.

3. **No trailing commas.** Standard JSON.

4. **Numbers in canonical JSON form.** Integers serialize without a fractional
   part; doubles use V8's `Number.prototype.toString` rendering (the same one
   the canonicalizer's pass 7 relies on). NaN and ±Infinity are not permitted
   in any IR value and the parser MUST reject them.

   *Note on parser-side number normalization.* `JSON.parse` silently
   normalizes some non-canonical number forms (e.g. `1.0` becomes the same
   in-memory `1` as `1`). Hand-crafted JSON containing a non-canonical
   numeric form will parse, but its re-emit will use the canonical form, so
   non-canonical input does NOT round-trip byte-identically. This is fine
   for kit-emitted input (the kits always emit canonical numbers) and is a
   documented divergence between "what the grammar accepts" and "what the
   round-trip property guarantees."

5. **String escaping is JSON-standard.** No unnecessary escapes; no `\/`
   solidus escape; non-ASCII characters MAY be emitted literally (UTF-8) or as
   `\uXXXX` escapes — kits are not required to agree on this beyond what their
   stdlib serializers produce. The fixtures currently used round-trip
   identically across kits with literal UTF-8 (the `≠`, `≤`, `≥` predicate
   names appear as raw three-byte UTF-8 sequences in the emitted JSON).

6. **Closed objects.** No node kind admits "extra" keys beyond those listed
   in its production. Parsers MUST reject documents containing unknown keys
   on a known node kind. (This is what makes the grammar tight; without it,
   kits could drift by silently emitting trailing fields.)

## Reference parser

The reference parser lives at `src/ir/grammar/parse.ts`. It exposes:

```typescript
export function parseDocument(json: string): Declaration[]
export function parseFormula(json: string): IrFormula
export function parseTerm(json: string): IrTerm
export function parseSort(json: string): Sort
```

Each parser:

- Accepts UTF-8 input encoded as a JavaScript string.
- Produces typed IR values matching `src/ir/formulas.ts` and
  `src/ir/symbolic/property.ts`.
- Throws a `GrammarParseError` (extends `Error`) on malformed input. The error
  carries:
  - `path`: a JSON Pointer (RFC 6901) to the offending node;
  - `expected`: a description of what was expected;
  - `actual`: the offending value (truncated for readability).

### Strict mode

`parseDocument(json, { strict: true })` additionally enforces:

- Key order matches the emit order specified in this document.
- Predicate name is one of the locked built-ins or matches `^[a-zA-Z_][a-zA-Z0-9_-]*$`.
- Primitive sort name is one of the nine canonical names.

Strict mode is what cross-language fixtures are validated under. Non-strict
mode is the parser's default (kits ship new predicates between releases; the
parser doesn't need a rev to ingest them).

### Round-trip property

The parser-emitter pair satisfies the following fixed-point property:

> For every byte sequence `B` that the grammar accepts,
> `emit(parseDocument(B)) === B`.

This is verified at test time against the three locked cross-language
fixtures (`scripts/cross-lang-equivalence/fixtures.txt`) and against
hand-built coverage examples for every node kind.

## Relationship to the existing kits

The kits are the producers; the grammar is the spec; the parser is the
reference consumer. Each kit's serialization path independently must conform
to the grammar.

### Currently-conforming behavior

| Kit              | Conforms (today) | How                                                                                         |
|------------------|------------------|---------------------------------------------------------------------------------------------|
| TypeScript       | yes              | Manual object literals with deterministic key order; runs in `src/ir/symbolic/`.            |
| Rust             | yes              | `serde::Serialize` with field declaration order matching this document.                    |
| Go               | yes              | `encoding/json` with struct field order matching this document.                             |
| C++              | yes              | Hand-written JSON serialization in `implementations/cpp/provekit-ir-symbolic/include/`.                |

Conformance today is a *fact* (the harness verifies byte-equality on three
fixtures). This grammar promotes it to a *contract* — any future kit, or any
modification to an existing kit, must validate against the grammar.

### Conformance test plan (sketch)

A future `scripts/grammar-conformance/` harness would extend the existing
cross-language equivalence harness:

1. **Per-kit emit test.** For each fixture, run the kit, capture the JSON,
   feed it through the reference parser in **strict mode**. Pass = parser
   accepts. Fail = grammar violation (kit drift).

2. **Round-trip test.** For each fixture, parse the kit's JSON, then re-emit
   via a reference emitter (also lives at `src/ir/grammar/parse.ts`, exposed
   as `emit(value)`). Assert byte equality with the kit's original output.

3. **Negative tests.** Hand-craft documents that violate each rule (extra
   keys, wrong key order in strict mode, NaN, missing required fields, etc.)
   and assert the parser rejects each with a structured error.

4. **Coverage matrix.** Each node kind (forall, exists, and, or, not,
   implies, atomic, var, const, ctor, primitive sort, bitvec sort, set sort,
   tuple sort, function sort, lambda, property declaration, bridge
   declaration) has at least one positive fixture and at least one negative
   fixture.

Step (1) is the load-bearing one for cross-language drift detection. Today
the harness in `scripts/cross-lang-equivalence/` verifies kit-vs-kit
byte-equality; under the grammar, it would additionally verify each kit
against an *external* spec, catching the case where all kits drift together.

The current harness's golden hashes (`scripts/cross-lang-equivalence/goldens.txt`)
are computed over the kit-emit form described by this grammar. Promotion to
a grammar-conformance regime does **not** invalidate those goldens; the
grammar describes exactly what produced them.

## Appendix A — Worked example: `forall_int_gt_zero`

The TS kit, given:

```typescript
property("forall_int_gt_zero", forAll(Int, (x) => gt(x, num(0))))
```

emits exactly this byte sequence (golden SHA256
`b4377644994579d5faafdd65c1d64fd0a70ec44639ac8218612f58892f91342e`):

```json
[{"kind":"property","name":"forall_int_gt_zero","formula":{"kind":"forall","sort":{"kind":"primitive","name":"Int"},"predicate":{"kind":"lambda","varName":"_x0","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"atomic","predicate":">","args":[{"kind":"var","name":"_x0","sort":{"kind":"primitive","name":"Int"}},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}}}}]
```

Every key here appears in the order locked by the corresponding production
above. The reference parser ingests this string, returns a typed
`Declaration[]` of length 1, and the reference emitter recovers the same
byte sequence. Strict-mode parse + round-trip is part of the test suite at
`src/ir/grammar/parse.test.ts`.

## Appendix B — Grammar choices and rationale

- **EBNF over PEG.** EBNF reads more naturally for a spec audience and
  doesn't need ordered choice (the `kind` discriminator does the work that
  PEG ordered choice would otherwise do). The grammar is unambiguous as
  written.

- **Insertion-order keys, not lexical order.** RFC 8785 (JCS) sorts keys
  lexicographically; this grammar locks insertion order instead. Rationale:
  the kits already emit insertion order (TS literals, Rust serde field
  order, Go struct order, C++ hand-written), and the cross-language goldens
  encode that. Switching to lex order would require simultaneous reissue of
  every kit and re-locking of every golden — a meaningless churn. The
  canonicalizer pipeline still uses JCS where it needs to (pass 7 / hash);
  the grammar describes a different layer.

- **Closed-object policy.** Strictness on extra keys keeps the grammar tight
  and prevents silent kit drift. New IR concepts (e.g. a future `iff`
  formula) require an explicit grammar update.

- **Open predicate names.** The TypeScript IR type allows `string` for
  AtomicPredicate as an open extension. The grammar reflects this in
  default mode and lets strict mode lock to the published list.
