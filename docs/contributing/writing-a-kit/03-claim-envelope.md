# Writing a kit, step 3: the claim envelope

A claim envelope is a signed memento. As of v1.4 every memento is structurally three layers: an `envelope` (signer, declaredAt, signature), a `header` (substrate-verified data), and a `metadata` body (tooling-interpreted, opaque to the substrate verifier). The signature covers JCS of `(header, body)`; tampering anywhere under the envelope's roof invalidates it.

This step is about the envelope shape for a single contract claim. Bundles, catalogs, and proof envelopes are step 4. Bridges are step 6.

## The three-layer cut

```
+-------------------+
|     ENVELOPE      |  signer, declaredAt, signature
+-------------------+
|      HEADER       |  data the substrate verifies
|      (data)       |  (schemaVersion, kind, cid, kind-specific fields)
+-------------------+
|      BODY         |  metadata tooling interprets
|    (metadata)     |  (sourceFile, sourceLine, build provenance, etc.)
+-------------------+
```

The verifier inspects envelope and header. Header invariants are normative. Body is opaque: the substrate MUST NOT reject a memento on body content alone. Tooling that reads body fields gets cryptographic provenance for free because the envelope's signature transitively covers the body.

The cut is the architectural enforcement of "substrate stays small." Adding a body field is free. Adding a header field requires a substrate spec change. A kit author who knows the cut never has to ask which side a new field belongs on.

## Layer contents

| Layer    | Required fields                                    | Notes                                                     |
|----------|----------------------------------------------------|-----------------------------------------------------------|
| envelope | `signer`, `declaredAt`, `signature`                | Signer is `ed25519:<base64-pubkey>`. Signature is over JCS of `(header, body)`. |
| header   | `schemaVersion`, `kind`, `cid`, kind-specific fields | For a contract: `name`, `outBinding`, `pre`, `post` (or their CIDs). |
| body     | none required                                      | Free-form `metadata` object. Diagnostic, not normative.   |

The `signer` and `declaredAt` fields are inside the envelope but are NOT signed-over. They are the binding between the signature and the public key (signer) and the assertion-time annotation (declaredAt). The signature operates over the JCS encoding of the array `[header, body]`, in that order.

## A full contract memento

```json
{
    "envelope": {
        "signer":     "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
        "declaredAt": "2026-05-03T17:00:00Z",
        "signature":  "ed25519:..."
    },
    "header": {
        "schemaVersion": "1",
        "kind":          "contract",
        "cid":           "blake3-512:69e2cf68...",
        "name":          "lift_plugin_lift_response_kind_in_set",
        "outBinding":    "out",
        "pre":           { "kind": "atomic", "name": "...", "args": [ ] },
        "post":          { "kind": "atomic", "name": "...", "args": [ ] }
    },
    "metadata": {
        "sourceFile":   "implementations/rust/provekit-self-contracts/src/lift_plugin_protocol.rs",
        "sourceLine":   255,
        "buildSourceCommit": "git:abc123..."
    }
}
```

The header carries the contract's substrate identity: name, output binding, the IR formulas (or the CIDs of those formulas, if a kit chooses to indirect, both shapes are valid). The body carries the lift adapter's source pointer, useful for IDE tooling, irrelevant to the substrate.

## Two CIDs, not one

There are two distinct content addresses at this layer.

**contractCid.** Hash of the contract content alone, signer-independent:

```
contractCid = blake3-512(JCS(ContractDecl))
```

The `ContractDecl` is the IR-level shape of the contract: `name`, `outBinding`, `pre`, `post`, and any other contract-content fields named by the contract role's spec. The contractCid is what the header's `cid` field carries. Two signers attesting to byte-identical contract content compute identical contractCids without consulting each other or the original signer.

**attestationCid.** Hash of the signed envelope, signer-specific:

```
attestationCid = blake3-512(JCS(envelope, header, body))
```

The attestationCid changes when any of `signer`, `declaredAt`, or `signature` change, even when the underlying contract content is byte-identical.

The two are not interchangeable:

- The header's `cid` field IS the contractCid.
- The on-disk attestation file is named by the attestationCid.
- Bridges reference contractCid (per [`2026-05-03-contract-cid-vs-attestation-cid.md`](../../../protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md) §2 R3).
- Witness chains group attestations by shared contractCid; multiple distinct attestationCids form a witness set over one contractCid.

A kit MUST expose a function returning the contractCid of a declaration, computed without consulting any signer. Naming convention: `contract_cid` (Rust, Python, Ruby, C, Zig), `contractCid` (Go, TS, C#, Swift, Java).

## What the signature covers

```
signature_payload = JCS([header, body])
signature         = ed25519_sign(privateKey, signature_payload)
```

The payload is the JCS encoding of the two-element array `[header, body]`. JCS is recursive: each layer canonicalizes per its own keys, then the array is encoded with no whitespace.

The envelope's `signer` and `declaredAt` are NOT in the payload. They are inside the envelope object but outside what the signature commits to. This is deliberate: a signature is bound to its key by ed25519's algebra (the signature only verifies under the signer's public key), and the timestamp is metadata about when the assertion was made. Including either inside the payload would make signature replay across declarations of the same content artificially impossible without changing the substrate's trust properties.

What this buys: the body is signed. Anything tooling places in the body inherits the signature. A consumer reading `metadata.sourceFile` knows the signer attested to that path. The substrate does not care about that path; the toolchain that reads it does.

## Verification

A verifier MUST run three checks. All three must pass.

1. **Envelope CID matches.** Compute `attestationCid = blake3-512(JCS({envelope, header, body}))` and compare against whatever identity the consumer expected (or emit it as the canonical attestation identity). This catches tampering anywhere in the envelope.

2. **Signature verifies.** Compute `payload = JCS([header, body])` and run `ed25519_verify(envelope.signer, payload, envelope.signature)`. This catches tampering with header or body content, and binds the assertion to the signer's key.

3. **Header invariants hold.** Apply the kind-specific spec. For a contract memento: `header.cid` MUST equal `blake3-512(JCS(ContractDecl-from-header))`, where the declaration is reconstructed from the header's contract-content fields. Plus any kind-specific REQUIRED fields are present and well-typed.

The verifier does NOT interpret body fields. A memento with an unrecognized body field MUST validate. A memento where the body field is corrupt but the signature still verifies is correct at the substrate layer; tooling that consumes the field decides what to do with garbage.

## Round-trip parity

Your kit's emission MUST equal the Rust kit's emission for the same input. Specifically:

1. Read a Rust-kit-produced contract envelope. Verify all three checks. Extract the contract content.
2. Construct an envelope from the same contract content with the same signing key. Produce byte-identical output.

The conformance fixtures cover both directions. Byte-equality is the conformance bar; near-equality is broken.

## Common mistakes

- **Signing the envelope including `signer` and `declaredAt`.** Wrong. The signature payload is JCS of `[header, body]` only.
- **Conflating contractCid and attestationCid.** The header's `cid` is the contractCid (signer-independent). The on-disk file is named by the attestationCid (signer-specific). Bridges reference the contractCid. A kit that returns one when it means the other breaks witness convergence.
- **Putting substrate-relevant data in body.** If the verifier needs to read it to apply an invariant, it belongs in the header. Adding it to body silently disables the verifier's ability to enforce the invariant.
- **Putting decorative data in header.** Source paths, build commits, kit version strings: these are body. Adding them to the header changes the contractCid for byte-equal contracts produced by different toolchains, which breaks the substrate guarantee that anyone holding the same contract content computes the same identity.
- **Re-canonicalizing on round-trip.** Reading an envelope, mutating a body field, and re-encoding produces a new attestation with a new attestationCid and a new signature. The contractCid is unchanged because the contract content is unchanged. This is correct behavior. The original and modified attestations are different mementos sharing one contractCid.
- **Signing the JCS of the envelope object instead of the array `[header, body]`.** Producers in some languages reach for "JCS the whole envelope minus signature" by reflex. The protocol pins the payload as `JCS([header, body])` specifically so that the envelope's `signer` field can hold a self-identifying public key without recursion through the signed-over bytes.

## When this step is done

The `claim_envelope` fixture passes. Your kit's verifier accepts contract envelopes the Rust kit produces. Your kit's signer produces envelopes the Rust kit verifies. Your `contract_cid` function returns the same value as another kit's on byte-identical declarations and is independent of signer state. attestationCids differ across signers; contractCids do not.

## Read next

- [04-proof-envelope.md](04-proof-envelope.md): the bundle layer, where many signed mementos travel together.
- [`2026-05-03-substrate-layers-envelope-header-body.md`](../../../protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md): the canonical definition of the envelope/header/body cut.
- [`2026-05-03-contract-cid-vs-attestation-cid.md`](../../../protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md): the contractCid / attestationCid separation and witness convergence rules.
- [docs/security/multi-dimensional-pinning.md](../../security/multi-dimensional-pinning.md): why one CID is never enough; how contractCid composes with witnessCid and binaryCid.
