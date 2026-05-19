# Platform-Semantic Dimension Naming Conventions

Date: 2026-05-18
Status: Active. Implemented across kit declaration modules via PRs #1112-#1117 (Stage 3.1), #1201 (Stage 4 production demo).

## Ruling

Platform-semantic dimensions are kit-minted, open-keyed strings. The substrate does not enumerate them; kits declare them. This ruling captures the current set with their semantic content, so future kit authors know how to align with existing conventions and to avoid colliding with established names.

The substrate stays open-keyed throughout (per `docs/plans/2026-05-16-platform-semantic-tag-schema-ruling.md`). This document is a NAMING CONVENTIONS reference, not a closed enumeration.

## Platform-arithmetic dimensions

Declared by language-kits. Per-op tags reference these dimension VALUES via `PlatformSemanticTag.dimensions: BTreeMap<String, Cid>`.

### `ArithmeticOverflow`

Behavior of integer arithmetic operations on overflow.

| Value name             | Semantic                                                              | Kits declaring it          |
|------------------------|-----------------------------------------------------------------------|----------------------------|
| `Wrapping`             | Overflow wraps modulo 2^N silently                                    | Rust, Java                 |
| `Panic`                | Overflow raises a panic / runtime exception                           | Rust (overflow-checks=on)  |
| `UndefinedBehavior`    | Overflow is UB (compiler may assume it doesn't happen)                | C                          |
| `Arbitrary`            | Operands are arbitrary-precision; no overflow possible                | Python                     |
| `Ieee754Saturate`      | Operands are IEEE 754 doubles; arithmetic saturates to +/- Infinity   | TypeScript                 |
| `SilentTruncate`       | Narrowing truncates silently to the target type's width               | (proposed for cross-target)|

### `IntegerDivisionRounding`

Direction of division for integer operands.

| Value name              | Semantic                                                   | Kits declaring it    |
|-------------------------|------------------------------------------------------------|----------------------|
| `TruncateTowardZero`    | a / b rounds toward zero (C / Rust / Java semantics)       | Rust, Java, C        |
| `FloorTowardNegInf`     | a / b rounds toward negative infinity (Python `//`)        | Python               |
| `FloatDivision`         | a / b is float division; no integer rounding occurs        | TypeScript           |

### `NullSemantics`

Behavior at boundary cases (division by zero, null deref, etc.).

| Value name               | Semantic                                                   | Kits declaring it    |
|--------------------------|------------------------------------------------------------|----------------------|
| `PanicOnDivByZero`       | Division by zero raises a panic                            | Rust                 |
| `ThrowOnDivByZero`       | Division by zero throws an exception                       | Java                 |
| `UndefinedOnDivByZero`   | Division by zero is UB                                     | C                    |
| `RaisesException`        | Division by zero raises an exception                       | Python               |
| `ReturnsNanOrInfinity`   | Division by zero returns NaN or Infinity (IEEE 754)        | TypeScript           |

### `ShiftMode`

Behavior of bit shift operators.

| Value name             | Semantic                                                                | Kits declaring it    |
|------------------------|-------------------------------------------------------------------------|----------------------|
| `Arithmetic`            | Sign-preserving shift (Rust / Java for signed types)                   | Rust, Java           |
| `Logical`               | Zero-fill shift (C for unsigned types)                                 | C                    |
| `Int32Wrapping`         | Operands coerced to int32, shift wraps modulo 32                       | TypeScript           |

### `BitwiseSemantics`

Behavior of bitwise operators.

| Value name             | Semantic                                                          | Kits declaring it    |
|------------------------|-------------------------------------------------------------------|----------------------|
| `TwosComplement`       | Standard two's complement on the native integer width             | Rust, Java, C        |
| `Int32`                | Operands coerced to int32, bitwise on the int32 representation    | TypeScript           |

## Library-API dimensions

Declared by binding-kits. Specific to particular libraries; not language-wide.

### `RowIdMechanism`

How an INSERT operation surfaces the newly-inserted row's id.

| Value name           | Semantic                                                                                        | Kits declaring it    |
|----------------------|-------------------------------------------------------------------------------------------------|----------------------|
| `LastInsertRowid`    | Connection-global mutable state holds the id after `.run()`; statement object exposes a getter  | better-sqlite3       |
| `ReturningClause`    | Server-side: INSERT carries an explicit RETURNING clause; result set holds the id               | pg (postgres)        |

## Op-CID conventions

Each dimension value memento is content-addressed via a BLAKE3-512 CID computed from the JCS-canonical JSON of `DimensionValueMemento { kit_cid, dimension_name, value_name, compare_to: IrFormula }`. CIDs are NOT human-typable; the value_name is the human handle, but the CID is what the substrate compares.

Concept-op CIDs (e.g., for `concept:insert-and-get-id`) are minted similarly via `compute_fixture_cid` from the full AlgorithmMemento JSON. Catalog file naming follows `concept:<name>.blake3-512:<hex>.json` under `menagerie/concept-shapes/catalog/algorithms/`.

## Discipline

- New language-kits adopting an existing platform-arithmetic dimension MUST reuse the existing value names where the semantic is equivalent. Don't mint synonyms.
- **Per #1270 (substrate-uniform correction landed via PRs #1271-#1281):** kit declarations are NOT placed in libprovekit Rust source files. Each kit owns its declaration; libprovekit's job is to LOAD and COMPARE declarations via the `kit.platform_semantics` JSON-RPC method. New kits implement `kit.platform_semantics` in their plugin binary; libprovekit's `platform_semantics_loader` fetches and caches the declaration at startup. The kit binary IS the declaration's source of truth.
- New dimensions REQUIRE a ruling amendment to this document (or a new dimension-specific ruling). The substrate's claim "this dimension is load-bearing" needs explicit documentation, not implicit kit-by-kit drift.
- Dimension VALUES under a given dimension may be added freely by kits without ruling amendment, but should follow naming conventions consistent with this table (e.g., for `ArithmeticOverflow`, value names use a single CamelCase noun phrase describing the behavior).
- The `compare_to: IrFormula` of each DimensionValueMemento MUST be structurally distinguishable across kits. Two kits declaring the same `value_name` but with different `compare_to` formulas produces DIFFERENT CIDs, which is the substrate being honest about the divergence. Two kits declaring the same `value_name` with the SAME `compare_to` formula produces the SAME CID, indicating they agree on the semantics.

## Type-layer dimensions

Added 2026-05-19. Declared by both language-kits and binding-kits, on `concept:literal` and related value-tier ops. Value mementos CITE substrate-canonical concept CIDs (from `menagerie/concept-shapes/catalog/sorts/`), in contrast to Library-API dimensions whose values are kit-minted structural formulas. Both kinds share the same storage (`BTreeMap<String, Cid>`) and the same `compare_op_with` machinery; the distinction is in the value memento's `compare_to` formula CONTENT, not in any new substrate tier.

### `SortAdmission`

Which substrate-canonical sorts the kit admits at a given literal position.

The value memento's `compare_to` formula has shape:

```
Atomic { name: "admits_sorts", args: [Set [<sort_cid_1>, <sort_cid_2>, ...]] }
```

Cross-kit equivalence emerges when two kits declare the same admission set: identical formula content yields identical CID (after #1260's envelope-violation fix lands).

Substrate-canonical sort vocabulary today (in `menagerie/concept-shapes/catalog/sorts/`):
`Bool`, `Bytes`, `Cid`, `EffectName`, `Formula`, `Int`, `List<T>`, `Map<K,V>`, `OpCid`, `SortCid`, `String`, `Term`. New additions per #1261: `Float`, `Null`.

Example declarations:
- TypeScript language-kit: `{ Float, String, Bool, Null }` (JS numerics are Float, no native Int).
- Rust language-kit: `{ Int, Float, String, Bool, Bytes }` (no Null; uses `Option<T>` instead).
- Java language-kit: `{ Int, Float, String, Bool, Bytes, Null }`.
- C language-kit: `{ Int, Float, String, Bytes, Null }` (no native Bool until `<stdbool.h>`).
- Python language-kit: `{ Int, Float, String, Bool, Bytes, Null }`.

A null-free language declaring SortAdmission without `Null` is honest: migration TO that language characterizes source-side Null as a Divergent verdict; `propagate_effects` widens or refuses per the existing trichotomy. No special-casing.

### Future type-layer dimensions

Add when Trinity surfaces the need:
- `IntegerWidth` (e.g., `Int32`, `Int64`, `Arbitrary`).
- `FloatPrecision` (e.g., `Float32`, `Float64`).
- `EncodingMode` (e.g., `UTF-8`, `UTF-16-LE`, `UTF-16-BE`, `ASCII`, `Latin-1`).
- `MutabilityMode` (`Immutable`, `Mutable`, `InteriorMutable`).
- `OwnershipMode` (Rust-flavored: `Owned`, `Borrowed`, `Shared`).

Each future type-layer dimension follows the same shape: kit-minted open-keyed name; value mementos cite or structurally describe per the layer's convention.

## Future work

- Mint additional binding-kits (mysql, sqlserver, redshift) using established patterns. Each will declare `RowIdMechanism` + any library-specific dimensions.
- Audit existing language-kit declarations for missing dimensions (e.g., TypeScript currently has minimum-viable set; could be extended for completeness).

## Cross-references

- Stage 3.1 ruling: `docs/plans/2026-05-16-platform-semantic-tag-schema-ruling.md`
- Stage 4 ruling: `docs/plans/2026-05-16-platform-semantics-via-loss-records.md`
- [[2026-05-18-platform-semantics-binding-kit-compose-ruling]]
- [[2026-05-18-op-coverage-verdict-trichotomy-ruling]]
- Substrate-side primitive (post-#1270): `implementations/rust/libprovekit/src/core/platform_semantics.rs` (dispatcher + composition) + `implementations/rust/libprovekit/src/core/platform_semantics_loader.rs` (JSON-RPC loader). The hardcoded per-kit Rust declaration files were deleted by #1271; declarations now ship in each kit binary's `kit.platform_semantics` RPC handler.
