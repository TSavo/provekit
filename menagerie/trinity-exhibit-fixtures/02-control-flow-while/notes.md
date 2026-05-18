# Fixture 02: notes

`concept:while` is present in Python, Java, and Rust -- but Go has a `missing-source-op` gap,
making while one of the cleaner cross-language transport wins for the Trinity triple.

The conditional branch inside the loop exercises `concept:conditional` simultaneously. This
matters because the Java path may route through `concept:ite` (ternary) rather than a standalone
`concept:conditional` node; the harness should capture that as a loss-record entry if present,
not a refusal.

Distinct from #1099 conformance `control_flow` fixture: that fixture tests single-step emit
correctness. This fixture tests whether the loop + branch structure survives three language hops
and re-emerges with the same operational semantics.
