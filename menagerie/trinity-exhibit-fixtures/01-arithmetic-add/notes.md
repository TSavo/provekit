# Fixture 01: notes

This fixture exercises the simplest possible zero-loss chain.

`concept:add`, `concept:mul`, and `concept:sub` all have minted first-class morphisms in
Python, Java, and Rust (PRs #1150, #1151, #1152). The expected outcome is a clean
hub-CID-identity claim with an empty loss record.

This fixture is the Trinity exhibit's baseline: if it fails, the infrastructure is broken
before any interesting gap behavior is tested.

Distinct from conformance fixture `arithmetic` (PR #1099): that fixture exercises single-step
lower correctness. This fixture gates multi-hop hub-CID stability.
