# Fixture 02: expected roundtrip properties

Chain: Python -> Java -> Rust -> Python

## Must hold after chain

1. Final Python source compiles and runs without error.
2. `count_down(6)` produces `12` in the final Python output.
3. Hub CID for `concept:while` is identical at lift-out and final lift-out.
4. Hub CID for `concept:conditional` is identical across all three hops.
5. Hub CID for `concept:eq` is identical across all three hops.
6. Loss-record set is empty for all four concepts.
7. Loop termination condition (`i > 0`) is preserved structurally; no loop inversion.
8. Conditional branch on `i % 2 == 0` is preserved; branch sense is not inverted.
9. No `TransportGapMemento` for any of the four concepts.
10. Accumulated value `total` is a single integer that accumulates even numbers only.

## Concept morphism reference

- `concept:while`: morphism_python_while_to_while, morphism_java_while_to_while, morphism_rust_while_to_while
- `concept:conditional`: morphism_python_if_to_conditional, morphism_rust_if_to_conditional (no Java conditional listed separately -- Java uses ite path; note in loss-record if present)
- `concept:eq`: morphism_rust_eq_to_eq, morphism_java_eq_to_eq
- `concept:mod`: morphism_java_mod_to_mod, morphism_rust_rem_to_mod
