# Contract CID vs Attestation CID

**Status:** v1.1.0 normative addendum to `2026-05-02-bundle-attestation-protocol.md`
**Date:** 2026-05-03

## §0. Why this spec exists

ProvekIt's substrate guarantee is content addressing: anyone holding the same data computes the same CID. For contracts, this means anyone with the contract declaration can compute the contract's identity without help from the original signer.

The current implementations conflate two distinct hashes under one term ("contract CID"):

1. The hash of the contract's content (the `ContractDecl` itself, JCS-canonical).
2. The hash of the signed envelope wrapping that content (signer pubkey + signature + the content together).

These are not interchangeable. The first is independent of any signer; the second is signer-specific. Treating them as one breaks the substrate guarantee in two ways:

- Two distinct signers attesting to the same logical contract produce two different "contract CIDs", so peer kits bridging to "the contract" must pick which signer's version to bridge to. There is no canonical contract for multiple witnesses to converge on.
- Peer kits cannot independently compute the value to bridge to. They must read it from a signer-specific artifact (e.g. the original signer's mint binary). The §10 closure property of the substrate-not-blockchain manifesto fails for contracts.

This spec separates the two and pins their semantics.

## §1. Definitions

A `ContractDecl` is the IR-level memento that names a contract obligation. Its canonical encoding is JCS per `2026-04-30-canonicalization-grammar.md`.

**Contract CID.** Pure content hash:

```
contractCid := "blake3-512:" || hex(BLAKE3-512(JCS(ContractDecl)))
```

The contractCid is a function of the declaration's bytes only. It does not depend on a signer, a signature, a timestamp, or any envelope field.

**Contract attestation envelope.** A signed wrapper:

```
ContractAttestation := {
    schemaVersion: "1",
    kind:          "contract-attestation",
    contractCid:   <the value above>,
    signer:        <ed25519 pubkey>,
    declaredAt:    <ISO-8601 UTC timestamp>,
    signature:     <ed25519 over JCS({contractCid, signer, declaredAt})>
}
```

**Attestation CID.** Hash of the envelope:

```
attestationCid := "blake3-512:" || hex(BLAKE3-512(JCS(ContractAttestation)))
```

The attestationCid is a function of (contractCid, signer, declaredAt, signature). It changes when any of those change.

## §2. Identity rules

R1. The contractCid identifies a contract. Two declarations with byte-identical JCS produce the same contractCid regardless of who signed them, when, or whether they were signed at all.

R2. The attestationCid identifies an attestation. Two attestations differ in attestationCid if they differ in signer, declaredAt, or signature, even when their underlying contractCid is the same.

R3. Bridges reference contractCids, not attestationCids. A `Bridge` declaration's `sourceContractCid` and `targetContractCid` fields MUST hold contract CIDs (per §1). Implementations MUST NOT use attestation CIDs in these fields.

R4. Multiple distinct attestations MAY exist for the same contractCid. They form a witness set per `2026-05-02-bundle-attestation-protocol.md` §7. Each contributes its own attestationCid to the DAG; all share the underlying contractCid.

R5. An implementation MUST expose a function `contract_cid(decl: ContractDecl) -> CID` that returns the value defined in §1, computed without consulting any signer, signature, or external artifact.

## §3. Witness convergence

Per §2 R4, multiple signers attesting to the same logical contract produce attestations that share a contractCid. This is the property that makes the substrate's witness chain work:

- An auditor walking the DAG for "all witnesses of contract C" filters by `attestation.contractCid == C`. The match is a byte comparison on a content hash, not a signer-dependent value.
- A peer kit bridging to a Rust-minted contract can compute the contractCid locally from the contract declaration's bytes (e.g. extracted from the Rust source file or a published JSON of declarations) and verify the bridge points where it should.
- A second signer countersigning a contract uses the same contractCid; their attestation extends the witness chain without minting a new contract identity.

Without this separation, none of these operations are well-defined: each signer has their own "contract CID" and witnesses don't converge.

## §4. Implementation guidance

Existing kits that currently expose only an envelope-hash CID (Rust's `mint_contract` returns a signed-envelope hash) MUST add a public function that returns the contractCid of the same input. Naming convention:

- `contract_cid(&decl)` (Rust)
- `contractCid(decl)` (Go, TS, C#, Swift)
- `contract_cid(decl)` (Python, Ruby, C, Zig)
- `contractCid(decl)` (Java)

The existing envelope-hash function MAY be retained for attestation operations but MUST be renamed where it is currently called "contract CID" or similar (e.g. to `attestation_cid` or `signed_contract_cid`). Public APIs that return a CID must document which kind they return.

The `mint_contract` flow remains correct for producing signed attestations. The change is that `mint_contract` returns both: the contractCid of the input declaration, and the attestationCid of the signed envelope wrapping it. Bridges and witness queries use the first; the on-disk attestation file is identified by the second.

## §5. Migration

Existing on-disk attestations under `.provekit/self-contracts-attestations/<lang>.json` MUST be re-emitted with both CIDs surfaced:

```json
{
    "schemaVersion": "1",
    "kind": "self-contracts-attestation",
    "lang": "rust",
    "contractBundleCid": "blake3-512:<bundle JCS hash>",
    "attestationCid":    "blake3-512:<envelope hash>",
    "signer":            "ed25519:...",
    "declaredAt":        "...",
    "signature":         "..."
}
```

Where `contractBundleCid` is the content-only hash of the bundle (per §1 R5 applied to the bundle as a JCS array of declarations). Existing tooling that pinned to the envelope hash continues to work via `attestationCid`; tooling that wants signer-independent identity uses `contractBundleCid`.

Bridges minted before this spec MAY reference attestation CIDs; those bridges remain valid as historical witness statements, but new bridges MUST use contractCids.

## §6. Conformance test

A kit conforms to this spec if:

1. `contract_cid(decl)` returns the same value across two runs with the same declaration bytes.
2. `contract_cid(decl)` returns the same value as another kit's `contract_cid` on the same declaration (cross-kit byte equality).
3. `contract_cid(decl)` is independent of any signer state. Specifically: two `mint_contract` calls with different signer keys but the same declaration produce the same `contract_cid` and different `attestation_cid`.
4. New `Bridge` declarations have `sourceContractCid` and `targetContractCid` fields whose values match `contract_cid` of the underlying declarations, NOT `attestation_cid`.

The conformance test SHOULD live in each kit's self-contracts test suite and SHOULD be a `#[test]` (or equivalent) named `contract_cid_is_signer_independent` or similar.
