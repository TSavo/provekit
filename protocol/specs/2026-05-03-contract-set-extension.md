# Contract Set Extension

**Status:** v1.1.0 normative addendum to `2026-05-03-contract-cid-vs-attestation-cid.md` and `2026-05-02-bundle-attestation-protocol.md`
**Date:** 2026-05-03

## §0. Why this spec exists

Semver's "minor" upgrade promise is an honor-system claim: the maintainer asserts "I added new functionality but did not change anything you already depended on." Today the protocol cannot verify that claim. Consumers either trust the version string or hand-diff two releases.

A "minor" upgrade has a precise substrate shape: every contract identity that was in the old bundle is still in the new bundle, with the same `contractCid` (per the contractCid vs attestationCid spec). The new bundle may add new contracts; it may not modify or remove any existing contract. This is the **contract set extension** relation: `oldSet ⊆ newSet`.

This spec adds that as a verifiable substrate fact. It does not introduce a new memento type. It adds one optional metadata field to the existing bundle attestation.

## §1. Definitions

A **contract set** is a finite set of contract declarations. The set's identity is content-addressed.

```
contractSetCid := "blake3-512:" || hex(BLAKE3-512(JCS(<sorted contractCids>)))
```

Where `<sorted contractCids>` is the array of `contractCid` values (per the contractCid vs attestationCid spec) sorted lexicographically by their hex representation. The sort makes the set order-independent: two implementations enumerate contracts in different orders but agree on the set's CID.

The contract set CID is a function of the set's members only. It does not depend on bundle layout, mint order, signer identity, or any envelope field. Per the closure principle of the substrate-not-blockchain manifesto §10, this is a derived view: any holder of the underlying contracts can compute it. The substrate does not store it; tooling computes it on demand.

## §2. The metadata field

The existing bundle attestation memento (per `2026-05-02-bundle-attestation-protocol.md`) gains one optional field:

```
SelfContractsAttestation := {
    schemaVersion:           "1",
    kind:                    "self-contracts-attestation",
    lang:                    <kit identifier>,
    cid:                     <bundle CID, content-addressed>,
    contractSetCid:          <set CID per §1>,                       // NEW (REQUIRED)
    previousContractSetCid:  <prior set's contractSetCid> (optional), // NEW (OPTIONAL)
    declaredAt:              <ISO-8601 UTC timestamp>,
    signer:                  <ed25519 pubkey>,
    signature:               <ed25519 over JCS of all preceding fields>
}
```

`contractSetCid` is REQUIRED on every attestation. It pins the set identity for the bundle the attestation describes. Implementations re-derive it from the bundle's contract declarations (per §1) and check it matches.

`previousContractSetCid` is OPTIONAL. When present, it asserts that the bundle named by `cid` is a contract set extension of the prior set. When absent, no extension claim is made (the bundle is a fresh starting point or a major-version reset).

The signature covers all fields including these two. The maintainer's signature on the attestation transitively binds the extension claim to the bundle's history.

## §3. Validation

Given a candidate attestation A_new with `previousContractSetCid = X`, a verifier MUST:

R1. Recompute `contractSetCid` from the bundle named by `A_new.cid`. It MUST equal `A_new.contractSetCid`.
R2. Locate a prior attestation A_old whose `contractSetCid == X`. (The verifier MAY hold multiple candidates; any valid one suffices for the chain.)
R3. Recompute `contractSetCid` from the bundle named by `A_old.cid`. It MUST equal `X`.
R4. Verify `oldSet ⊆ newSet`: every `contractCid` in the old set MUST appear in the new set. The new set MAY contain additional `contractCid` values not in the old; these are the extension's added contracts.
R5. Verify the signature on A_new against the signer pubkey over JCS of the unsigned fields.

If any of R1-R5 fails, the extension claim is rejected. The substrate gives a fail-closed verdict: either this is a valid extension or it is not. There is no "partial extension" — a single missing or modified contract from the old set falsifies R4 and the claim fails.

The added contracts (the diff `newSet \ oldSet`) are computed by the verifier on demand. They are not enumerated in the attestation. Per the closure principle (manifesto §10): asking for the set difference is a query, not a request to the substrate. Any holder of both bundles can compute the diff and produce its CID independently.

## §4. Semver mapping

Given two versions `v_old` and `v_new` of the same library, each with its own attestation:

| Substrate observation                                           | Semver semantics              |
|-----------------------------------------------------------------|-------------------------------|
| `contractSetCid_new == contractSetCid_old`                      | patch                         |
| `previousContractSetCid_new == contractSetCid_old` and §3 holds | minor                         |
| Neither (different set, no extension claim or invalid extension) | major                         |

The maintainer's version-string label (`18.2.1` vs `18.3.0` vs `19.0.0`) is a derivation OF this comparison, not a substitute for it. A binary published as a "minor" upgrade whose attestation lacks `previousContractSetCid`, or whose extension claim fails §3, is structurally a lie regardless of what the version string says.

A "patch" upgrade is observed when two attestations share `contractSetCid` (the bundles may differ in non-contract content but the set is unchanged). Patch upgrades MAY also set `previousContractSetCid` to the same value as `contractSetCid` to make the chain explicit; verifiers treat this as a valid extension with empty diff.

A "major" upgrade has no `previousContractSetCid` or has one that fails validation. The maintainer is asserting a fresh contract set; the relation between old and new is a separate question (a `Bridge` declaration MAY attest specific cross-version equivalences for individual contracts, but the bundle-level claim is "this is a different set").

## §5. Witness chains for extensions

The attestation IS the witness. Multiple signers may attest to the same bundle, each with their own `previousContractSetCid`:

- The maintainer signs at release time, asserting they did the additive work correctly.
- Independent auditors verify R1-R5 mechanically and publish their own attestations naming the same `cid` and `previousContractSetCid` they chose to validate against.
- A package registry signs to assert provenance.

These accumulate as witnesses per the substrate-not-blockchain manifesto §7. An auditor evaluating a candidate upgrade walks the witness chain and applies their trust policy. The extension's truth-value is a single mechanical check; the trust calculus is the auditor's.

A consumer holding a pin at `contractSetCid_v0` who wants to upgrade to `contractSetCid_v3` walks the linked list of attestations: A_v3.previousContractSetCid → A_v2.previousContractSetCid → A_v1.previousContractSetCid → contractSetCid_v0. Each step is verified per §3. The chain length is bounded by the number of releases, not the size of the contract sets, so the verification cost is linear in version distance and constant per link (the same property as the witness-chain validation in manifesto §7).

A maintainer MAY skip intermediate versions by setting `previousContractSetCid_v3 = contractSetCid_v0` directly. Both forms are valid; the consumer may pick which chain they prefer. The closure principle applies: any subset of the attestation chain is itself addressable, and any auditor can mint their own pin over their preferred path.

## §6. Conformance test

A kit conforms to this spec if:

1. `compute_contract_set_cid(contracts)` returns the same value across two runs with the same inputs.
2. The function returns the same value as another kit's implementation on the same inputs (cross-kit byte equality).
3. The function is independent of mint order, signer state, or bundle layout.
4. `verify_contract_set_extension(attestation_new, attestation_old)` returns true iff R1-R5 hold (§3).
5. The verification respects trust calculus separation: signature validity is checked, but signer identity is not interpreted by the protocol (per the substrate-not-blockchain manifesto §7).

The conformance test SHOULD live in each kit's self-contracts test suite alongside the contractCid conformance test.

## §7. What this spec does not add

This spec does not add a new memento kind. It adds two metadata fields to the existing bundle attestation: `contractSetCid` (REQUIRED) and `previousContractSetCid` (OPTIONAL). It does not change the verifier's four substrate invariants. It does not require any change to existing `ContractDecl` or `BridgeDeclaration` shapes.

The contract set CID is a derived view per the closure principle, computable by anyone holding the underlying contracts. The extension claim is metadata on an existing memento, not a separate artifact. The signature already covers it.

The substrate stays small. The version-history layer becomes a verifiable property of the existing attestation chain, not a new wire-format addition.

## §8. Metadata as the extension surface

This spec is an instance of a more general pattern: the substrate provides three primitives (sign, hash, reference) and a single signed memento format with arbitrary metadata. Anything richer than those three primitives lives in metadata, interpreted by tooling.

Semver semantics is one extension protocol carried in metadata. Others can coexist on the same attestations:

- **Deprecation policy**: an optional `deprecatedAt` field, interpreted by package managers to surface end-of-support warnings.
- **License attestation**: an optional `licenseCid` field naming a content-addressed license document, interpreted by compliance tools.
- **Audit policy**: an optional `requiresWitnessesFrom` field naming pubkeys that must witness for the attestation to be accepted by certain consumers.
- **Build provenance**: an optional `builtFrom` field referencing a commit CID, interpreted by supply-chain auditors.

Each of these is a metadata convention plus tooling. None require new memento types, new primitives, or substrate changes. The maintainer's signature on the attestation transitively binds whatever metadata they choose to include; consumers read whichever fields their tooling cares about.

This is what metadata is for: to be the extension surface where ecosystems build the protocols they need without growing the substrate. The substrate stays at sign + hash + reference + signed metadata, and the composition layer above is unbounded. New extension protocols arrive by spec'ing a metadata convention, not by adding primitives.
