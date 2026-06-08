# Writing a kit, step 6: bridge IR

A bridge memento expresses a single contract-axis claim: the contract at `sourceContractCid` is bound to a target named by a tagged-union `target` field. The substrate verifies the envelope and the header. Body fields carry optional witness, binary, and navigation axes that the substrate does not interpret.

This step is the largest delta in v1.4. The v1.1 9-field flat shape (`kind`, `name`, `sourceSymbol`, `sourceLayer`, `sourceContractCid`, `targetContractCid`, `targetProofCid`, `targetLayer`, `notes?`) is superseded. Bridges minted under v1.4 use the envelope/header/body layering from [`02-canonicalizer.md`](02-canonicalizer.md) and [`03-claim-envelope.md`](03-claim-envelope.md), with a tagged-union `target` field replacing the flat `targetContractCid`.

Two things change beyond the shape itself. First, bridges are derived, not authored. The linker emits them from the union of contract mementos and call-edge mementos produced by the lifter; per-kit code does not mint bridges by hand. Second, the three-axis pin (`contractCid`, `witnessCid`, `binaryCid`) is composed at the consumer's attestation, not at the bridge. The bridge supplies the inputs.

## The v1.4 shape

A full bridge memento under the layered shape:

```json
{
    "envelope": {
        "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
        "declaredAt": "2026-05-03T17:00:00Z",
        "signature":  "ed25519:..."
    },
    "header": {
        "schemaVersion":     "1",
        "kind":              "bridge",
        "name":              "go-kit-lift_plugin_protocol-counterpart",
        "sourceSymbol":      "lift_plugin_lift_response_kind_in_set",
        "sourceLayer":       "rust-kit",
        "sourceContractCid": "blake3-512:abc123...",
        "target": {
            "kind": "contract",
            "cid":  "blake3-512:abc123..."
        }
    },
    "metadata": {
        "targetLayer":      "go-kit",
        "targetWitnessCid": "blake3-512:def456...",
        "targetBinaryCid":  "blake3-512:789abc...",
        "callSite":         { "file": "...", "line": 0, "col": 0 },
        "derivedRelation":  {
            "kind":         "post-implies-pre",
            "evidenceTerm": { /* ProofIR Term */ }
        },
        "derivedBy":     "linker",
        "linkerVersion": "1.0.0",
        "producedAt":    "2026-05-03T17:00:00Z"
    }
}
```

The header carries exactly seven fields. `schemaVersion`, `kind`, `name`, `sourceSymbol`, `sourceLayer`, `sourceContractCid`, and `target`. No other fields appear in the header. The body carries optional witness, binary, and navigation axes plus the linker's derivation provenance.

The signature covers JCS of `[header, body]` per [`03-claim-envelope.md`](03-claim-envelope.md). The bridge's attestationCid is `blake3-512(JCS({envelope, header, body}))`. A consumer comparing bridges by attestationCid distinguishes signers and mint times. A consumer comparing by header content alone hashes JCS of the header.

## The tagged-union target

The `target` field is a JSON object with a `kind` discriminator. Two variants are defined.

```json
{ "kind": "contract", "cid": "<contractCid>" }
```

```json
{ "kind": "contractSet", "cid": "<contractSetCid>" }
```

`kind: "contract"` carries a single contractCid per [`2026-05-03-contract-cid-vs-attestation-cid.md`](../../../protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md). The bridge claims a relation to one specific contract.

`kind: "contractSet"` carries a contractSetCid per [`2026-05-03-contract-set-extension.md`](../../../protocol/specs/2026-05-03-contract-set-extension.md). The bridge claims a relation to a whole sorted set of contracts. This is the shape used when the target is a versioned bundle, a semver-minor extension surface, or any multi-contract anchor.

A kit MUST emit exactly one variant per bridge. A kit MUST NOT emit a string value for `target`. A kit MUST NOT emit both variants in the same field. Attempting to mint a bridge with no target throws or returns an error.

## Omit, don't stringify

Placeholder strings of the form `pending-X:...`, `deferred:...`, `unknown:...`, or any value that is not a syntactically valid CID are forbidden in any substrate-verified bridge field. They are forbidden in `header.sourceContractCid`, in `header.target.cid`, and by extension anywhere the substrate verifier reads a content-addressed reference.

If the witness or binary axis is unknown at mint time, the corresponding body field is absent. It is not represented by a string.

Do this:

```json
{
    "header": {
        "target": { "kind": "contract", "cid": "blake3-512:abc123..." }
    },
    "metadata": {
        "targetLayer": "go-kit"
    }
}
```

Not this:

```json
{
    "header": {
        "target": "pending-go-counterpart:lift_plugin_protocol"
    },
    "metadata": {
        "targetLayer":      "go-kit",
        "targetWitnessCid": "deferred:phase-3-proof-bundle",
        "targetBinaryCid":  null
    }
}
```

A bridge that round-trips with absent axes MUST emit those fields absent on the second pass. A round-trip that adds `"targetWitnessCid":"deferred:..."` or `"targetWitnessCid":null` to a bridge whose original body omitted the field is not byte-equal and is non-conformant.

## Three-axis pin lives at the consumer's attestation

A bridge expresses the contract-axis claim. It MAY carry witness and binary axis references in its body. It does NOT promise the three-axis composition.

The rank-3 pin per `(contractCid, witnessCid, binaryCid)` is constituted by a separate signed attestation, minted by the consumer after reading the three CIDs from the bridge. The bridge gives the inputs; the consumer's attestation is the signed claim.

A consumer asserting the three-axis pin reads:

- `header.target.cid` as the contract axis,
- `metadata.targetWitnessCid` as the witness axis,
- `metadata.targetBinaryCid` as the binary axis,

and signs an attestation over those three values. Where a body axis is absent, the pin is rank-2 (contract plus the present axis) or rank-1 (contract alone), per the pin/float semantics in [`docs/security/multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md). The substitution risk that v1.1 expressed via the `BridgeDeclaration.ConsequentBundlePinned` invariant relocates to the consumer's attestation: the consumer MUST verify that the binary it ran matches `metadata.targetBinaryCid` before treating the bridge as a forward-pinned obligation.

## Cross-kit RPC bridges: target.cid byte-identical to sourceContractCid

A cross-kit RPC bridge attests that an implementation in another kit satisfies a contract owned by the source kit. The canonical shape for such bridges sets `header.target.cid` byte-identical to `header.sourceContractCid`. There is one contractCid per protocol obligation, owned by the kit that defined the contract. Per-kit bridges anchor at that same value.

Sugar's `lift_plugin_protocol` is the worked example. The rust kit owns the contract in `implementations/rust/sugar-self-contracts/src/lift_plugin_protocol.rs`. Per-kit RPC servers in cpp, csharp, go, swift, ts, and zig each implement the protocol. Each implementing kit emits a bridge whose target is the rust contractCid; the implementing kit does not re-declare a parallel "counterpart" contract.

A go-kit bridge for `lift_plugin_lift_response_kind_in_set`:

```json
{
    "header": {
        "schemaVersion":     "1",
        "kind":              "bridge",
        "name":              "go-kit-lift_plugin_lift_response_kind_in_set",
        "sourceSymbol":      "lift_plugin_lift_response_kind_in_set",
        "sourceLayer":       "rust-kit",
        "sourceContractCid": "blake3-512:abc123...",
        "target": {
            "kind": "contract",
            "cid":  "blake3-512:abc123..."
        }
    },
    "metadata": {
        "targetLayer":      "go-kit",
        "targetWitnessCid": "blake3-512:<go-kit's signed witness>",
        "targetBinaryCid":  "blake3-512:<go RPC server binary>"
    }
}
```

A cpp-kit bridge for the same contract:

```json
{
    "header": {
        "schemaVersion":     "1",
        "kind":              "bridge",
        "name":              "cpp-kit-lift_plugin_lift_response_kind_in_set",
        "sourceSymbol":      "lift_plugin_lift_response_kind_in_set",
        "sourceLayer":       "rust-kit",
        "sourceContractCid": "blake3-512:abc123...",
        "target": {
            "kind": "contract",
            "cid":  "blake3-512:abc123..."
        }
    },
    "metadata": {
        "targetLayer":      "cpp-kit",
        "targetWitnessCid": "blake3-512:<cpp-kit's signed witness>",
        "targetBinaryCid":  "blake3-512:<cpp RPC server binary>"
    }
}
```

The contract axis is anchored once at the rust-canonical CID. The implementing kit's identity lives entirely in the body axes. A consumer's attestation per [`docs/security/multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md) binds the three together as a rank-3 pin.

Bridges with `header.target.cid != header.sourceContractCid` are bridging between distinct contracts. Semver migrations, cross-protocol adapters, and version drift bridges are the legitimate cases. Such a divergence MUST be justified in the body. The default for the cross-kit RPC case is byte-identity.

## Bridges are derived, not authored

Per [`2026-05-03-bridge-linkage-protocol.md`](../../../protocol/specs/2026-05-03-bridge-linkage-protocol.md), bridges are emitted by the linker (the rust CLI under `sugar prove`) from the union of contract mementos and call-edge mementos produced by per-kit lifters. A lifter conforming to the protocol emits two streams in its lift response.

1. Contract mementos under `kind: "contract"`.
2. Call-edge mementos under `kind: "call-edge"`, one per call site within the lifted compilation unit.

The linker walks `U = ⋃_kit (contracts_kit ∪ call-edges_kit)`, resolves each call edge's target (using the kit's `resolve_ffi_target` method for cross-language calls), and emits a derived bridge memento per call edge. Re-running the linker over the same input streams produces a byte-identical `bridgeSetCid`.

A kit's per-language code MUST NOT mint `kind: "bridge"` mementos directly. The Phase 2 `cross_kit_bridges.<ext>` slabs that shipped in cpp, csharp, zig, swift, and go are non-normative under v1.4 and are removed in the migration. The kit's job at this step is to read derived bridges, round-trip them, and supply the call-edge stream the linker consumes.

A consumer asserting a non-derived bridge (a hand-curated cross-kit binding for a call site behind dynamic dispatch) MAY mint a bridge directly under their own signing key. Such bridges are valid mementos but carry `metadata.derivedBy: "<consumer name>"` rather than `"linker"`. The substrate verifies them identically; the consumer carries the trust posture.

See [`2026-05-03-bridge-linkage-protocol.md`](../../../protocol/specs/2026-05-03-bridge-linkage-protocol.md) for the full linker semantics, the `LinkBundle` shape, and the FFI resolver protocol.

## Migration from v1.1

Three migration cases apply to existing bridge mementos.

**Bridges with placeholder values.** Bridges carrying `targetContractCid: "pending-X:..."` or `targetProofCid: "deferred:..."` are non-normative under v1.4. They MAY remain on disk as historical artifacts. They MUST be re-emitted under the layered shape before any conformance gate enforcing v1.4 accepts them. On re-emission, the placeholder values are dropped; the corresponding body fields are absent.

**Bridges that carry a contractSetCid in `targetContractCid`.** Existing bridges that put a contractSetCid value in the v1.1 flat `targetContractCid` field migrate to `header.target = { kind: "contractSet", cid: <same value> }`. The flat `targetContractCid` field disappears.

**Bridges relying on the locked key order.** The v1.1 9-field locked key order (`kind`, `name`, `sourceSymbol`, `sourceLayer`, `sourceContractCid`, `targetContractCid`, `targetProofCid`, `targetLayer`, `notes?`) was byte-equality-pinned across kits. The v1.4 layered shape with a nested `target` object breaks that key order. Kits MUST re-pin their golden bridge bytes after migration. The migration is a version boundary. Bridges minted before v1.4 use the old shape; bridges minted after use the new shape. The two shapes MUST NOT mix in the same slab.

## Round-trip conformance

A kit conforms to bridge-target-dimensionality when all of the following hold.

1. **Tagged-union emission.** Emitting a bridge produces JCS bytes whose `header.target` field contains exactly one of `"kind":"contract"` or `"kind":"contractSet"`, and the `cid` value is a syntactically valid CID per the canonicalization grammar.

2. **No-target throws.** Attempting to mint a bridge with no target (no `target` field, or `target: null`) throws or returns an error. The implementation MUST NOT silently emit a placeholder.

3. **Omit, don't stringify.** A bridge with no known `targetWitnessCid` or `targetBinaryCid` round-trips with those fields absent from the body. The body MUST NOT contain `"targetWitnessCid":"deferred:..."`, `"targetWitnessCid":null`, or any string value standing in for an absent axis. Round-trip parity requires absent on input matches absent on output.

4. **Consumer attestation surface.** A test that reads `header.target.cid`, `metadata.targetWitnessCid`, and `metadata.targetBinaryCid` from a bridge and mints a three-axis attestation over those values exists in the kit's conformance suite. Where `targetWitnessCid` or `targetBinaryCid` is absent, the attestation omits those axes per the pin/float semantics of [`docs/security/multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md). This test is RECOMMENDED for bridge-target-dimensionality conformance and REQUIRED for any conformance gate that enforces the three-axis substitution protection.

5. **Cross-kit RPC byte-identity.** A bridge to a contract owned by another kit sets `header.target.cid` byte-identical to `header.sourceContractCid`. The implementing kit does not re-declare a parallel contract with a different contractCid for the same protocol obligation. The implementing kit's identity lives entirely in the body axes.

The fixture covering this conformance is `bridge_decl` (renamed for v1.4 to cover the layered shape). A kit's canonicalizer plus IR types must round-trip a derived bridge byte-identical to the linker's output.

## Common mistakes

- **Hand-authoring bridges in per-kit code.** Bridges are derived by the linker. Per-kit slabs that mint `kind: "bridge"` directly are non-normative under v1.4. Emit call-edge mementos from the lifter; let the linker derive.
- **Placeholder strings.** `pending-X:...`, `deferred:...`, `unknown:...`, and any non-CID value in a substrate-verified field. Omit, don't stringify.
- **Conflating contractCid and attestationCid.** `header.target.cid` is a contractCid per [`2026-05-03-contract-cid-vs-attestation-cid.md`](../../../protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md). Bridges reference content addresses, not envelope hashes.
- **Witness or binary axes in the header.** `targetWitnessCid`, `targetBinaryCid`, `targetLayer`, `targetContractSetCid`, `producedBy`, `producedAt` are body fields. The substrate does not verify them.
- **Re-declaring per-kit "counterpart" contracts for cross-kit RPC.** One contractCid per protocol obligation, owned by the defining kit. Per-kit bridges anchor at that same value.
- **String-quoting the absent axis.** `"targetWitnessCid": null` and `"targetWitnessCid": "deferred:..."` are both non-conformant. Absent means absent.
- **Mixing v1.1 and v1.4 bridges in the same slab.** The two shapes are different wire formats. Pick one per slab.

## When this step is done

The kit's IR types parse derived bridges produced by the linker, round-trip them byte-identical, and throw on attempts to construct a bridge with no target. The body's optional axes are emitted absent when unknown, never stringified. Cross-kit RPC bridges set `header.target.cid` byte-identical to `header.sourceContractCid`. The `bridge_decl` fixture under the v1.4 layered shape passes.

Self-contracts ([step 5](05-self-contracts.md)) can include a derived bridge member without falling back to "self-contracts-partial" status. The kit is fully v1.4 conformant for the bridge axis.

## Read next

- [`protocol/specs/2026-05-03-bridge-target-dimensionality.md`](../../../protocol/specs/2026-05-03-bridge-target-dimensionality.md): the bridge shape spec, normative.
- [`protocol/specs/2026-05-03-bridge-linkage-protocol.md`](../../../protocol/specs/2026-05-03-bridge-linkage-protocol.md): bridges are derived; the linker, the call-edge stream, the link bundle.
- [`protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md`](../../../protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md): what `target.cid` references.
- [`protocol/specs/2026-05-03-contract-set-extension.md`](../../../protocol/specs/2026-05-03-contract-set-extension.md): `contractSetCid` as the second target variant.
- [`protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`](../../../protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md): the cut between substrate and metadata.
- [`docs/security/multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md): how the consumer composes the three-axis pin.
- [`docs/papers/05-witness-pluralism-and-jurisdiction-neutral-transport.md`](../../papers/05-witness-pluralism-and-jurisdiction-neutral-transport.md): body conventions, witness pluralism, the substrate's composition layer.
