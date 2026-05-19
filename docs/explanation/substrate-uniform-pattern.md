# The substrate-uniform pattern

Date: 2026-05-19
Status: Active. Every D-series GitHub issue references this document. Every new
issue that proposes new substrate machinery is wrong by default; it should
instead show how its gap closes via the primitives below.

This document is operational, not theoretical. Read it before drafting any
issue, brief, or kit declaration.

## 1. The machine

The substrate is one machine. Every cross-language, cross-binding, cross-version
gap closes via these existing primitives:

1. **Concept catalog.** Concept-CIDs (and boundary-CIDs) minted via JCS+blake3-512
   into `menagerie/concept-shapes/catalog/algorithms/`. Two namespaces:
   `concept:*` for things-in-code, `boundary:*` for kinds-of-boundary-interactions.
2. **Per-kit `PlatformSemanticsDeclaration`.** Each language kit and each
   library/binding kit ships one declaration of shape
   `{ tags, dimension_values, op_aliases }`. The declaration is a (concept-CID,
   dimension-name) -> dimension-value matrix; it is NOT a flat tag list.
3. **Composition.** `platform_semantics_for_binding(lang, tag)` (libprovekit
   `src/core/platform_semantics.rs:124`) merges language-kit and library-kit
   declarations. Binding-wins on conflicts. M+N hub.
4. **Comparison.** `compare_op_with` produces the 4-state trichotomy verdict
   (NoOpinion / Uncharacterizable / Same / Divergent) for each (callsite-op,
   target-kit) pair.
5. **Propagation.** Divergent and Uncharacterizable verdicts become
   `ChangedCallsite { callsite_cid, dimension_name, effect }`.
   `propagate_effects` (libprovekit `src/effect_propagation.rs:111`) cascades
   up the call graph producing Widen / Halt / Refuse decisions per containing
   function.
6. **Realization.** `dispatch_realize` (`kit_dispatch.rs:913`) invokes the
   target binding's realize plugin with an NTT-shaped request; the plugin
   emits target-language source.
7. **Receipt.** Aggregates per-dimension decisions into the trichotomy outcome
   (exact / loudly-bounded-lossy / refuse).

No new dispatcher layers. No new effect engines. No new comparison primitives.
No new "translation" verbs. Anything that feels like a new primitive is almost
certainly a per-kit declaration that has not been written.

## 2. Two layers, one machine

Every dimension the substrate handles falls in one of two layers. Both layers
flow through the same primitives above.

### 2.1 Type-layer dimensions

Properties of values appearing in code. Declared on `concept:literal` and
related value-tier ops, parameterized by AST position.

- `SortAdmission`: which sorts the binding admits at this position. Values:
  `Int32`, `Int64`, `Float32`, `Float64`, `String-UTF-8`, `String-UTF-16`,
  `Bytes`, `Bool`, `Null`. Future-extensible (add `Date`, `Decimal`, etc.).
- `EncodingMode`: byte-level encoding for string sorts. Values: `UTF-8`,
  `UTF-16-LE`, `UTF-16-BE`, `ASCII`, `Latin-1`.
- `IntegerWidth`: bit width and signedness. Often correlated with SortAdmission;
  separable when languages have arbitrary-precision Int.
- `FloatPrecision`: f32 / f64 / arbitrary.
- `MutabilityMode`: immutable / mutable / interior-mutable.
- `OwnershipMode` (Rust-flavored): owned / borrowed / shared.

### 2.2 Boundary-layer dimensions

Properties of operations at library API boundaries. Declared on `concept:sql-*`,
`concept:http-*`, `concept:file-*`, and similar API-tier ops.

- `AsyncMode`: Sync / Async / Cps. The substrate's existing async-migration
  work (D4 dimensionalizes properly).
- `RowIdMechanism`: LastInsertRowid / ReturningClause / CursorLastRowid (already
  minted; see `better_sqlite3.rs:72`).
- `TransactionScope`: explicit-begin / auto-commit / connection-scoped.
- `ErrorPropagation`: exception / Result-wrapping / errno / promise-reject.
- `FetchMechanism`: single-method / multiple-methods / iterator.
- `ConnectionScope`: connection-global / statement-local / session.

Boundary contracts (`boundary:*` namespace) carry their own dimension matrices
parallel to op declarations.

## 3. Per-kit declaration matrix

Each `PlatformSemanticsDeclaration` is a matrix of:

`(concept-CID, dimension-name) -> dimension-value-CID`

A library kit's declaration is NOT one tag. It's the full surface of the
library's admission at its API boundary, across every dimension the substrate
defines.

### Worked example: better-sqlite3's sugar kit

Today: one tag, one dimension. (`concept:insert-and-get-id`, `RowIdMechanism`)
-> `LastInsertRowid`.

Full sugar kit declaration:

| Concept | Dimension | Value |
|---|---|---|
| concept:sql-query | AsyncMode | Sync |
| concept:sql-query | ErrorPropagation | Exception |
| concept:sql-query | TransactionScope | ExplicitBegin |
| concept:sql-query | FetchMechanism | MultipleMethods |
| concept:sql-query | ConnectionScope | ConnectionGlobal |
| concept:sql-execute | AsyncMode | Sync |
| concept:sql-execute | ErrorPropagation | Exception |
| concept:sql-execute | TransactionScope | ExplicitBegin |
| concept:insert-and-get-id | AsyncMode | Sync |
| concept:insert-and-get-id | RowIdMechanism | LastInsertRowid |
| concept:insert-and-get-id | ErrorPropagation | Exception |
| concept:insert-and-get-id | TransactionScope | ExplicitBegin |
| concept:literal@sql-string-position | SortAdmission | { String-UTF-8 } |
| concept:literal@sql-string-position | EncodingMode | UTF-8 |
| concept:literal@bind-args-position | SortAdmission | { Int32, Int64-via-BigInt-flag, Float64, String-UTF-8, Bytes, Null } |
| boundary:sqlite-c-api | (declared, see #1182 family) | (per boundary contract) |

Same matrix shape for every library kit. The pg, sqlite3, aiosqlite, rusqlite,
sqlite-jdbc declarations all populate the same shape with their own values.

## 4. Migration as matrix diff

`compare_op_with` walks both source-binding's and target-binding's declarations
for every (concept, dimension) pair. For each pair:

- Both declare same value -> Same. No ChangedCallsite.
- Both declare different values -> Divergent. ChangedCallsite emitted; the
  dimension-value mementos' structural `compare_to` formulas characterize the
  divergence.
- One side declares, other does not -> Uncharacterizable. ChangedCallsite with
  `absent_on` populated; trichotomy ruling routes to refuse-leg per
  `docs/plans/2026-05-18-refuse-leg-short-circuit-ruling.md`.
- Neither declares -> NoOpinion. No ChangedCallsite.

`propagate_effects` independently cascades each ChangedCallsite up the call
graph; per containing function: Widen / Halt / Refuse.

The receipt's `aggregate_summary` carries per-dimension counts. The migrate's
trichotomy claim is computed from the aggregate.

## 5. Cycle invariance applied

A closed cycle (e.g., Rust+rusqlite -> Java+sqlite-jdbc -> Python+sqlite3 ->
Rust+rusqlite) is lossless when:

- Every (concept, dimension) divergence in every leg gets characterized.
- The composition of leg-1's divergence-functions with leg-2's, then leg-3's,
  composes to identity post-formatter.

The closure invariant is provable only if every Trinity library kit declares
its FULL admission matrix. A kit declaring one tag (today's better-sqlite3
state) cannot ground the closure claim at its boundary. Trinity demo requires
each binding kit's matrix at adequate coverage.

## 6. Issue framing checklist

Before filing or rewriting a D-series issue, the body MUST:

1. Name the concept(s) the gap touches. Cite the catalog file path for each.
   If a concept is missing, file a mint-issue as a prerequisite.
2. Name the dimension(s) the gap adds or extends. Cite the existing sibling
   dimension-value memento that establishes the pattern.
3. Pin the file path and function the declaration arm lives in. Example:
   `implementations/rust/libprovekit/src/core/platform_semantics/<tag>.rs`
   `declaration()` function.
4. Pin the existing test that the new declaration must extend. Tests for
   declarations live in
   `implementations/rust/libprovekit/src/core/platform_semantics.rs:197+` and
   the per-tag module's own `#[cfg(test)] mod tests`.
5. Confirm zero changes to: dispatcher, composition function, comparison
   primitive, propagation engine, realize dispatcher, receipt aggregator. The
   work is declaration-only.
6. If the body needs to add a new dispatcher / new primitive / new engine,
   STOP. The substrate already has the machinery. Re-read this document and
   identify the missing declaration or concept-mint instead.

## 7. Failure modes this document prevents

These framings are wrong; the substrate already covers them.

- "Add a SQL-translate primitive." Wrong. SQL placeholder mangling is
  EMISSION-time text concern owned by per-plugin body templates. The substrate
  carries SQL strings as `concept:literal { value, sort: "String-UTF-8" }`;
  each realize plugin handles its dialect.
- "Add a structural-SQL substrate." Wrong. Same as above.
- "Build a sort-translation engine for cross-language migration." Wrong.
  SortAdmission is a dimension; `compare_op_with` characterizes mismatch;
  `propagate_effects` cascades; same machine.
- "Build an effect-propagation variant for type-layer." Wrong. The existing
  propagation engine is dimension-agnostic; it takes ChangedCallsites with
  `dimension_name` and operates uniformly.
- "Add a translation-comment carrier for cross-version migrations." Wrong.
  Use `concept:transported-op` (project memory `transported_op`) plus existing
  dimension-value mementos.
- "Build a separate machinery for type-layer migration vs boundary-layer
  migration." Wrong. Both are dimensions; same machine.

## 8. Cross-references

- Parent audit: `docs/audits/2026-05-18-kit-as-substrate-participant-vision.md`.
- D6 decomposition audit: `docs/audits/2026-05-19-d6-decomposition.md`.
- Trichotomy ruling: `docs/plans/2026-05-18-op-coverage-verdict-trichotomy-ruling.md`.
- Refuse-leg ruling: `docs/plans/2026-05-18-refuse-leg-short-circuit-ruling.md`.
- Existing declarations as worked examples:
  - `implementations/rust/libprovekit/src/core/platform_semantics/better_sqlite3.rs`
  - `implementations/rust/libprovekit/src/core/platform_semantics/pg.rs`
  - `implementations/rust/libprovekit/src/core/platform_semantics/python_sqlite3.rs`
- The composition + comparison + propagation primitives:
  - `implementations/rust/libprovekit/src/core/platform_semantics.rs:124`
  - `implementations/rust/libprovekit/src/effect_propagation.rs:111`
  - `implementations/rust/provekit-cli/src/kit_dispatch.rs:913`

## 9. Discipline

This document is the substrate-uniform pattern, period. The pattern does not
get extended by adding sections. The pattern gets extended by adding rows to
section 2 (new dimensions) or examples to section 3 (new kits).

If a D-series issue or codex brief proposes a new dispatcher / new primitive
/ new engine, the issue or brief is wrong. The reviewer raises this document
and asks: "where in the existing matrix does this gap close?"
