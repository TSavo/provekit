# Debugging A Failed Handshake

A handshake fails when the verifier cannot carry a claim from a producer postcondition to a consumer precondition. Treat it as a graph problem over content-addressed claims: which node is missing, stale, malformed, or unsupported?

## Fast Triage

1. Confirm the protocol catalog CID expected by the tool:

   ```sh
   provekit verify-protocol
   ```

2. Inspect the `.proof` bundle or witness that should carry the edge:

   ```sh
   provekit dump path/to/artifact.proof
   ```

3. Compare the consumer precondition CID, producer postcondition CID, and any bridge target CID. A one-byte canonicalization drift is a different claim.

## Common Causes

**Catalog mismatch.** The producer, consumer, and verifier are using different protocol catalog CIDs. PEP can document non-semantic transitions, but the verifier still needs an admitted transition edge.

**Lift adapter coverage gap.** The source annotation exists, but the adapter does not lift it. Check the per-language and per-adapter coverage docs before assuming the producer emitted the claim.

**Canonicalization drift.** Equivalent-looking source constructs do not matter if the emitted canonical bytes differ. Compare canonical JSON and the BLAKE3-512 CID.

**Missing bridge.** The producer and consumer claims may both be valid but not connected. Add or accept a bridge from the source claim to the target reference contract.

**Unaccepted extension witness.** GCP, ORP, CBP, and FRP bodies are claims, not magic privileges. Policy-aware tooling must verify and admit the body before it is part of the graph.

## IDE Symptoms

If the failure appears as an editor diagnostic:

- check the editor's LSP log for process crashes or JSON-RPC errors;
- verify that the plugin binary sees the same `PATH` and catalog as your shell;
- look up the diagnostic code in [error codes](../reference/error-codes.md);
- confirm that the adapter for the source annotation is listed in [per-language status](../reference/per-language-status.md).

## Read Next

- [IDE integration overview](ide-integration/overview.md).
- [Canonical form](../reference/ir/canonical-form.md).
- [Current CIDs](../reference/cids.md).
- [Proof protocol fixtures](../../protocol/conformance/proof-protocol/README.md).
