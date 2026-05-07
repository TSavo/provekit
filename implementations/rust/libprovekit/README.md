# libprovekit

`libprovekit` is the Rust reference library behind the universal
`provekit` CLI.

The protocol topology is:

- Rust owns the reference implementation and CLI orchestration.
- Language libraries emit native ProofIR, lift/dropper/LSP data, and
  extension body claims in their host ecosystems.
- RPC is used at process boundaries: lifters, droppers, LSP plugins,
  and other kit-owned behaviors.
- The CLI calls this library for reusable protocol logic so terminal
  workflows and embedded Rust workflows cannot drift.

This crate currently exposes the Content-Addressed CI Protocol (CICP)
body builders/checkers:

- `CIBlastRadius`
- `CIJobResultBodyClaim`
- `CIReuseBodyClaim`
- `CIImpactBodyClaim`

Other language libraries should match this crate's body shapes and
canonical CIDs. The Rust CLI can validate those emitted bodies with:

```sh
provekit ci check --body path/to/body.json
```

Shared golden vectors live in:

```text
protocol/conformance/cicp/
```

Those files are the handoff target for language-library agents: derive
the same CIDs for passing vectors and refuse the invalid vector with the
same fail-closed condition.
