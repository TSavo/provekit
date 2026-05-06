# Fix Receipt Protocol (FRP)

**Status:** v0.1.0 draft extension protocol
**Date:** 2026-05-06
**Layer:** extension protocol over ORP, TDP, GCP, and the ProofIR/memento substrate

## Section 0. Purpose

FRP defines the receipt a producer emits when it claims a host-language change closed a named ProofIR gap.

The receipt is not the fix. The receipt is the content-addressed claim that the changed bytes, when re-lifted, closed the exact missing edge under the exact policy named by the receipt.

This is the protocol shape that makes generated code accountable:

```text
candidate bytes -> re-lift -> closure witness -> fix receipt
```

An LLM, human, dropper, IDE quick-fix, or migration tool may propose candidate code. The candidate is not accepted by origin. It is accepted only when it can attach a nontrivial fix receipt.

## Section 1. Relationship to ORP

ORP `transform` mode produces a `RealizerOutput`. FRP is the durable receipt over an accepted transform.

An FRP receipt SHOULD reference:

- the ORP plan CID;
- the pre-transform artifact CID;
- the transformed artifact CID;
- the patch or edit CID;
- the post-transform lift CID;
- the closure witness CID;
- the policy CID;
- the closed gap CID.

FRP does not replace ORP. It names the object downstream systems can cache, audit, sign, compare, and require from generated changes.

## Section 2. Minimal Receipt Shape

```json
{
  "kind": "FixReceipt",
  "schemaVersion": "1",
  "status": "closed",
  "gapCid": "blake3-512:...",
  "missingEdge": "maybe_null(name) => non_null(name)",
  "planCid": "blake3-512:...",
  "preArtifactCid": "blake3-512:...",
  "patchCid": "blake3-512:...",
  "transformedArtifactCid": "blake3-512:...",
  "postLiftCid": "blake3-512:...",
  "closureWitnessCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "producer": {
    "kind": "llm|human|dropper|ide|migration-tool",
    "name": "provekit-realize-java",
    "version": "0.1.0"
  }
}
```

## Section 3. Nontriviality Rule

A fix receipt is nontrivial only if it binds both sides of the change:

```text
preArtifactCid != transformedArtifactCid
and postLiftCid exists
and closureWitnessCid exists
and closureWitnessCid discharges gapCid under policyCid
```

A lint-only edit, formatting-only edit, explanation, or unverified candidate may be useful, but it is not an FRP closed receipt.

## Section 4. LLM Rule

When an LLM produces code that claims to fix a bug, the load-bearing output should be a fix receipt, not prose confidence.

The model may search. The substrate accepts.

```text
LLM candidate -> lift -> verify closure -> FixReceipt
```

Without a fix receipt, the model produced a candidate. With a fix receipt, the candidate has been connected to a witnessed obligation closure.
