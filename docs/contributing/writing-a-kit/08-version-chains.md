# Writing a kit, step 8: version chains

Semver's "minor upgrade" promise is an honor-system claim. The maintainer types `18.3.0`, the registry serves the bytes, the consumer trusts the version string. Nothing in that pipeline checks that the new release actually preserves every contract the consumer had pinned to.

Sugar v1.4 turns that promise into a verifiable substrate fact. Each release ships a signed bundle attestation that carries two body fields: a content-addressed identity for the contract set itself, and an optional pointer to the prior set. A consumer can walk the chain back, recompute every link, and reject any release whose claim of "minor upgrade" fails to mechanically hold.

This step teaches the producer side. You are minting attestations; consumers are walking your chain. The conformance suite checks that the bytes you emit agree with every other kit's bytes for the same inputs.

## What `contractSetCid` is

A bundle declares some number of contracts. Each contract has a content-addressed identity, the `contractCid` covered in [07-contract-cid-vs-attestation-cid.md](07-contract-cid-vs-attestation-cid.md). The contract set's identity is the hash of those identities, sorted and JCS-encoded:

```
contractSetCid := "blake3-512:" || hex(BLAKE3-512(JCS(<sorted contractCids>)))
```

`<sorted contractCids>` is the array of contractCid values sorted lexicographically by their hex representation. Sort makes the value order-independent. Two implementations that enumerate the bundle's contracts in different orders agree on the set's CID, because both sort before encoding.

The contractSetCid is a function of the set's members only. It does not depend on bundle layout, mint order, signer identity, or any envelope field. Per the closure principle of the substrate-not-blockchain manifesto §10, this is a derived view: any holder of the underlying contracts can compute it. The substrate does not store it; tooling computes it on demand.

## The two body fields

Under the v1.4 envelope/header/body layering (see [`protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`](../../../protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md)), a self-contracts attestation looks like this:

```json
{
    "envelope": {
        "signer":     "ed25519:<base64-pubkey>",
        "declaredAt": "2026-05-03T17:00:00Z",
        "signature":  "ed25519:<base64-sig>"
    },
    "header": {
        "schemaVersion": "1",
        "kind":          "self-contracts-attestation",
        "lang":          "rust",
        "cid":           "blake3-512:<bundle CID>"
    },
    "metadata": {
        "contractSetCid":         "blake3-512:<this set>",
        "previousContractSetCid": "blake3-512:<prior set>",
        "versionTag":             "18.3.0",
        "channel":                "stable",
        "binaryCid":              "blake3-512:<binary>"
    }
}
```

Two fields matter for version chains, and both live in the body, not the header:

- **`contractSetCid` is REQUIRED on every attestation.** It pins the set identity for the bundle named by `header.cid`. The verifier re-derives it from the bundle's contract declarations and checks it matches.
- **`previousContractSetCid` is OPTIONAL.** When present, it asserts the bundle is a contract-set extension of the prior set. When absent, no extension claim is made: the bundle is a fresh starting point or a major-version reset.

The signature in the envelope covers the JCS of `(header, body)`, so the maintainer's signature transitively binds the extension claim to the bundle's history. If anyone tampers with `previousContractSetCid`, the signature stops verifying.

The body is opaque to the substrate verifier itself. The four substrate invariants apply only to the envelope and header. Body fields are interpreted by tooling. The package-management story this step teaches is one of those tooling layers, riding on top of substrate-load-bearing primitives.

## The five validation rules

Given a candidate attestation `A_new` with `previousContractSetCid = X`, a verifier MUST run R1 through R5. Each rule is a one-line mechanical check; together they decide whether the extension claim holds.

**R1. Recompute `contractSetCid` from the new bundle.** Read every `ContractDecl` in the bundle named by `A_new.header.cid`, compute its contractCid, sort the array, JCS-encode, hash. The result MUST equal `A_new.metadata.contractSetCid`. This catches a maintainer who lies about which set their bundle actually contains.

**R2. Locate a prior attestation `A_old` whose `contractSetCid == X`.** The verifier may hold multiple candidates (multiple maintainers, multiple registries, multiple auditors all attesting). Any candidate whose other rules pass suffices for the chain. The substrate does not pick which witness to use; the consumer's trust policy does.

**R3. Recompute `contractSetCid` from the prior bundle.** Same procedure as R1, applied to `A_old.header.cid`. The result MUST equal `X`. This catches the case where `A_old` itself was lying about its own set.

**R4. Verify `oldSet ⊆ newSet`.** Every `contractCid` in the old set MUST appear in the new set. The new set MAY contain additional contractCids; those are the extension's added contracts. The added contracts are not enumerated in the attestation. Any holder of both bundles can compute the diff on demand and produce its CID independently.

**R5. Verify the signature on `A_new`.** The envelope's `signature` MUST validate against the envelope's `signer` over JCS of `(header, body)`. This binds the entire claim to the asserting key.

If any of R1 through R5 fails, the extension claim is rejected. The verdict is fail-closed: either this is a valid extension, or it is not. There is no "partial extension"; a single missing or modified contract from the old set falsifies R4 and the claim fails outright.

## The semver mapping

The version-string label a maintainer chooses is a derivation of the substrate observation, not a substitute for it.

| Substrate observation                                                    | Semver semantics |
|--------------------------------------------------------------------------|------------------|
| `contractSetCid_new == contractSetCid_old`                               | patch            |
| `previousContractSetCid_new == contractSetCid_old` and R1 through R5 pass | minor            |
| Neither (different set, no extension claim or invalid extension)         | major            |

A binary published as a "minor" upgrade whose attestation lacks `previousContractSetCid`, or whose extension claim fails the rules above, is structurally a lie regardless of what the version string says.

Patch upgrades MAY set `previousContractSetCid` equal to `contractSetCid`. Verifiers treat this as a valid extension with empty diff. The chain stays explicit even when the set is unchanged, which is useful for consumer-side resolvers walking the chain back across patches.

A major upgrade has no `previousContractSetCid`, or has one whose extension claim fails. The maintainer is asserting a fresh contract set; the relation between old and new is now a separate question. A `Bridge` declaration (see [06-bridge-IR.md](06-bridge-IR.md)) MAY attest specific cross-version equivalences for individual contracts, but the bundle-level claim is "this is a different set."

## Witness chains for extensions

The attestation IS the witness. Multiple signers may attest to the same bundle, each with their own `previousContractSetCid` choice:

- The maintainer signs at release time, asserting they did the additive work correctly.
- Independent auditors verify R1 through R5 mechanically, then publish their own attestations naming the same `cid` and the same `previousContractSetCid` they validated against.
- A package registry signs to assert provenance.

These accumulate as witnesses per the substrate-not-blockchain manifesto §7. An auditor evaluating a candidate upgrade walks the witness chain and applies their trust policy. The extension's truth-value is a single mechanical check; the trust calculus is the auditor's.

A consumer holding a pin at `contractSetCid_v0` who wants to upgrade to `contractSetCid_v3` walks the linked list of attestations: `A_v3.previousContractSetCid` to `A_v2.previousContractSetCid` to `A_v1.previousContractSetCid` to `contractSetCid_v0`. Each step is verified per the five rules. The chain length is bounded by the number of releases, not by the size of the contract sets, so verification cost is linear in version distance and constant per link.

A maintainer MAY skip intermediate versions by setting `previousContractSetCid_v3 = contractSetCid_v0` directly. Both forms are valid; the chain density is a maintainer choice, and the consumer picks which chain they prefer to walk. The closure principle applies: any subset of the attestation chain is itself addressable, and any auditor can mint their own pin over their preferred path.

## Producer-side mint code shape

Two functions, both deterministic, both byte-identical across kits.

```
fn compute_contract_set_cid(contracts: List[ContractDecl]) -> CID:
    cids = [contract_cid(c) for c in contracts]    # per step 07
    cids.sort()                                    # lex sort by hex string
    bytes = jcs_encode(cids)                       # array of strings
    return "blake3-512:" + hex(blake3_512(bytes))
```

```
fn mint_self_contracts_attestation(
    bundle_cid: CID,
    contracts: List[ContractDecl],
    prior_set_cid: Optional[CID],
    signer_priv: Ed25519Priv,
    declared_at: ISO8601,
    lang: str,
) -> AttestationEnvelope:
    header = {
        "schemaVersion": "1",
        "kind":          "self-contracts-attestation",
        "lang":          lang,
        "cid":           bundle_cid,
    }
    body = {
        "contractSetCid": compute_contract_set_cid(contracts),
    }
    if prior_set_cid is not None:
        body["previousContractSetCid"] = prior_set_cid
    payload = jcs_encode({"header": header, "body": body})
    signature = ed25519_sign(signer_priv, payload)
    return {
        "envelope": {
            "signer":     ed25519_pubkey_of(signer_priv),
            "declaredAt": declared_at,
            "signature":  signature,
        },
        "header":   header,
        "metadata": body,
    }
```

The mint takes contracts as input and computes the set CID itself; the maintainer never types a CID by hand. The optional prior-set parameter is the only place version chaining shows up at the producer; everything else is the same shape used for non-chained attestations.

## Consumer pinning shapes

A consumer pin file references either the exact attestation or the contract set it claims:

```toml
[dependencies.react]
attestationCid = "blake3-512:<the maintainer's attestation>"
trustPolicy    = "policy-react-conservative"
```

`attestationCid` pins exactly this signed attestation. The consumer fetches it by CID, validates the envelope signature, and walks `previousContractSetCid` back as far as their policy requires.

```toml
[dependencies.react]
contractSetCid = "blake3-512:<the desired contract set>"
trustPolicy    = "policy-react-conservative"
```

`contractSetCid` pins any release whose set matches, regardless of which signer attested to it. Witness counts on the contractSetCid become the trust signal. Multiple maintainers can attest to the same set; the consumer accepts whichever signers their policy trusts.

Range matching (`^18.2.0`, `~18.2.0`, `>= 18.2.0, < 19.0.0`) compiles to a typed DAG query: walk forward from the base contractSetCid, filter by trusted signers and channel, validate each chain link, return the latest by `declaredAt`. Full detail is in [`protocol/specs/2026-05-03-version-chains-pinning.md`](../../../protocol/specs/2026-05-03-version-chains-pinning.md) §3. As a kit author, your job is to mint correctly; the resolver walks what you produce.

## Yank handling at the producer

A yank today is the registry retroactively withdrawing a published version. On the substrate, a maintainer publishes a NEW attestation that yanks an earlier one:

```json
{
    "metadata": {
        "yanksContractSetCid":  "blake3-512:<the withdrawn set's CID>",
        "yankReason":           "security:CVE-2026-XXXX",
        "yankSeverity":         "critical"
    }
}
```

The yank is itself a signed memento. The producer's job ends at minting it. Consumers walk the DAG and apply yanks per their policy: strict policies exclude yanked sets, audit policies honor only specific signers' yanks, permissive policies treat yanks as informational. The substrate carries the signed claim; tooling decides what to do with it.

## Channels and parallel chains

A maintainer publishes parallel attestation chains for different channels:

```
stable:  v18.0.0 -> v18.1.0 -> v18.2.0 -> v18.3.0 -> ...
canary:  v18.0.0 -> v18.0.0-canary.1 -> v18.0.0-canary.2 -> ...
lts:     v17.0.0 -> v17.0.1 -> v17.0.2  (no minor bumps allowed)
```

Each chain is a sequence of attestations linked by `previousContractSetCid`. A `channel` body field labels which chain a given attestation belongs to. A consumer pinning to `lts` walks only the LTS chain.

Branching is straightforward: mint two attestations whose `previousContractSetCid` is the same prior release. Each fork goes forward independently. Merging is also legal: mint an attestation whose new contract set is the union of two prior chains' sets. The chain DAG forms a lattice; consumers pick paths through it.

The maintainer chooses chain density and channel labeling. The substrate stays neutral on what those labels mean.

## Conformance tests

A kit conforms to the contract-set-extension and version-chains specs if:

1. `compute_contract_set_cid(contracts)` returns the same value across two runs with the same inputs.
2. The function returns the same value as another kit's implementation on the same inputs (cross-kit byte equality).
3. The function is independent of mint order, signer state, or bundle layout.
4. `verify_contract_set_extension(attestation_new, attestation_old)` returns true if and only if R1 through R5 from the validation section above hold.
5. Signature validity is checked but signer identity is not interpreted by the protocol; trust calculus stays in the consumer's policy layer.

The conformance test SHOULD live in your kit's self-contracts test suite alongside the contractCid conformance test from step 7. Pinned fixtures cover at least: empty set, single-contract set, multi-contract set in two mint orders (must produce the same CID), valid extension (R1 through R5 pass), and at least one negative case for each rule.

## Common mistakes

- **Forgetting to sort before JCS-encoding the contractCid array.** The sort is the part that makes the set CID order-independent. Any kit that JCS-encodes in mint order produces a different CID than every other kit on the same inputs, and the cross-kit byte-equality test catches this immediately.
- **Treating `previousContractSetCid` as required.** It is OPTIONAL. A bundle with no chain claim (a fresh start, a major bump, a one-off release) is fully valid; it just makes no extension claim. Verifiers who expect the field to be present will reject legitimate attestations.
- **Putting `contractSetCid` in the header.** The header is substrate-load-bearing and the verifier validates it against substrate invariants. The set CID and the chain link are body metadata, interpreted by tooling. The substrate does not validate "is this really an extension"; that check is the resolver's, riding on top of the substrate's signature and content-CID guarantees.
- **Conflating contractSetCid with the bundle file CID.** `header.cid` is the bundle's content hash, computed over the proof-file bytes. `metadata.contractSetCid` is computed over the sorted array of contractCids inside the bundle. These are different hashes over different byte sequences. They will never coincide.
- **Re-implementing the contractCid hash.** The contractCid is defined in step 7. Use the function your kit already exposes; do not roll a parallel implementation for the set computation.

## When this step is done

Your kit's mint emits an attestation whose `contractSetCid` matches the cross-kit fixture, optionally with a `previousContractSetCid` whose chain validates against the prior fixture. Your verifier runs R1 through R5 and returns the spec-defined verdict on every conformance case. A consumer pinning either an `attestationCid` or a `contractSetCid` minted by your kit can resolve and verify the same way they would against any other kit.

Your kit is now a first-class participant in the substrate's package-management story. Future maintainers of libraries written in your language can mint attestations, chain releases, and let consumers walk the chain back without anyone having to trust a registry to underwrite the version-string promise.

## Read next

- [07-contract-cid-vs-attestation-cid.md](07-contract-cid-vs-attestation-cid.md): the leaf identity that contractSetCid is computed over.
- [06-bridge-IR.md](06-bridge-IR.md): the cross-version equivalence story for major bumps.
- [`protocol/specs/2026-05-03-contract-set-extension.md`](../../../protocol/specs/2026-05-03-contract-set-extension.md): the normative spec for the body fields and validation rules.
- [`protocol/specs/2026-05-03-version-chains-pinning.md`](../../../protocol/specs/2026-05-03-version-chains-pinning.md): the consumer-side resolver, fetcher, and trust-policy detail.
- [`docs/papers/05-witness-pluralism-and-jurisdiction-neutral-transport.md`](../../papers/05-witness-pluralism-and-jurisdiction-neutral-transport.md): Corollary 4.5.4 on spec-evolution viability.
- [`docs/security/multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md): pinning surfaces beyond the contract set.
