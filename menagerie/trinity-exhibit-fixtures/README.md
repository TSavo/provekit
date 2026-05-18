# Trinity exhibit fixtures

These fixtures exercise the three-language chain: Python -> Java -> Rust -> Python.
They are input data for the Trinity exhibit (#1068) and are intentionally decoupled from
the test harness, which is blocked on Sir's real-toolchain ruling.

## What the Trinity exhibit demonstrates

The substrate's Trinity claim: source code lifted to the concept-tier hub, transported through
N registered language kits, and lifted back closes at the same hub CID -- byte-identical at
the hub level, modulo a well-characterized loss-record. Fixtures here make that claim concrete
and testable rather than a spec-only assertion.

The chain executed by the exhibit:

```
provekit lift python.py      -> bind
  -> lower --target=java     -> bind
  -> lower --target=rust     -> bind
  -> lower --target=python
```

The final Python output is asserted byte-equivalent to the input modulo loss-records.

## Fixture taxonomy

| Dir | Category | Concept(s) | Expected loss |
|-----|----------|------------|---------------|
| `01-arithmetic-add/` | First-class concept transport | `concept:add`, `concept:mul`, `concept:sub` | Zero (all three Trinity languages have minted morphisms) |
| `02-control-flow-while/` | First-class concept transport | `concept:while`, `concept:conditional`, `concept:eq`, `concept:mod` | Zero or minimal (Java may route conditional through ite) |
| `03-sugar-carrier-throw/` | Sugar-carrier transport | `concept:throw` | One entry: `gap_rust_throw_to_concept_throw` (Rust missing-source-op); concept preserved via comment carrier |
| `04-boundary-http-request/` | Boundary transport | `concept:http-request` | Library-binding differences per hop recorded in loss-records; concept CID stable |
| `05-effect-async/` | Effect propagation | eff_io / async-await | Effect signature tracked; async-to-blocking translation produces loss-record entries if present |
| `06-fn-name-roundtrip/` | Function-name preservation | fn_name_sugar (R14.5) | Naming convention differences (snake_case vs camelCase in Java intermediate) in loss-records; final names restored |

## How a future harness consumes these fixtures

The harness lands under #1068's real-toolchain ruling. Skeleton:

```rust
// menagerie/trinity-exhibit-fixtures/harness (placeholder)
// For each fixture directory:
//   1. invoke: provekit lift <fixture>/source.py -> bind
//   2. invoke: provekit lower --target=java -> bind
//   3. invoke: provekit lower --target=rust -> bind
//   4. invoke: provekit lower --target=python
//   5. compare hub CIDs at steps 1 and 4
//   6. verify loss-records match expected-roundtrip-properties.md assertions
//   7. compile and run final Python; assert observable output matches source
```

Properties checked per fixture:
- Hub CID identity (or loudly-bounded-lossy with documented loss-record)
- No silent drops (`CompositionRefusalMemento` is failure; loudly-bounded-lossy is pass)
- Observable behavior equivalence (final Python prints the same output as original)
- `fn_name_sugar` threading for all named functions (fixture 06 is the canonical test)

## References

- Issue #1068: Trinity exhibit runnable command + integration test
- Issue #978: Phase-6-Trinity umbrella
- PR #1153: bind/lower thread fn_name through wire citations as fn_name_sugar (Option C)
- PR #1154: R14.5 appendix -- function names are sugar at the algebra layer
- `docs/plans/2026-05-17-realization-tag-kinds-and-marketplace-ruling.md` section 2.5 (R14.5)
- `docs/plans/2026-05-16-trinity-completion-checklist.md` (source of truth for Trinity queue state)
- `menagerie/concept-shapes/transport-gaps.md` (gap table referenced by fixture 03)
- `menagerie/concept-shapes/README.md` (hub catalog with concept CIDs)
- PR #1099 (Python emit-compile-run conformance fixtures -- distinct from these; see below)

## Relation to existing conformance fixtures (PR #1099)

PR #1099 landed Python conformance fixtures for single-step lower correctness:
hello_world, recursive factorial, arithmetic, control flow, transported-concept-citation.

These Trinity fixtures are DISTINCT. Their purpose is multi-hop chain correctness, not
single-step emit correctness. The harness around them will invoke the full three-language
chain, not just LiftKit -> BindKit -> LowerKit for a single target.

Fixture 06 (`fn-name-roundtrip`) uses `factorial` as one of its three named functions.
This overlaps structurally with the #1099 `recursive factorial` fixture but tests a
different property: name survival across three language hops, not emitted code correctness.
