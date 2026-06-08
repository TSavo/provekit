# Truth Discharge Protocol (TDP)

**Status:** v0.1.0 draft extension protocol
**Date:** 2026-05-06
**Layer:** extension protocol over the ProofIR/memento substrate
**Related:**
- `2026-05-06-extension-protocols.md` - DAG-of-DAGs, witnessed-root reliance, non-executing core
- `2026-05-06-checker-bytecode-protocol.md` - checker witnesses as TDP-specialized discharges
- `2026-05-06-obligation-realizer-protocol.md` - ORP `attest` mode as a producer of discharge witnesses
- `2026-05-06-grammar-conformance-protocol.md` - grammar/invariant conformance as a TDP claim family
- `2026-05-03-substrate-layers-envelope-header-body.md` - signed header/body letter shape
- `2026-05-06-provenance-memento.md` - signing and provenance discipline

## Section 0. Purpose

TDP standardizes the smallest reusable positive evidence claim:

```
this canonical body-claim is true under this policy
```

A proof does not discharge "the world." A proof discharges a named claim. The claim lives in signed/content-addressed body bytes. The positive result is deliberately small:

```
true
```

Everything else is in the body: bytecode CIDs, proof CIDs, obligation CIDs, verifier CIDs, policy CIDs, binding CIDs, source CIDs, compilation witnesses, execution traces, and input DAG roots.

TDP exists so parent claims can rely on one witnessed root rather than re-listing every artifact beneath it.

## Section 1. Unit truth

The central rule:

```
A positive discharge witness emits unit truth over its body.
```

Equivalently:

```
bodyCid + verifierCid + policyCid + evidenceRootCid -> truthWitnessCid
```

where `truthWitnessCid` means:

```
true(bodyCid, verifierCid, policyCid)
```

The body defines what was proven. The result says only that the accepted verifier, under the accepted policy, discharged that body-claim.

Changing any byte in the body changes the body CID. Changing the verifier changes the verifier CID. Changing the policy changes the policy CID. A positive witness over one body, verifier, or policy is not a positive witness over another.

## Section 2. Non-execution rule

TDP is an extension protocol.

Core verification MUST NOT execute proof checkers, checker bytecode, VMs, source compilers, or TDP interpreters. Core verification verifies signed bytes, CIDs, signatures, references, and core memento/header validity. TDP-aware tooling may evaluate a body-claim under policy and emit a discharge witness.

If evaluation fails, refuses, times out, or does not terminate, no positive discharge exists.

## Section 3. Vocabulary

**Body-claim.** Canonical signed/content-addressed body bytes that describe exactly what proposition is being discharged.

**TruthDischargeWitness.** A signed memento asserting that an accepted verifier discharged a body-claim under policy.

**TruthDischargeRefusal.** A signed memento stating that a body-claim was not discharged because evaluation refused, failed, timed out, or was rejected by policy.

**Verifier.** A proof checker, checker runtime, interpreter, compiler witnesser, solver, human-signature policy, or other accepted evaluator identified by CID.

**Policy.** The content-addressed acceptance rule deciding which verifiers, signers, runtimes, fuel limits, proof systems, and result shapes are acceptable.

**Evidence root.** The root CID of the proof DAG, compilation DAG, execution DAG, checker DAG, or other evidence DAG consumed by the verifier.

**Witnessed root.** The CID of the positive witness memento. This is the root parent claims SHOULD reference when relying on the discharged truth.

## Section 4. TruthDischargeWitness shape

Draft body convention:

```json
{
  "kind": "TruthDischargeWitness",
  "schemaVersion": "1",
  "claimBodyCid": "blake3-512:...",
  "claimKind": "proof-acceptance",
  "result": true,
  "verifierCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "evidenceRootCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."],
  "execution": {
    "startedAt": "2026-05-06T00:00:00Z",
    "finishedAt": "2026-05-06T00:00:00Z",
    "fuelUsed": 1842
  },
  "metadata": {
    "producer": "sugar-tdp",
    "producerVersion": "0.1.0"
  }
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"TruthDischargeWitness"`. |
| `schemaVersion` | MUST be `"1"` for this draft. |
| `claimBodyCid` | CID of the canonical body-claim being discharged. |
| `claimKind` | Extension-specific claim family, e.g. `"proof-acceptance"`, `"checker-holds"`, `"compiler-output"`, `"closure"`, `"policy-admission"`. |
| `result` | MUST be JSON boolean `true` for a positive discharge witness. |
| `verifierCid` | CID of the verifier/checker/interpreter accepted by policy. |
| `policyCid` | CID of the policy under which the body-claim was discharged. |
| `evidenceRootCid` | Root CID of the evidence DAG consumed by the verifier. |
| `inputCids` | Prior artifacts this witness depends on. |
| `execution` | Optional execution accounting. |
| `metadata` | Signed metadata/body; part of the witness CID. |

The witness CID is computed over the signed canonical letter containing these fields. A parent that references this witness root is referencing the positive discharge over this exact body.

## Section 5. Body-claim shape

TDP does not define one universal body-claim schema. Each extension protocol defines the body shape it needs.

TDP requires every body-claim to be:

1. canonicalized;
2. content-addressed;
3. signed or contained in a signed memento;
4. explicit about its subject CIDs;
5. explicit about its obligation or proposition CID;
6. explicit about verifier and policy requirements;
7. explicit about input roots.

Example body-claim for an EVM proof acceptance:

```json
{
  "kind": "TruthDischargeBodyClaim",
  "schemaVersion": "1",
  "claimKind": "proof-acceptance",
  "proposition": "evm-bytecode-satisfies-obligation",
  "subjectCids": {
    "evmBytecodeCid": "blake3-512:...",
    "proofCid": "blake3-512:..."
  },
  "obligationCid": "blake3-512:...",
  "verifierCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

The body may contain arbitrary extension-specific bytes or references, but a positive TDP witness discharges only the proposition named by the body. It does not globally bless every referenced artifact.

## Section 6. Refusals and negative evidence

Draft refusal shape:

```json
{
  "kind": "TruthDischargeRefusal",
  "schemaVersion": "1",
  "claimBodyCid": "blake3-512:...",
  "verifierCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "reasonCode": "UNSUPPORTED_PROOF_SYSTEM",
  "message": "verifier policy does not accept this proof system",
  "inputCids": ["blake3-512:..."]
}
```

A refusal is not `false`. It is a signed record that no positive discharge was produced under the cited verifier and policy.

Counterexamples, adversarial witnesses, and negative proof systems MAY be defined by other extension protocols. TDP v0.1 standardizes positive discharge only.

## Section 7. Parent reliance rule

Parent claims SHOULD reference the witnessed root they rely on.

For example:

```
evmBytecodeCid
proofCid
proofCheckerCid
policyCid
obligationCid
  -> claimBodyCid
  -> truthWitnessCid

parentClaimCid
  -> truthWitnessCid
```

The parent does not need to include the EVM bytecode directly if the accepted truth witness already commits to a body that commits to the EVM bytecode. The bytecode remains reachable through the child DAG. The parent references the truth it needs.

Rule:

```
Do not inline the world.
Reference the witnessed root of the world you mean.
```

## Section 8. Relationship to CBP

CBP `CheckerWitness` is a TDP-specialized positive discharge when:

```
CheckerWitness.result == "holds"
```

maps to:

```
TruthDischargeWitness.result == true
```

The CBP body-claim names the obligation CID, checker memento CID, bytecode CID, binding ABI CID, runtime CID, policy CID, and observed artifact CIDs.

A CBP proof-acceptance witness for EVM, WASM, source-derived bytecode, or Sugar-native checker IR SHOULD be representable as a TDP witness or reference a TDP witness root.

## Section 9. Relationship to ORP

ORP `attest` mode MAY emit TDP witnesses.

ORP `transform` mode SHOULD NOT treat a transform as accepted merely because a dropper emitted code. The accepted transform path is:

```
transform output -> re-lift -> body-claim -> TDP witness
```

The transform is trusted only through the witnessed root that discharges the post-transform obligation.

## Section 10. Relationship to GCP

GCP is a TDP claim family.

GCP defines body-claims of kind:

```
grammar-conformance
```

TDP defines the positive discharge shape:

```
true(grammar-conformance body-claim, verifier, policy)
```

This lets extension protocols publish formal grammars and ProofIR invariant sets, then receive ordinary TDP witnessed roots for conforming bodies. TDP does not need to understand grammar metalanguages. It only needs a body-claim, verifier CID, policy CID, evidence root CID, and positive result.

## Section 11. Non-goals

- Define one universal proof language.
- Define one universal body-claim schema.
- Treat `false` as the opposite of refusal.
- Make core verification execute proof checkers or extension bytecode.
- Make every object in a witness body globally trusted.
- Replace CBP, ORP, SMT solvers, proof assistants, or host lifters.

## Section 12. Open questions

1. Should CBP `CheckerWitness.result == "holds"` be normatively defined as a TDP positive discharge?
2. Should TDP claim kinds be cataloged in a shared registry?
3. Should `evidenceRootCid` be required when all evidence CIDs are already listed in the body-claim?
4. Should TDP define a standard counterexample witness, or leave negative evidence to a separate protocol?
5. Should `claimBodyCid` point only to body bytes, or to the enclosing memento CID that contains the body?

## Section 13. Conformance

A TDP v0.1 implementation is conformant if it:

1. Emits canonical signed/content-addressed `TruthDischargeWitness` mementos.
2. Uses JSON boolean `true` as the only positive discharge result.
3. Makes the body-claim explicit and content-addressed.
4. Binds verifier CID, policy CID, evidence root CID, and input CIDs into the witness identity.
5. Emits refusals instead of positive witnesses on unsupported proof systems, unsupported verifiers, timeout, malformed evidence, or policy rejection.
6. Never requires core verification to execute the verifier.
7. Allows parent claims to rely on the witness root rather than re-listing every lower artifact.

## Section 14. Citation

Cite as:

> Sugar Protocol Working Notes (2026). *Truth Discharge Protocol (TDP)*. Draft extension protocol v0.1.0.
