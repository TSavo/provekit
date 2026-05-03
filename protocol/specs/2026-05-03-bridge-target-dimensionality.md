# Bridge Target Dimensionality

**Status:** v1.1.0 normative addendum to `2026-04-30-ir-formal-grammar.md` §BridgeDeclaration
**Date:** 2026-05-03

## §0. Motivation

Four problems in the current bridge IR shape motivate this addendum.

**Problem 1: target dimensionality is silently overloaded.** Today `targetContractCid` carries one of three distinct claims depending on the kit: a contractCid naming one specific contract (1D), a contractSetCid naming a whole set of contracts (set-D), or a string placeholder (`pending-csharp-counterpart:<name>`, `deferred:phase-3-proof-bundle`) that names nothing. These are different claims pretending to be the same field.

**Problem 2: placeholder strings are anti-substrate.** Strings like `pending-*:` and `deferred:*` are refusals to address. They violate the substrate's closure property (manifesto §10): every CID in a header must be computable from content alone. A placeholder string tells the substrate "I am pinning something" while pinning nothing. The field name says `contractCid`; the value is not a CID.

**Problem 3: the three-axis pin from manifesto §8 is not formalized for bridges.** The manifesto §8 names `contractCid`, `witnessCid`, and `binaryCid` as three orthogonal pinning axes. Spec `2026-05-03-version-chains-pinning.md` makes this the basis of package-manager replacement. Bridges have no normative way to express "I am pinning a contract AND a witness AND a binary." They have `targetContractCid` (one axis, sometimes wrongly occupied) and a half-implemented `targetProofCid` slot that conflates the witness and binary axes.

**Problem 4: the substrate-vs-metadata cut is not enforced for bridges.** Per spec `2026-05-03-substrate-layers-envelope-header-body.md`, the substrate verifies envelope and header; body is metadata, opaque to the substrate. Bridges currently put `targetLayer`, `targetProofCid`, and related fields in the flat header alongside the contract-axis claim. The spec needs to draw the line.

The address-is-multi-dimensional principle (manifesto §11, pending merge; §8 and §10 as current normative basis) requires that every CID in a substrate-verified field be a content-only projection. Fields that mix signer state, build metadata, or deferred identifiers are not CIDs at any dimension. Fields that carry witness or binary claims belong in the body, not in the header, because the substrate has no business verifying them.

Spec `2026-05-03-contract-set-extension.md` solved the same dimensionality question for attestations: it added `contractSetCid` as a body field (metadata) rather than overloading the header's content CID. This addendum applies the same decision to bridges.

## §1. Normative changes

The following rules supersede the corresponding sections of `2026-04-30-ir-formal-grammar.md` §BridgeDeclaration for bridges minted after this spec. The existing flat IR shape (9-field locked key order per §BridgeDeclaration) remains valid for bridges minted before this spec; see §2.

**R1. Tagged-union target field.** A bridge's `target` MUST be a JSON object with a `kind` discriminator. Two variants are defined:

```json
{ "kind": "contract", "cid": "<contractCid>" }
```

```json
{ "kind": "contractSet", "cid": "<contractSetCid>" }
```

`kind: "contract"` carries a single contractCid per `2026-05-03-contract-cid-vs-attestation-cid.md`. `kind: "contractSet"` carries a contractSetCid per `2026-05-03-contract-set-extension.md`. Implementations MUST emit exactly one variant. Implementations MUST NOT emit both variants in the same field. Implementations MUST NOT emit a string value (not a tagged union) for `target`.

**R2. Placeholder strings are not valid CIDs.** Strings of the form `pending-*:` or `deferred:*` are NOT valid CIDs and MUST NOT appear in any substrate-verified bridge field. These include `target.cid`, `sourceContractCid`, and any field the substrate verifier reads as a content-addressed reference. If the witness or binary axis is unknown at mint time, the corresponding body field is OMITTED entirely. It is not represented by a string placeholder.

**R3. Header carries the contract-axis claim only.** Under the layering defined in spec `2026-05-03-substrate-layers-envelope-header-body.md`, the bridge header carries:

| Field              | Type                          | Notes                                               |
|--------------------|-------------------------------|-----------------------------------------------------|
| `schemaVersion`    | string                        | Memento schema version.                              |
| `kind`             | `"bridge"`                    | Fixed discriminator.                                 |
| `name`             | string                        | Unique bridge name.                                  |
| `sourceSymbol`     | string                        | Symbol name in the source layer.                     |
| `sourceLayer`      | string                        | Source kit or language identifier.                   |
| `sourceContractCid`| contractCid string            | Per R1 of `2026-05-03-contract-cid-vs-attestation-cid.md`. |
| `target`           | tagged union per R1           | Contract-axis target claim.                          |

The bridge body (under the `metadata` key) carries the optional metadata axes:

| Field                  | Type             | Notes                                                       |
|------------------------|------------------|-------------------------------------------------------------|
| `targetWitnessCid`     | string (OPTIONAL)| CID of the witness chain endorsing the target contract.      |
| `targetBinaryCid`      | string (OPTIONAL)| CID of the binary artifact the target contract was proven against. |
| `targetLayer`          | string (OPTIONAL)| Target kit or language identifier, for human navigation.     |
| `targetContractSetCid` | string (OPTIONAL)| contractSetCid of the target's containing set at mint time. Per R5. |
| `producedBy`           | string (OPTIONAL)| Identifier of the tool or process that minted this bridge.   |
| `producedAt`           | string (OPTIONAL)| ISO-8601 UTC timestamp of mint time.                         |

Per spec `2026-05-03-substrate-layers-envelope-header-body.md` §2, the substrate verifier MUST verify envelope and header; it MUST NOT interpret body fields. A verifier MUST NOT reject a bridge because a body field is unrecognized.

A full bridge under the new layering:

```json
{
    "envelope": {
        "signer":     "ed25519:...",
        "declaredAt": "2026-05-03T00:00:00Z",
        "signature":  "ed25519:..."
    },
    "header": {
        "schemaVersion":    "1",
        "kind":             "bridge",
        "name":             "swift-lift_plugin_lift_response_kind_in_set-counterpart",
        "sourceSymbol":     "lift_plugin_lift_response_kind_in_set",
        "sourceLayer":      "rust-kit",
        "sourceContractCid":"blake3-512:abc123...",
        "target": {
            "kind": "contract",
            "cid":  "blake3-512:def456..."
        }
    },
    "metadata": {
        "targetWitnessCid": null,
        "targetBinaryCid":  null,
        "targetLayer":      "swift-kit"
    }
}
```

Where `targetWitnessCid` and `targetBinaryCid` are omitted (or null) because the witness and binary axes are not yet pinned. Compare with the Phase 2 bridges in `implementations/swift/Sources/Provekit/CrossKitBridges.swift`, which currently put `deferred:phase-3-proof-bundle` in the flat `targetProofCid` field. Under this spec those fields move to body and the deferred value is omitted rather than stringified.

**R4. Three-axis pin belongs on the consumer's attestation.** A bridge expresses the contract-axis claim in its header. It MAY carry witness and binary axis references in its body. However, a bridge DOES NOT promise the three-axis composition. Only a consumer's own attestation over `(contractCid, witnessCid, binaryCid)` constitutes a three-axis pin (per manifesto §8).

A consumer asserting the three-axis pin reads the three CIDs from the bridge body (header for contract axis, body for witness and binary axes) and signs its own attestation over them. This preserves the closure rule from manifesto §10: composition is free, but no new substrate primitives are added. The bridge provides the inputs; the consumer's attestation is the signed claim.

This rule replaces the forward-pin invariant `BridgeDeclaration.ConsequentBundlePinned` from `2026-04-30-ir-formal-grammar.md`. That invariant tied the bridge to a specific `.proof` bundle CID in the header. Under this spec, the bundle reference moves to body as `targetWitnessCid` or `targetBinaryCid`, and the forward-pin closure is expressed through the consumer's attestation. Implementations that relied on `BridgeDeclaration.ConsequentBundlePinned` for substitution protection SHOULD migrate to consumer-side attestation verification. The substitution risk named in `2026-04-30-ir-formal-grammar.md` §BridgeDeclaration Scenario A/B does not disappear; it relocates: the consumer's attestation is the new closure point, and the consumer MUST verify that the bridge body's `targetBinaryCid` (when present) matches the binary the consumer verified against.

**R5. Optional `targetContractSetCid` in body.** Bridges MAY carry `targetContractSetCid` in the body even when `target.kind == "contract"`. This is the contractSetCid of the target contract's containing set at mint time, useful for downstream DAG walks per spec `2026-05-03-contract-set-extension.md`. It is metadata; the substrate does not verify it.

## §2. Migration of existing bridges

**Bridges with placeholder values.** Existing bridges with `targetContractCid: "pending-X:..."` or `targetProofCid: "deferred:..."` are NON-NORMATIVE under this spec. They MAY remain on disk as historical artifacts. They MUST be re-emitted under the new shape before any conformance gate that enforces this spec. On re-emission, the placeholder values are dropped; the corresponding body fields are omitted.

**Bridges that carry a contractSetCid in `targetContractCid`.** Existing bridges (swift, python patterns) that put a contractSetCid value in the flat `targetContractCid` field are migrated to `target: { kind: "contractSet", cid: <same value> }` in the header. The `targetContractCid` field is removed from the flat shape.

**The locked key order breaks.** The existing 9-field locked key order (`kind`, `name`, `sourceSymbol`, `sourceLayer`, `sourceContractCid`, `targetContractCid`, `targetProofCid`, `targetLayer`, `notes?`) is byte-equality-pinned across kits. Migrating to the envelope/header/body layering with a nested `target` object changes the shape and breaks cross-kit byte equality for existing bridges. Kits MUST re-pin their golden bytes after migration. The migration is a version boundary; bridges minted before this spec use the old shape, bridges minted after use the new shape. The two shapes MUST NOT be mixed in the same slab.

**New bridges.** Bridges minted after this spec MUST use the layered shape with the tagged-union `target` field.

## §3. Conformance test

A kit conforms to this spec if all of the following hold:

1. **Tagged-union emission.** Emitting a bridge produces JCS bytes (under the header's `target` field) containing exactly one of `"kind":"contract"` or `"kind":"contractSet"`, and the `cid` value is a syntactically valid CID per the canonicalization grammar.

2. **No-target throws.** Attempting to emit a bridge with no target (no `target` field, or `target: null`) throws or returns an error. The implementation MUST NOT silently emit a placeholder.

3. **Omit, don't stringify.** A bridge emitted with no known `targetWitnessCid` or `targetBinaryCid` round-trips with those fields absent from the body. The body MUST NOT contain `"targetWitnessCid":"deferred:..."` or any string value for an absent axis. Absent means absent, not null-string-quoted.

4. **Consumer attestation surface.** A consumer test that reads `target.cid`, `metadata.targetWitnessCid`, and `metadata.targetBinaryCid` from a bridge and mints its own three-axis attestation over those values SHOULD exist in the kit's conformance suite. Where `targetWitnessCid` or `targetBinaryCid` is absent, the three-axis attestation omits those axes per the pin/float semantics of manifesto §8. This test is RECOMMENDED, not REQUIRED, for this spec; it is REQUIRED for any conformance gate that enforces the three-axis substitution protection.
