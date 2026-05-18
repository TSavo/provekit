# Fixture 03: expected roundtrip properties

Chain: Python -> Java -> Rust -> Python

## Must hold after chain

1. Final Python source compiles and runs without error.
2. `safe_divide(10, 2)` returns `5`; `safe_divide(7, 0)` raises an exception equivalent to `ValueError`.
3. The `concept:throw` hub CID is present in the lift-out from Python (morphism_python_throw_to_throw).
4. During the Rust lower hop: `concept:throw` has `gap_rust_throw_to_concept_throw` (missing-source-op). A `TransportGapMemento` is emitted; the concept is preserved via a `provekit-concept:throw` comment carrier in the emitted Rust source.
5. During the Java lower hop: `concept:throw` is realized via morphism_java_throw_to_throw. No gap entry.
6. Final Python output preserves the raise site; the throw concept is NOT silently dropped.
7. Loss-record at the Rust hop records the gap; loss-record at the Java hop is clean for throw.
8. Overall chain loss-record contains exactly one entry: the Rust throw gap.
9. No `CompositionRefusalMemento` -- the chain completes loudly-bounded-lossy, not refused.
10. The comment carrier in Rust intermediate output has the form `// provekit-concept:throw`.

## Gap reference

- `gap_rust_throw_to_concept_throw.json`: `missing-source-op`, `accept-permanent`
- This is the designated sugar-carrier demonstration for the Trinity exhibit.

## Note on Java auto-deref framing

The task brief mentioned `concept:addr` as a candidate for this fixture (Java auto-derefs
references). The transport-gap table shows `concept:addr` has `missing-source-op` for both
Python and Java, meaning neither can be a source for the concept. `concept:throw` is a
stronger sugar-carrier case: Python and Java both lift it natively; Rust lacks it; the gap is
well-documented in the gap table. The addr framing would not exercise comment-carrier
preservation because the source language cannot emit addr either.
