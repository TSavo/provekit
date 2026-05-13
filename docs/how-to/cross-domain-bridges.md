# Cross-Domain Bridges

A cross-domain bridge is a signed, content-addressed implication between two claims that were lifted from different surfaces. The common case is language-to-language correctness: a Rust contract, a Java annotation, and a TypeScript schema all bind to the same reference contract CID.

Use bridges when two domains express the same boundary fact but cannot share syntax, runtime, or checker implementation.

## Bridge Shape

A bridge names:

- the source contract CID;
- the target contract CID or contract-set CID;
- the witness proving `source => target`;
- the signer and policy context;
- optional implementation axes such as witness CID or binary CID.

For cross-kit RPC bridges, the target can be the same canonical contract CID owned by the source kit. The implementing kit adds its own witness and binary axes instead of minting a parallel counterpart contract.

## Build One

1. Lift the source surface into canonical ProofIR.
2. Choose the target reference contract or contract set.
3. Prove the implication from source to target.
4. Emit a bridge body with CIDs for source, target, and witness.
5. Verify that the bridge round-trips through the kit's canonicalizer.

The verifier should fail closed if any CID cannot be resolved, any signature fails, or the source/target terms do not satisfy the bridge policy.

## Use One

A consumer verifier walks from its local precondition to a target CID, then follows accepted bridges from producers or sibling kits. Exact CID equality is the fast path. Cached implication witnesses are the next path. Solver fallback should be reserved for genuinely new pairs.

## Read Next

- [Cross-domain verification](../explanation/cross-domain-verification.md): conceptual walkthrough.
- [Bridge IR guide](../contributing/writing-a-kit/06-bridge-IR.md): kit authoring details.
- [Protocol extensions](../reference/protocol-extensions.md): GCP, ORP, CBP, FRP, and related surfaces.
- [Bridge target dimensionality](../../protocol/specs/2026-05-03-bridge-target-dimensionality.md): normative shape for target axes.
