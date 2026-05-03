# Writing a kit, step 4: the proof envelope

A proof envelope is a `.proof` bundle: many signed mementos travelling together under one outer signature. As of v1.4 a bundle has the same three-layer cut as a single memento. An `envelope` (signer, declaredAt, signature) wraps a `header` (substrate-verified data, including the members map) and a `body` (metadata tooling interprets, opaque to the substrate verifier). The outer signature transitively covers header and body. Tampering with any member, or with any body field, invalidates it.

This step is about wrapping multiple mementos into a single distribution artifact. Single contract envelopes are step 3. Bridges are step 6. The bundle is what ships alongside (or replaces) `package.json` / `Cargo.toml` / `setup.py` / `pom.xml`.

## The three-layer cut applied to a bundle

```
+-------------------+
|     ENVELOPE      |  signer, declaredAt, signature
+-------------------+
|      HEADER       |  data the substrate verifies
|      (data)       |  (schemaVersion, kind, cid, members)
+-------------------+
|      BODY         |  metadata tooling interprets
|    (metadata)     |  (binaryCid, contractSetCid, licenseCid, ...)
+-------------------+
```

The substrate verifies the envelope (signature against signer over the canonical signed bytes) and the header (`kind` is `"catalog"`, `members[cid] -> bytes` is consistent, plus any kind-specific REQUIRED fields). The body is opaque: the substrate MUST NOT reject a bundle on body content alone. Any tool that reads a body field gets cryptographic provenance for free, because the envelope's signature transitively covers the body.

The cut is the same architectural enforcement of "substrate stays small" introduced for single mementos. Adding a body field is free; adding a header field requires a substrate spec change. A kit author who knows the cut never has to ask which side a new bundle field belongs on.

## Layer contents

| Layer    | Required fields                                              | Notes                                                                          |
|----------|--------------------------------------------------------------|--------------------------------------------------------------------------------|
| envelope | `signer`, `declaredAt`, `signature`                          | Signer is `ed25519:<base64-pubkey>`. Signature is over the canonical bytes of `(header, body)`. |
| header   | `schemaVersion`, `kind` (= `"catalog"`), `cid`, `members`    | `members` is a map keyed by member CID.                                        |
| body     | none required                                                | Free-form `metadata` object. Diagnostic, not normative.                        |

The `signer` and `declaredAt` fields live inside the envelope but are NOT signed-over. They bind the signature to a public key and annotate when the assertion was made.

## A full proof bundle

```
{
    "envelope": {
        "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
        "declaredAt": "2026-05-03T17:00:00Z",
        "signature":  "ed25519:..."
    },
    "header": {
        "schemaVersion": "1",
        "kind":          "catalog",
        "cid":           "blake3-512:af09...",
        "members": {
            "blake3-512:bafy...contract-1":   <claim envelope bytes>,
            "blake3-512:bafy...bridge-1":     <bridge envelope bytes>,
            "blake3-512:bafy...evidence-1":   <evidence envelope bytes>
        }
    },
    "metadata": {
        "binaryCid":              "blake3-512:b0f2030d...",
        "contractSetCid":         "blake3-512:c4d1...",
        "previousContractSetCid": "blake3-512:b8e2...",
        "licenseCid":             "blake3-512:f110...",
        "deprecatedAt":           "2027-01-01T00:00:00Z",
        "buildSourceCommit":      "git:abc123...",
        "name":                   "@types/node-v24",
        "version":                "24.3.0"
    }
}
```

The header carries the bundle's substrate identity: kind, content CID, members map. The body carries v1.4 metadata conventions: `binaryCid` (back pin to a compiled artifact), `contractSetCid` and `previousContractSetCid` (contract set extension), `licenseCid` (license attestation), `deprecatedAt` (lifecycle), `buildSourceCommit` (build provenance), plus `name` and `version` strings for human display. None of these are substrate-load-bearing; each is an optional convention an extension protocol or tooling layer interprets.

`name` and `version` were header-shaped fields under v1.1. They are not anymore. The substrate does not key off them, and two bundles whose member contents are byte-identical compute the same content CID regardless of the strings tooling chose to label them.

## Wire format: deterministic CBOR

The bundle's bytes are the CBOR Core Deterministic Encoding (RFC 8949 §4.2.1) of the three-layer object. CBOR is mandatory for the bundle:

1. Member values are byte strings (canonical envelope bytes, signatures, public keys). JSON would force base64 wrapping that lies about what the hash covers.
2. CBOR's deterministic mode is well-specified and content-addressing-clean.
3. Indefinite-length items are forbidden; map keys are length-then-byte-lex; integers use shortest encoding; floats use simplest preservation.

JCS handles canonical JSON for IR and for individual claim envelopes. CBOR handles canonical binary for the `.proof` bundle. Different layers, different tools.

Most CBOR libraries default to convenient encoding, not deterministic encoding. Configure the deterministic mode flag explicitly. Verify the bytes match the Rust kit's CBOR encoder against the conformance fixtures. Avoid handwriting a CBOR encoder unless you have to.

## Members map

The `members` map keys are member CIDs. The values are the canonical bytes of those members, embedded as CBOR byte strings.

For a claim envelope member, the value is the JCS-canonicalized envelope bytes from step 3. The CID is `blake3-512(envelope_bytes)`. The CID is computed over the bytes as stored in the map, not over an abstract structure. Storing a decoded structure and re-canonicalizing on read is wrong: the bytes that hashed to the CID are the bytes that must be stored.

For a bridge envelope member, same shape, different inner content (BridgeDeclaration instead of contract IR). Step 6 covers bridges.

For an evidence envelope member, same shape, the inner content is a witness term (Z3 model, Coq term, etc.). The protocol does not interpret evidence; it stores it for diagnostic and auditing purposes.

Per-member CID consistency (`blake3-512(members[k]) == k` for every key) is a substrate invariant. Per-member signature validity is inherited from step 3: each claim envelope verifies under its own signer.

Map iteration order: keys MUST be in CBOR map-key order (length-then-byte-lex) for the canonical encoding. When a kit reads a bundle the keys are already in this order; when a kit writes a bundle the encoder sorts them.

## binaryCid is body now

In v1.4 `binaryCid` lives in `metadata` (body), not in the header. The substrate does not validate the binary, because the substrate has no concept of "the running binary." The verifier described in [`2026-05-02-binary-attestation-protocol.md`](../../../protocol/specs/2026-05-02-binary-attestation-protocol.md) §4 is the one that hashes the running compiled artifact and compares against `binaryCid`. That is a tooling-layer gate, not a substrate gate. The substrate's job ends at the envelope/header invariants.

This is the body/header cut showing up operationally. `binaryCid` is metadata of a particular extension protocol (binary attestation); it should not force every bundle verifier to reach for the filesystem and hash a binary.

A bundle that omits `binaryCid` is a bundle that does not claim to attest to a compiled artifact. Lift adapters that produce IR without compiling correctly omit it.

## Letter and envelope

The framing from [`2026-05-02-binary-attestation-protocol.md`](../../../protocol/specs/2026-05-02-binary-attestation-protocol.md) §0 is worth holding in mind. A binary attestation is a *letter* and an *envelope*: the binary is the letter, content-addressed by `bcid = hash(binary_bytes)`; the `.proof` bundle is the envelope, minted AFTER the binary is built, carrying `binaryCid: bcid` in its body. The binary does not know its envelope. The envelope knows the binary's hash. One-way reference. No circularity.

Inside the bundle the same shape recurs. A claim envelope (a letter) is content-addressed by its envelope CID; the bundle (an envelope wrapping many letters) carries the per-member CIDs as map keys. The bundle does not appear inside any of its members, and no member references the bundle by CID. References run only in the read-direction.

The two-pin closure of binary-attestation §5 lives at the next layer up: a bridge declaration's `targetProofCid` points at the bundle (forward pin); the bundle's `binaryCid` points at the binary (back pin). The verifier checks both. Either pin alone leaves a hole. That is a step-6 concern, not a step-4 concern; raised here only so the body-level `binaryCid` is not mistaken for a bundle-internal substrate claim.

## What the signature covers, and what the outer CID is

```
signature_payload = JCS([header, body])
signature         = ed25519_sign(privateKey, signature_payload)
```

The payload is the JCS encoding of the two-element array `[header, body]`, in that order. This is the same payload shape as a single claim envelope (step 3) and is signer-machinery the kit already has from step 2. The envelope's `signer` and `declaredAt` are inside the envelope object but outside what the signature commits to.

The bundle's outer CID is BLAKE3-512 of the full canonical CBOR bytes, with the signed envelope (signature included). The bundle's filename is `<outer-cid>.proof`. This makes the filename a self-describing trust root: anyone with the file can recompute the hash and verify it matches the filename, with no metadata lookup. When a downstream consumer references a bundle by CID (a bridge's `targetProofCid`, an external `bundle-attestation` envelope's `cid`), that is the outer CID.

The signature does NOT cover the outer CID. The outer CID is over the signed bytes; the signature is inside those bytes. BLAKE3 is deterministic, so the outer CID is reconstructible from the file bytes alone.

Producer order:

1. Construct the bundle with `signer`, `declaredAt`, header, body. No signature yet.
2. Compute `signature_payload = JCS([header, body])`.
3. `signature = ed25519_sign(privateKey, signature_payload)`.
4. Insert the signature into the envelope.
5. CBOR-canonical-encode the now-signed bundle.
6. `outer_cid = blake3-512(<those bytes>)`.
7. Write to `<outer-cid>.proof`.

## Verification gates

Five checks. Each gate is independent; a verifier MUST run all that apply.

1. **Outer CID consistency.** `blake3-512(file_bytes) == filename_cid`. Catches tampering with any byte of the file.
2. **Top-level signature.** `ed25519_verify(envelope.signer, JCS([header, body]), envelope.signature)`. Binds header and body to the bundle signer.
3. **Header invariants hold.** `kind` is `"catalog"`. `members` is a map. For every key `k` in `members`, `blake3-512(members[k]) == k`. The `cid` field equals the catalog's content CID per the catalog kind's spec (see open question below).
4. **Per-member signatures.** For each member that is itself a signed envelope (every claim envelope, every bridge envelope), the per-member signature verifies under its own declared signer per step 3. The bundle signer and a member signer are unrelated; a third-party attester can wrap mementos signed by other parties.
5. **Binary axis (only if `metadata.binaryCid` is present).** `blake3-512(running_compiled_artifact) == metadata.binaryCid`. This is NOT a substrate gate. It is the verifier procedure of [`2026-05-02-binary-attestation-protocol.md`](../../../protocol/specs/2026-05-02-binary-attestation-protocol.md) §4, run by tooling that knows what "the running binary" means in its context. A bundle whose binary axis fails is not a malformed bundle; it is a bundle whose extension-protocol claim about a compiled artifact does not hold for the artifact at hand.

The verifier does NOT interpret other body fields. A bundle with a body field the verifier does not recognize MUST validate at the substrate layer. Tooling that consumes the field decides what to do with the value.

## Round-trip parity

Your kit's emission MUST equal the Rust kit's emission for the same input. Specifically:

1. Read a Rust-kit-produced `.proof`. Verify outer CID, top-level signature, header invariants, per-member signatures, and (where applicable) the binary axis. Extract the members.
2. Construct a `.proof` from the same members, the same signing key, the same header and body fields. Produce byte-identical output.

Conformance fixtures cover both directions. Byte-equality is the bar; near-equality is broken.

## Common mistakes

- **CBOR's non-deterministic mode.** Most CBOR libraries default to convenient encoding. Configure deterministic mode explicitly and verify against fixtures.
- **JCS-encoding the bundle.** Wrong. JCS is for the signature payload (`JCS([header, body])`) and for IR canonical bytes. CBOR is for the bundle on disk. Different layers.
- **Putting `binaryCid` in the header.** This is the v1.1 layout. In v1.4 `binaryCid` is body metadata, validated by the binary-attestation verifier, not by the substrate. A header `binaryCid` field forces every bundle verifier to reach for the filesystem; the cut exists to prevent that.
- **Treating body fields as substrate-load-bearing.** Adding a body field is free, but using it for trust decisions at the substrate layer breaks the cut. If a verifier needs a field to apply an invariant, the field belongs in the header, which requires a substrate spec change and a kind-specific normative rule. Body fields inform tooling; they do not gate substrate verification.
- **Signing the bundle including its own signature.** The signature payload is `JCS([header, body])`, computed before the signature is added. Including the signature in its own input is cyclic and not how detached signing works.
- **Storing decoded member structures.** Members are stored as the canonical bytes that hash to their CIDs, not as decoded structures. Re-canonicalizing on read can shift bytes by one and break per-member CID consistency.
- **Self-referential pin.** Putting the bundle's own outer CID inside the bundle is the pathology [`2026-05-02-bundle-attestation-protocol.md`](../../../protocol/specs/2026-05-02-bundle-attestation-protocol.md) retires. The bundle's outer CID lives in the filename and in any external `bundle-attestation` envelope that names this bundle as its letter. It does not live inside the bundle.

## When this step is done

The `proof_envelope` fixture passes. Your kit produces `.proof` bundles the Rust kit verifies, and verifies `.proof` bundles the Rust kit produces. Outer CIDs and per-member CIDs agree byte-for-byte. The body/header cut holds: removing `binaryCid` from a bundle does not change the bundle's verification outcome at the substrate layer, only at the binary-attestation layer.

## Read next

- [05-self-contracts.md](05-self-contracts.md): every kit mints its own self-contracts package, which ships as a `.proof` bundle.
- [06-bridge-IR.md](06-bridge-IR.md): the bridge declarations that may appear as members and that consume bundle CIDs via `targetProofCid`.
- [`2026-05-03-substrate-layers-envelope-header-body.md`](../../../protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md): the canonical definition of the envelope/header/body cut, applied uniformly across mementos and bundles.
- [`2026-05-02-binary-attestation-protocol.md`](../../../protocol/specs/2026-05-02-binary-attestation-protocol.md): `binaryCid` semantics, the verifier procedure, the two-pin closure with bridges.
- [`2026-05-02-bundle-attestation-protocol.md`](../../../protocol/specs/2026-05-02-bundle-attestation-protocol.md): the detached attestation envelope shape that points at a `.proof` bundle (or any other content-addressed letter) by CID.
- [`2026-04-30-proof-file-format.md`](../../../protocol/specs/2026-04-30-proof-file-format.md): the underlying file format reference, still canonical for non-v1.4-specific bundle mechanics.
- [docs/security/what-binaryCid-catches.md](../../security/what-binaryCid-catches.md): what the binary axis pins and what it does not.
- [docs/security/multi-dimensional-pinning.md](../../security/multi-dimensional-pinning.md): how `contractCid`, `witnessCid`, and `binaryCid` compose into a rank-N pin.
