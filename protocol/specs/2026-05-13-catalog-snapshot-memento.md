# CatalogSnapshotMemento

## §0 Purpose + Snapshot As Commitment

`CatalogSnapshotMemento` is a content-addressed commitment to the admitted state of one catalog kind at one catalog version. It pins the admitted member set, the promotion decisions that admitted those members, and the policy context active at snapshot time.

The snapshot is a CITATION, not the source of truth. Validity comes from signatures, promotion decisions, policies, and optional proof run receipts. A snapshot MUST NOT silently override its inputs. A consumer MAY use the signed snapshot as a fast path, but a consumer MUST also be able to replay the cited promotion decisions against the cited policies.

## §1 Wire Shape

```cddl
CatalogSnapshotMemento = CatalogSnapshotGenesis / CatalogSnapshotSuccessor

CatalogSnapshotGenesis = {
  admitted_member_set_cid: cid,
  catalog_kind: catalog-kind,
  catalog_root_cid: cid,
  genesis: "genesis",
  policy_set_cid: cid,
  promotion_decision_set_cid: cid,
  provenance_cid: cid,
  signature: signature,
  signer_cid: cid,
  snapshot_time: iso8601,
}

CatalogSnapshotSuccessor = {
  admitted_member_set_cid: cid,
  catalog_kind: catalog-kind,
  catalog_root_cid: cid,
  parent_snapshot_cid: cid,
  policy_set_cid: cid,
  promotion_decision_set_cid: cid,
  provenance_cid: cid,
  signature: signature,
  signer_cid: cid,
  snapshot_time: iso8601,
}

catalog-kind = "concept-shapes" / "policy" / "realization" / namespaced-catalog-kind
namespaced-catalog-kind = tstr
cid = tstr
iso8601 = tstr
signature = tstr
```

All map keys above are listed in JCS-canonical alphabetical order. Producers MUST serialize JSON objects with JCS-canonical alphabetical key order when constructing CIDs or signing bytes.

## §2 Field Semantics

`admitted_member_set_cid` is the CID of the canonical admitted member CID set. Each member CID names a fact admitted into this catalog version.

`catalog_kind` identifies the catalog namespace. The reserved values are `"concept-shapes"`, `"policy"`, and `"realization"`. Extension catalogs MUST use `<namespace>:<kind>` strings so local names do not collide with reserved names.

`catalog_root_cid` is the Merkle root for this snapshot. It commits to `admitted_member_set_cid` and `policy_set_cid` as defined in §7.

`genesis` appears only on the first snapshot for a `catalog_kind`. Its value MUST be exactly `"genesis"`.

`parent_snapshot_cid` is the chain pointer to the previous `CatalogSnapshotMemento` for the same `catalog_kind`. It MUST be present on every non-genesis snapshot and MUST be absent on genesis snapshots.

`policy_set_cid` is the CID of the canonical active policy CID set at `snapshot_time`. Policies include `PolicyMemento` or `PromotionPolicyMemento` objects that define the admission gates applied to the promotions in this snapshot.

`promotion_decision_set_cid` is the CID of the canonical `PromotionDecisionMemento` CID set that admitted the members in `admitted_member_set_cid`.

`provenance_cid` names the provenance record for snapshot construction. It SHOULD include the tool, operator or automation identity, input catalog version, and any optional proof run receipt CIDs used while constructing the snapshot.

`signature` authenticates the JCS-canonical snapshot object with `signature` omitted. The signing algorithm and signature text encoding are governed by the signer key memento referenced by `signer_cid`.

`signer_cid` is a CID reference to the public key or signer identity memento. Acceptance of that signer is a local verifier policy decision.

`snapshot_time` is an ISO 8601 timestamp. It fixes the policy activation point used to decide whether each policy in `policy_set_cid` was active.

## §3 Canonical Set Encoding

The set CIDs for `admitted_member_set_cid`, `promotion_decision_set_cid`, and `policy_set_cid` are computed the same way.

1. Resolve the logical set of CIDs for the field.
2. Reject duplicate CID strings.
3. Sort the CID strings in ascending bytewise order using their UTF-8 string bytes.
4. Encode the sorted array as JSON.
5. Canonicalize that JSON array with JCS.
6. Compute `BLAKE3-512` over the resulting bytes.
7. Render the CID as the repository's self-identifying CID string form, for example `blake3-512:<hex>`.

The committed JSON value is an array, not an object:

```json
["blake3-512:...", "blake3-512:..."]
```

Empty sets are invalid for `admitted_member_set_cid` and `promotion_decision_set_cid`. `policy_set_cid` MUST contain at least one policy CID unless local policy explicitly admits an empty-policy genesis catalog. Consumers that do not permit empty-policy genesis catalogs MUST refuse such snapshots.

## §4 Parent Chaining + Genesis Behavior

Every `catalog_kind` has at most one genesis snapshot in a verifier's accepted chain. The genesis snapshot MUST omit `parent_snapshot_cid` and MUST include `genesis: "genesis"`.

Every successor snapshot MUST include `parent_snapshot_cid`, MUST omit `genesis`, and MUST use the same `catalog_kind` as its parent. A verifier walking a chain MUST check that each `parent_snapshot_cid` equals the CID of the immediately previous accepted snapshot. If the pointer skips, forks, changes catalog kind, or names bytes that do not decode as a `CatalogSnapshotMemento`, verification MUST fail closed.

Parent chaining establishes snapshot succession only. It does not prove member admission by itself.

## §5 Verifier Admission Policy

Verifiers MUST support two admission modes. Consumers choose which mode to use according to their local policy.

**Trusted-snapshot mode:** The verifier resolves the snapshot and parent chain, recomputes the snapshot CID from JCS-canonical bytes, verifies `signature` against `signer_cid`, checks that local policy accepts `signer_cid` for `catalog_kind`, and checks that the parent chain is internally consistent back to an accepted genesis. If all checks pass, the verifier MAY accept `catalog_root_cid` as a fast path commitment to the admitted catalog state.

**Replay mode:** The verifier resolves `admitted_member_set_cid`, `promotion_decision_set_cid`, and `policy_set_cid`; recomputes each set CID by §3; and re-admits each member by matching it to one or more `PromotionDecisionMemento` objects whose promoted CID is the member CID. Each promotion decision MUST cite a policy in the active `policy_set_cid`, and that policy MUST be active at `snapshot_time`. Evidence and optional proof run receipts MUST be checked according to the cited promotion policy. If any member lacks a valid promotion path, replay MUST fail closed.

Trusted-snapshot mode is a citation fast path. Replay mode is the audit path. A conforming implementation MUST implement both paths, even if a deployment config permits only one path for a particular trust boundary.

## §6 Fail Closed Conditions

A verifier MUST refuse the snapshot when any required referenced object is unavailable, malformed, or has a CID that does not match its bytes.

A verifier MUST refuse when any member expected by the consumer is missing from the resolved `admitted_member_set_cid`.

A verifier MUST refuse when a promotion decision used for replay cites a policy that is not in `policy_set_cid`, or when the cited policy was not active at `snapshot_time`.

A verifier MUST refuse when `signature` is invalid, `signer_cid` does not resolve to an accepted signer, or local policy does not accept that signer for the `catalog_kind`.

A verifier MUST refuse when `parent_snapshot_cid` does not equal the previous snapshot CID in the accepted chain, when a successor omits `parent_snapshot_cid`, or when a genesis snapshot includes `parent_snapshot_cid`.

A verifier MUST refuse when `catalog_root_cid` does not equal the root recomputed by §7.

## §7 CID Construction

All CIDs in this spec are computed over JCS-canonical bytes and rendered as `BLAKE3-512` self-identifying CID strings.

The snapshot CID is:

```text
BLAKE3-512(JCS(snapshot_without_signature))
```

`snapshot_without_signature` is the `CatalogSnapshotMemento` JSON object with the `signature` key omitted and all remaining keys JCS-canonicalized. A producer MUST sign exactly those JCS bytes. A verifier MUST verify the signature over exactly those JCS bytes.

The `catalog_root_cid` is:

```text
BLAKE3-512(JCS({"admitted_member_set_cid": admitted_member_set_cid, "policy_set_cid": policy_set_cid}))
```

The object keys in the root preimage are already alphabetical. `promotion_decision_set_cid` is not part of `catalog_root_cid` because it is the replay support set for admission. It remains pinned by the snapshot CID and signature.

## §8 Cross References

- TSavo/provekit#791: `PromotionDecisionMemento`.
- TSavo/provekit#798: policy mementos and active promotion policies.
- TSavo/provekit#799: proof run and receipt evidence used by replay.
- TSavo/provekit#796: admissibility-spine umbrella.
- `protocol/specs/2026-04-30-proof-file-format.md`: `.proof` bundle format and proof envelope context.
- `menagerie/concept-shapes/README.md`: concept shape catalog and CID-addressed catalog members.

## §9 Out Of Scope

Snapshot publishing protocol is out of scope. This spec does not define gossip, pubsub, registry APIs, or storage layout.

Cross-catalog joint snapshots are out of scope. A future spec may define a joint commitment across multiple `catalog_kind` chains, but this spec commits to one catalog kind at a time.
