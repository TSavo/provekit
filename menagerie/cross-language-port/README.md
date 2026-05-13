# Cross-Language Program Transport Exhibit

This exhibit runs the program-transport path through the `concept:*` common-imperative hub:

1. lift C11 source with `provekit-c11-term-project`
2. check the source desugaring-equation set
3. transport C11 algebra to `concept:*` through discharged morphisms
4. transport `concept:*` to the target language algebra through discharged inverse morphisms
5. emit target core-form source
6. transport the target algebra back to `concept:*` and compare the concept artifact byte-for-byte

The driver covers three C functions and nine target languages: C#, Go, Python, TypeScript, Zig, Ruby, PHP, Java, and Rust.

## Inputs

- `foo.c`: the original if-shape, `if (x == 0) return -22; return x;`
- `sum_to.c`: declarations, assignment, arithmetic, comparison, and a `while` loop
- `classify.c`: comparisons, short-circuit boolean `and`, assignment, and conditional branches

## Command

```sh
menagerie/cross-language-port/driver.sh
```

The driver builds the C term projector and `provekit-cli`, then writes one artifact directory per function and target under `artifacts/<function>/<target>/`.

Each target directory contains:

- `<target>.term.json`: the target-language algebra term
- `concept.term.json`: the concept-hub transport image
- `roundtrip.concept.term.json`: target algebra transported back to concept
- `<function>.<ext>`: realized target source in core form
- `transport-report.json`: stage report and morphism receipt references

The driver also writes:

- `artifacts/receipt-cids.tsv`: every morphism receipt CID used by every function and target case
- `artifacts/foo_contract_transport_rust.json`: the contract-level C-to-Rust exhibit path for `foo`, referencing the existing C and Rust function contracts and the operation morphism receipts
- `artifacts/summary.json`: function and target summary

## Receipts And Gaps

The receipt table is generated from the CLI reports so it stays in sync with the minted catalog. The complete source of truth for minted and refused operation morphisms is `../concept-shapes/transport-gaps.md`.

A morphism is present only when the canonicalizer discharge lands exactly on the concept operation CID after namespace, operator, slot, and representation maps. Unsupported or semantically broader source operations are recorded as gaps instead of being guessed.

## Worked Artifact Boundary

Worked artifacts in this exhibit:

- generated `concept:*` operation-contract shape nodes for the common-imperative op set
- discharged `LanguageMorphismMemento` files (minted from real lifter-emitted op specs) and unsigned `MorphismDischargeReceipt` files
- C11-to-target term transport for the three sample functions across the listed targets
- target core-form source emission for those samples
- concept round-trip closure from target algebra back to the same concept term
- a C-to-Rust `foo` contract transport exhibit record under Lemma 4

Spec artifacts:

- the protocol is specified in `../../protocol/specs/2026-05-12-program-transport-protocol.md`
- `transport-gaps.md` documents semantic restrictions and refusal cases

Deferred here:

- bytecode and asm control-flow recovery through `conditional-jump`
- cosmetic re-sugaring after core-form realization
- new desugaring equation sets for languages that do not already have one
- non-C source lifter subprocess wiring inside `provekit transport`; the CLI currently accepts C source directly and can accept already-lifted term JSON for other source algebras
- proof envelope emission for transported function contracts; the exhibit records the Lemma 4 path and operation receipts, while source proof serialization remains a follow-up adapter

T Savo
