# Substrate Layers: Envelope, Header, Body

**Status:** v1.2.0 normative — substrate layering. Companion to `2026-05-03-contract-cid-vs-attestation-cid.md` and `2026-05-03-contract-set-extension.md`.
**Date:** 2026-05-03

## §0. Why this spec exists

Today's protocol mementos conflate three distinct layers under one shape. Fields get added to the top level without a principled answer to "is this thing the substrate verifies, or something tooling interprets?" The result: the substrate has been growing one field at a time, and the boundary between "what the protocol asserts" and "what an ecosystem layer adds" has been getting fuzzier with each addition.

This spec names the cut. Every signed memento is structurally three layers:

```
+-----------------+
|    ENVELOPE     |  signer, declaredAt, signature
+-----------------+
|     HEADER      |  data — what the substrate verifies
|     (data)      |  (kind, content cid, required references)
+-----------------+
|      BODY       |  metadata — what tooling interprets
|   (metadata)    |  (everything else)
+-----------------+
```

The substrate verifies the envelope (signature against signer over the bytes) and the header (the four invariants apply to header references). The body is opaque to the substrate; ecosystems define their own protocols by interpreting body fields. The signature transitively covers all three layers, so the body inherits cryptographic guarantees without being substrate-load-bearing.

This is the architectural cut that makes the principle "substrate stays small" mechanically enforceable: any field a contributor wants to add is either a body field (no spec change required, ecosystems iterate) or a header field (requires substrate spec change, requires verifier-side validation rule).

## §1. Definitions

**Envelope.** The signed wrapper around a memento. Fields:

| Field        | Type          | Notes                                                              |
|--------------|---------------|--------------------------------------------------------------------|
| `signer`     | string        | `ed25519:<base64-pubkey>` per the canonicalization grammar.        |
| `declaredAt` | string        | ISO-8601 UTC timestamp. May be a pinned per-version constant.       |
| `signature`  | string        | `ed25519:<base64-sig>` over JCS of `(header, body)`.                |

The envelope's CID is `hash(JCS(envelope))`. This is the signer-specific attestation CID per the contractCid vs attestationCid spec.

**Header (data).** Substrate-load-bearing fields. The verifier reads these and applies the four invariants. Every memento header MUST include:

| Field            | Type          | Notes                                                              |
|------------------|---------------|--------------------------------------------------------------------|
| `schemaVersion`  | string        | Memento schema version this header conforms to.                     |
| `kind`           | string        | Discriminator: `"contract"`, `"bridge"`, `"self-contracts-bundle"`, etc. |
| `cid`            | string        | Content CID of the underlying claim (per the contractCid spec).      |

Plus any kind-specific REQUIRED header fields named in that kind's normative spec.

**Body (metadata).** Everything else. A JSON object under the field name `metadata`. Body fields are interpreted by tooling, not by the substrate verifier. Examples:

```json
{
    "metadata": {
        "binaryCid":              "blake3-512:...",
        "contractSetCid":         "blake3-512:...",
        "previousContractSetCid": "blake3-512:...",
        "licenseCid":             "blake3-512:...",
        "deprecatedAt":           "2027-01-01T00:00:00Z",
        "buildSourceCommit":      "git:abc123...",
        "downstreamPolicy":       { ... }
    }
}
```

Body is signed (the envelope's signature covers it via JCS of `(header, body)`), so any tool that reads a body field gets cryptographic provenance for free. The substrate doesn't validate body field semantics; the tool consuming the field does.

## §2. Verification rules

A verifier MUST:

R1. Compute the envelope CID and confirm it matches whatever the consumer expected (or recompute and emit it as the canonical attestation identity).
R2. Verify the envelope signature against the envelope's signer over JCS of `(header, body)`. Any tampering with body fields invalidates the signature; tooling can trust body content as having signer provenance.
R3. Validate the header against the kind-specific normative spec. The four invariants apply to header content references (e.g. `cid`, catalog member CIDs, contract member CIDs).
R4. NOT interpret body fields. Body is opaque to the substrate. A verifier MUST NOT reject a memento because a body field is unrecognized.

## §3. The cut, by example

**A self-contracts attestation under the new layering:**

```json
{
    "envelope": {
        "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
        "declaredAt": "2026-05-02T17:00:00Z",
        "signature":  "ed25519:..."
    },
    "header": {
        "schemaVersion": "1",
        "kind":          "self-contracts-attestation",
        "cid":           "blake3-512:69e2cf68...",
        "lang":          "rust"
    },
    "metadata": {
        "contractSetCid":         "blake3-512:abc123...",
        "previousContractSetCid": "blake3-512:def456...",
        "binaryCid":              "blake3-512:789abc..."
    }
}
```

The substrate verifies envelope + header. Tooling reads metadata. Adding `contractSetCid` or `previousContractSetCid` per the contract-set-extension spec required no substrate change because they live in metadata.

**A contract memento under the new layering:**

```json
{
    "envelope": {
        "signer":     "ed25519:...",
        "declaredAt": "2026-05-02T17:00:00Z",
        "signature":  "ed25519:..."
    },
    "header": {
        "schemaVersion": "1",
        "kind":          "contract",
        "cid":           "blake3-512:<contractCid per content-cid spec>",
        "name":          "lift_plugin_lift_response_kind_in_set",
        "outBinding":    "out",
        "pre":           { /* IrFormula */ },
        "post":          { /* IrFormula */ }
    },
    "metadata": {
        "sourceFile":   "implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs",
        "sourceLine":   255
    }
}
```

The header carries the contract's substrate identity (name, outBinding, pre, post — the things the verifier inspects when discharging). The metadata carries the lift adapter's source pointer (useful for IDE tooling, irrelevant to the substrate). The signature covers both.

## §4. Migration

Existing v1.1 mementos have flat field structures (no explicit envelope/header/body separation). v1.2 introduces the layering. Tooling that consumes both versions:

- v1.1 mementos: treat the entire flat structure as `header`, with a synthetic empty `body` and the existing signature/signer/declaredAt as `envelope`.
- v1.2 mementos: read the explicit layering.

The catalog at v1.2 increments and pins the new memento schema versions. Existing signed v1.1 attestations remain valid as historical artifacts; new attestations MUST emit the v1.2 layered shape.

A migration tool walks v1.1 attestations and emits v1.2 equivalents by classifying each top-level field as header (substrate-relevant) or body (metadata) per the kind's spec. Re-signing is not required for the historical chain; the new emissions get fresh signatures.

## §5. What this changes about extension protocols

Every extension protocol previously specced (and any future one) is body-only:

- **Contract set extension** (semver minor): `contractSetCid` and `previousContractSetCid` are body fields.
- **Three axes of pinning** (manifesto §8): `contractCid`, `witnessCid`, `binaryCid` are all body references.
- **Witness chains**: the chain structure is itself a derived view over body references; the substrate doesn't enforce chain shape.
- **Deprecation, licensing, build provenance, audit policy, jurisdictional metadata**: all body fields by default.

This means extension protocols can iterate freely without growing the substrate. A new ecosystem feature is "add a body field convention + tooling that reads it"; the wire format and verifier code do not change. The substrate's surface area is the envelope shape, the header shape per kind, and the four invariants. Everything else is composition above that surface.

## §6. The substrate's full primitive set

After this cut, the substrate's primitives are precisely:

1. **Sign.** Bind an envelope to a signer.
2. **Hash.** Produce a content CID for any byte string.
3. **Reference.** Embed a CID in another memento's header or body.

That is the entire substrate. Header conventions per memento kind add validation rules but no new primitives. Body conventions add zero substrate surface; they are purely a composition pattern. The four invariants operate on envelope + header. Everything else, including the verifiability of any future ecosystem extension, is composition over those three primitives plus the layered shape.

The substrate stays small. The layering makes the smallness mechanical: every contributor to ProvekIt knows that adding to the body is free and adding to the header requires a substrate spec change. The cut decides where the boundary is, and the spec lets contributors stay on the right side of it without asking.
