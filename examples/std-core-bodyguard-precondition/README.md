# std/core Body-Guard Precondition Showcase

This showcase proves one effects-free precondition seam from Rust `core` source.

The producer contract is lifted from the real `char::to_digit` body in pinned
`rust-src` 1.96.0:

```rust
assert!(
    radix >= 2 && radix <= 36,
    "to_digit: invalid radix -- radix must be in the range 2 to 36 inclusive"
);
```

The lifter treats that body guard as a flat FOL predicate over inputs. It does
not model panic, unwinding, abort, control flow, or effects. The bad caller
passes `radix = 1`, so the existing implication verifier refuses the seam
because the caller cannot establish `radix >= 2 && radix <= 36`.

The witness axis reruns the real std/core `#[should_panic]` tests for the same
guard and also verifies the showcase cargo-test witness.

Claimed: one std/core body-guard precondition seam, zero std source changes.

Not claimed: hidden `.expect()` panics, loop-carried guards, match arms,
if-then-else strengthening, early-return reasoning, or panic semantics.

Residual: multi-formal seam specialization currently pairs target
`formal_names` with caller `arg_terms` positionally. That is sufficient for the
`char::to_digit(self, radix)` slice because the lifted actual ordering places
`radix` in the matching slot, but broader method shapes need a future
receiver/argument alignment refinement before they are claimed.
