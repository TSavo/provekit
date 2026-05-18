# Fixture 01: expected roundtrip properties

Chain: Python -> Java -> Rust -> Python (via lift -> bind -> lower -> bind -> lower -> bind -> lower)

## Must hold after chain

1. Final Python source compiles and runs without error.
2. `compute_sum(3, 4)` produces `13` in the final Python output.
3. Concept CID for `concept:add` is identical at every hub-bound checkpoint (lift-out, mid-chain, final lift-out).
4. Concept CID for `concept:mul` is identical at every hub-bound checkpoint.
5. Concept CID for `concept:sub` is identical at every hub-bound checkpoint.
6. Loss-record set is empty: all three concepts have first-class morphisms in Python, Java, and Rust.
7. No `TransportGapMemento` entries in any chain output.
8. No `CompositionRefusalMemento` entries in any chain output.
9. Structure of the returned value is a single integer expression -- structural shape preserved.
10. Two-operand call shape (`compute_sum(a, b)`) is preserved; no argument reordering.

## Concept morphism reference

- `concept:add`: morphism_java_add_to_add, morphism_rust_add_to_add (both minted, PR #1150/#1151)
- `concept:mul`: morphism_java_mul_to_mul, morphism_rust_mul_to_mul
- `concept:sub`: morphism_java_sub_to_sub, morphism_rust_sub_to_sub
