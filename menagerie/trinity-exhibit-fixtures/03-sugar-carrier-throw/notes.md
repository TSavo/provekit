# Fixture 03: notes

`concept:throw` is minted in Python and Java but absent in Rust
(`gap_rust_throw_to_concept_throw.json`, missing-source-op, accept-permanent).

This makes it the cleanest sugar-carrier demonstration in the Trinity triple: the concept
survives the Rust hop via comment annotation, then is re-realized when the chain returns to
Python. The loss-record tracks the gap honestly rather than silently dropping the throw
semantics.

The original task brief named `concept:addr` for this fixture category. The gap table shows
`concept:addr` has `missing-source-op` in Python, Java, AND Rust -- so it cannot demonstrate
concept preservation through a middle hop. `concept:throw` is architecturally correct for
the sugar-carrier category. The discrepancy is documented in expected-roundtrip-properties.md.
