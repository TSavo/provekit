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
- New binding-kits MUST place their declarations under `implementations/rust/libprovekit/src/core/platform_semantics/<binding-tag>.rs` (inline) or in a `provekit-realize-<binding>-core` crate. Naming convention for the file matches the binding-tag string from `split_library_surface`.
- New dimensions REQUIRE a ruling amendment to this document (or a new dimension-specific ruling). The substrate's claim "this dimension is load-bearing" needs explicit documentation, not implicit kit-by-kit drift.
- Dimension VALUES under a given dimension may be added freely by kits without ruling amendment, but should follow naming conventions consistent with this table (e.g., for `ArithmeticOverflow`, value names use a single CamelCase noun phrase describing the behavior).
- The `compare_to: IrFormula` of each DimensionValueMemento MUST be structurally distinguishable across kits. Two kits declaring the same `value_name` but with different `compare_to` formulas produces DIFFERENT CIDs, which is the substrate being honest about the divergence. Two kits declaring the same `value_name` with the SAME `compare_to` formula produces the SAME CID, indicating they agree on the semantics.

## Future work

- Mint additional binding-kits (mysql, sqlserver, redshift) using established patterns. Each will declare `RowIdMechanism` + any library-specific dimensions.
- Audit existing language-kit declarations for missing dimensions (e.g., TypeScript currently has minimum-viable set; could be extended for completeness).

## Cross-references

- Stage 3.1 ruling: `docs/plans/2026-05-16-platform-semantic-tag-schema-ruling.md`
- Stage 4 ruling: `docs/plans/2026-05-16-platform-semantics-via-loss-records.md`
- [[2026-05-18-platform-semantics-binding-kit-compose-ruling]]
- [[2026-05-18-op-coverage-verdict-trichotomy-ruling]]
- Per-kit declaration files: `implementations/rust/libprovekit/src/core/platform_semantics/{java,python_common,typescript,better_sqlite3,pg}.rs` and `provekit-realize-{rust,c}-core/src/platform_semantics.rs`
