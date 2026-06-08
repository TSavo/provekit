# libsugar

`libsugar` is the Rust reference library behind the universal
`sugar` CLI.

The protocol topology is:

- Rust owns the reference implementation and CLI orchestration.
- Language libraries emit native ProofIR, lift/dropper/LSP data, and
  extension body claims in their host ecosystems.
- RPC is used at process boundaries: lifters, droppers, LSP plugins,
  and other kit-owned behaviors.
- The CLI calls this library for reusable protocol logic so terminal
  workflows and embedded Rust workflows cannot drift.
