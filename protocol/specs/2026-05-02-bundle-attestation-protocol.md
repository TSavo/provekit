# Bundle Attestation Protocol

**Status:** v1.4.0 normative draft
**Date:** 2026-05-02
**Catalog property:** Targeted for the v1.4.0 catalog as `bundle-attestation-protocol`; CID is computed from this file's bytes per `2026-04-30-protocol-catalog-format.md` §2.1 (raw-byte BLAKE3-512). No CID line appears in this header by design: a self-referencing CID would invalidate on every edit, which is the very pathology this spec retires.
**Owner:** producers minting any content-addressed artifact (catalogs, self-contracts bundles, binaries, `.proof` bundles, source-tree archives) and the verifier crate consuming them.
**Related:**
- `2026-05-02-binary-attestation-protocol.md` (binary-specific instance of this shape; this spec is the generalization)
- `2026-04-30-protocol-catalog-format.md` (catalog hashing rules; catalog signatures are the historical first instance of this shape)
- `2026-04-30-proof-file-format.md` (the `.proof` envelope grammar that consumes attestations as members)
- `2026-04-30-canonicalization-grammar.md` (JCS canonicalization, normative for the signed payload)
- `2026-04-30-signatures-and-non-repudiation.md` (signature semantics)

## §1. Scope and motivation

### §1.1 The pathology

A content-addressed artifact cannot know its own CID. The artifact is the bytes that hash to the CID; including the CID inside the artifact changes the bytes, which changes the CID, which invalidates the inclusion. Self-reference is a fixed-point problem with no general solution.

ProvekIt's source tree currently carries the pathology in several forms:

- `Makefile` constants of the form `RUST_CID := <literal>`, `GO_CID := <literal>`, `CPP_CID := <literal>`, `CATALOG_CID := <literal>`, baked into the build system so a code change requires hand-editing a claim about the new bytes the build produces.
- `.proof` bundle scaffolds where the producer's expected output CID is hand-edited into surrounding code (test fixtures, scripts, READMEs, prompts) that share a git tree with the producer.
- Documentation that pins a bundle's CID in the same repository that produces the bundle.

Each of these is a manual signing dance dressed as a checksum. The constant claims a hash; the hash is not verified by anything; a wrong constant ships and the build still completes; the wrongness is discovered out of band, days later, by a verifier that does the actual hash.

### §1.2 The shape

Separate the artifact (the **letter**) from the attestation (the **envelope**). The envelope carries the artifact's CID, signed. The artifact never claims its own hash. References run only one way: envelope to letter.

The verifier does the work the constant pretended to:

1. Hash the artifact bytes.
2. Read the envelope.
3. Confirm the envelope's CID matches the recomputed hash.
4. Verify the envelope's signature.

If any step fails, the verifier rejects. There is nothing to hand-edit; if the artifact bytes change, the envelope's CID stops matching, the verifier rejects, and the producer mints a new envelope.

### §1.3 Generalization

This pattern has shipped twice in ProvekIt under different names. `2026-05-02-binary-attestation-protocol.md` defines it for binaries (the binary's CID lives in a `.proof` bundle, never in the binary). The catalog signature files at `.provekit/catalog-signatures/v*.json` define it for protocol catalogs (the catalog's CID lives in a separately signed JSON file, never in the catalog).

This spec is the generalization. Catalog signatures, binary attestations, and any future content-addressed artifact's attestation are three instances of one shape. This spec names the shape and codifies the rules every instance MUST follow.

## §2. Terminology

- **Letter**: the content-addressed artifact whose CID is being attested. May be a binary, a `.proof` bundle, a self-contracts bundle, a catalog, a source-tree archive, or any other byte sequence with a content-identifier.
- **Envelope**: a separately-signed JSON attestation file carrying the letter's CID, kind, name, declaration timestamp, signer pubkey, and signature.
- **Producer**: the entity that signs envelopes. Examples: a foundation key, a vendor key, a project key, a third-party attester.
- **CID**: content-identifier in `<algorithm>:<digest>` form (e.g. `blake3-512:abc...`). Per `2026-04-30-protocol-catalog-format.md` §2.
- **Pin**: deprecated term for the broken self-reference pattern this spec retires (see §7).

## §3. Envelope file format (NORMATIVE)

A bundle attestation is a JSON object with exactly the following fields:

| Field | Type | Description |
|---|---|---|
| `kind` | string | MUST be the literal `"bundle-attestation"`. Distinguishes envelopes governed by this spec from earlier instances such as catalog signatures. |
| `artifactKind` | string | RECOMMENDED values: `"catalog"`, `"self-contracts"`, `"binary"`, `"proof-bundle"`, `"source-tree"`. Other values MAY appear; consumers MUST handle unknown kinds per §3.2. |
| `artifactName` | string | Identifier for the artifact within its kind. For `"catalog"` this is the catalog version (e.g. `"v1.3.1"`). For `"self-contracts"` this is the language kit name (e.g. `"rust"`, `"go"`, `"cpp"`). For `"binary"` this is the binary's logical name. For `"source-tree"` this is the project name. |
| `cid` | string | The letter's content-identifier in `<algorithm>:<digest>` self-identifying form. |
| `declaredAt` | string | ISO-8601 UTC timestamp of when the envelope was minted. |
| `signer` | string | Producer pubkey in `<algorithm>:<base64>` form (e.g. `"ed25519:IVL40Zt5..."`). |
| `signature` | string | Detached signature in `<algorithm>:<base64>` form. The signed bytes are defined in §3.1. |

Example (the canonical shape this spec defines):

```json
{
  "kind": "bundle-attestation",
  "artifactKind": "self-contracts",
  "artifactName": "rust",
  "cid": "blake3-512:0123abcd...",
  "declaredAt": "2026-05-02T17:00:00Z",
  "signer": "ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=",
  "signature": "ed25519:phj9HlpP0af..."
}
```

### §3.1 INVARIANT BundleAttestation.SignaturePayload (NORMATIVE)

The bytes signed by `signature` MUST be the JCS-canonical encoding (per `2026-04-30-canonicalization-grammar.md`) of the six-field payload:

```json
{
  "kind": "bundle-attestation",
  "artifactKind": <string>,
  "artifactName": <string>,
  "cid": <string>,
  "declaredAt": <string>,
  "signer": <string>
}
```

The `signature` field is NOT included in the signed bytes. JCS sorts keys lexicographically, eliminates insignificant whitespace, and produces a single canonical byte sequence per logical payload. Producers and verifiers MUST agree on those bytes; the only conformant rule is JCS.

This is the same rule the catalog signature files at `.provekit/catalog-signatures/v*.json` already follow. It is codified here as the rule for ALL future envelopes.

### §3.2 INVARIANT BundleAttestation.Wellformed (NORMATIVE)

All seven fields (`kind`, `artifactKind`, `artifactName`, `cid`, `declaredAt`, `signer`, `signature`) MUST be present and non-empty.

Any missing field is a schema violation. Any extra top-level field is a schema violation. A verifier MUST refuse a malformed envelope before performing any other check (signature verification, CID match, signer trust).

Unknown `artifactKind` values MUST be parsed (the envelope is well-formed) but MUST NOT be accepted as conformant by a consumer that does not recognize the kind. The fail-closed default applies: tolerate parsing, refuse acceptance. A consumer that does not understand `artifactKind = "future-kind-name"` rejects the envelope; it does not silently degrade to a generic check.

### §3.3 INVARIANT BundleAttestation.KindLiteral (NORMATIVE)

The `kind` field MUST be the exact literal string `"bundle-attestation"`. Verifiers MUST reject envelopes whose `kind` field is any other value, including the historical `"catalog-signature"` shape (legacy catalog signatures are recognized by file location, not by a `kind` field they never had; see §6.1).

## §4. Verification (NORMATIVE)

Given an artifact at path `P` and a candidate envelope at path `E.json`:

1. Read `envelopeBytes := bytes(E.json)`. Parse as JSON. Reject if parse fails.
2. Validate the parsed object against §3.2 (Wellformed). Reject if any check fails.
3. Validate `envelope.kind == "bundle-attestation"` per §3.3. Reject otherwise.
4. Parse `envelope.cid` into `(algorithm, digest)`. Compute `actualCid := algorithm + ":" + hex(algorithm.hash(bytes(P)))`.
5. Check `envelope.cid == actualCid`. Otherwise fail closed: `BundleCidMismatch { expected: envelope.cid, actual: actualCid }`.
6. Recompute `signedPayload := JCS({kind, artifactKind, artifactName, cid, declaredAt, signer})` from the parsed envelope's six payload fields.
7. Parse `envelope.signer` into `(algorithm, pubkey)` and `envelope.signature` into `(algorithm, sigbytes)`. The two algorithms SHOULD match (an Ed25519 pubkey signs Ed25519 signatures).
8. Verify `sigbytes` against `signedPayload` using `pubkey`. Otherwise fail closed: `BundleSignatureInvalid`.
9. Check that `envelope.signer` is in the consumer's accepted-producer set. Otherwise fail closed: `BundleSignerNotTrusted { signer: envelope.signer }`.

Steps 1 through 8 are protocol-level; step 9 is consumer policy. A verifier MAY surface step 9's signer to the consumer for an interactive trust decision; a build-time verifier MUST consult a configured trust policy.

### §4.1 INVARIANT BundleAttestation.VerifierProcedureIsTotalOrder (NORMATIVE)

Steps 1 through 9 are gating. A verifier MUST NOT skip a step. A verifier MUST NOT accept the envelope unless every step passes.

Specifically, a valid signature does NOT compensate for a CID mismatch (step 5 vs step 8 are separate gates), and a CID match does NOT compensate for an invalid signature. Both are required.

### §4.2 RECOMMENDED CLI

Producers SHOULD provide a CLI verb to invoke this verification:

```
provekit verify-bundle-attestation <artifact-path> <envelope-path>
```

The existing `provekit verify-protocol --signed` is the first instance of this verb (specialized to catalog letters and the legacy catalog-signature envelope shape). Future cuts SHOULD subsume that command into the generic verb.

## §5. Production (NORMATIVE)

Given an artifact at path `P` whose CID a producer wishes to attest, and a producer keypair `(skProducer, pkProducer)`:

1. Compute `cid := <algorithm>:<digest>` over `bytes(P)` using the algorithm permitted by the consuming protocol catalog's `algorithms.hash` entry.
2. Construct the payload:
   ```
   payload = {
     kind: "bundle-attestation",
     artifactKind: <one of the §3 RECOMMENDED values, or a registered extension>,
     artifactName: <string identifying the artifact within its kind>,
     cid: <computed in step 1>,
     declaredAt: <ISO-8601 UTC timestamp of now>,
     signer: <pkProducer in algorithm:base64 form>
   }
   ```
3. Compute `signedBytes := JCS(payload)`.
4. Compute `signature := <algorithm>:<base64(sign(skProducer, signedBytes))>`.
5. Write the envelope as JSON: the six payload fields plus `signature`. Producers MAY format the JSON with whitespace for human readability; the canonical form for signing is JCS, but the file on disk MAY be pretty-printed because the file is parsed before verification.

### §5.1 INVARIANT BundleAttestation.SignAfterHash (NORMATIVE)

`cid` MUST be computed from the final artifact bytes, after all transformations the producer applies (compression, stripping, signing of nested artifacts). A producer that hashes a pre-final artifact and signs that hash ships a mismatched envelope; verifiers will reject (per §4 step 5).

### §5.2 RECOMMENDED CLI

Producers SHOULD provide a CLI verb to mint envelopes:

```
provekit attest <artifact-path> --kind <artifactKind> --name <artifactName> [--out <envelope-path>]
```

Foundation-keygen's existing `sign-catalog-v1-3-1` and any `sign_self_contracts` entry point are specialized instances. Both SHOULD be subsumed by this generic verb in a future cut.

## §6. Relationship to existing specs

### §6.1 Catalog signatures

The catalog signature files at `.provekit/catalog-signatures/v*.json` are the historical first instance of this spec's shape. The fields in those files map onto bundle-attestation fields as follows:

| Catalog signature field | Bundle attestation equivalent |
|---|---|
| `schemaVersion: "1"` | (no equivalent; legacy field, not carried forward) |
| `protocolName: "provekit-protocol"` | implicit in `artifactKind: "catalog"` |
| `protocolVersion: "v1.3.1"` | `artifactName: "v1.3.1"` |
| `catalogCid` | `cid` |
| `declaredAt` | `declaredAt` (unchanged) |
| `signer` | `signer` (unchanged) |
| `signature` | `signature` (unchanged) |

Legacy catalog signatures have NO `kind` field. Verifiers identify them by file location (`.provekit/catalog-signatures/<version>.json`) and by the presence of `catalogCid` rather than `cid`. Future cuts SHOULD migrate to the bundle-attestation shape with `kind: "bundle-attestation"` and `artifactKind: "catalog"`.

### §6.2 Binary attestations

`2026-05-02-binary-attestation-protocol.md` is the binary-specific instance of this spec. The `.proof` bundle's `binaryCid` field plays the role of `cid`; the binary's filename (or a manifest entry) plays the role of `artifactName`; `artifactKind` is implicitly `"binary"`.

The two specs are consistent. Where they conflict, binary-attestation wins for binaries (more specific). Where binary-attestation is silent, this spec governs. The two-pin closure of binary-attestation §5 is a binary-specific elaboration of the letter-envelope shape; nothing in this spec contradicts it.

### §6.3 Self-contracts attestations

A self-contracts bundle is a JSON file produced by a per-language kit listing the contracts the kit asserts about its own implementation. A self-contracts envelope has `artifactKind: "self-contracts"` and `artifactName` equal to the language kit name (`"rust"`, `"go"`, `"cpp"`, etc.).

When the parallel implementation work lands, self-contracts envelopes MUST conform to §3 of this spec.

### §6.4 Migration path

- v1.4.0: bundle-attestation-protocol promoted into the catalog. New envelopes (binary attestations, self-contracts attestations) emit the `kind: "bundle-attestation"` shape natively.
- v1.4.x: catalog signatures continue to use the legacy shape; verifiers tolerate both shapes for catalog letters.
- v1.5.0+: deprecate the legacy catalog-signature shape. Catalog signatures emit `kind: "bundle-attestation"` and `artifactKind: "catalog"`. Verifiers MAY refuse the legacy shape entirely (scorched earth, since back-compat shims are net negative once the cut is complete).

## §7. The pin pattern is deprecated (NORMATIVE)

A source tree MUST NOT carry constants of the form

```
<NAME>_CID := <literal CID>
```

or any equivalent (variable assignment, hardcoded string literal, embedded comment claiming a CID, documentation footnote pinning a CID) for content the same source tree produces.

### §7.1 INVARIANT BundleAttestation.NoSelfReferentialPin (NORMATIVE)

For any path `Q` inside a producing repository's source tree, and any literal CID `c` appearing at `Q`, the following test MUST hold:

> **The discriminator.** If a code change at any other path in the same source tree CAN cause the artifact to which `c` refers to be reproduced with a different CID, then `c` is a self-referential pin and MUST NOT appear at `Q`.

Equivalent informal rule: "if I change a line of code in this tree, does this pin need updating?" If yes, the pin is self-referential. Retire it.

The discriminator is mechanical and decidable: it is a static check over the producing repository's build graph. A future contract memento can encode it directly.

### §7.2 Sites this rule applies to

- `Makefile` constants of the form `*_CID :=` for self-contracts bundles, language-kit binaries, source-tree archives, or any other artifact the same Makefile produces.
- Test fixtures that mint a bundle from the producing source tree and assert the resulting CID against a hardcoded literal.
- Documentation (`README.md`, status files, postmortems, prompts) that pins a bundle's CID inside the git tree that produces the bundle.
- IR sources, schemas, or grammars that reference the CID of a sibling artifact in the same producing tree.

For each site, the replacement is the same: emit a bundle attestation envelope (per §3 and §5) carried OUTSIDE the producing source tree (or in an attestation directory excluded from the build inputs that produce the artifact), and verify per §4 at build time.

### §7.3 What this rule does NOT cover

Pins for artifacts produced by OTHER repositories are not self-referential and are NOT deprecated. Examples:

- A `Cargo.toml` lockfile referencing the CID of an upstream crate.
- A protocol catalog referencing the CIDs of foundation-published spec files (the catalog and the specs are produced by separate processes; the catalog references specs as opaque content).
- A `.proof` bundle's `binaryCid` field referencing a binary built by a different repository.

The discriminating question (§7.1) is the test: changing a line in a consuming repository does not cause an upstream artifact to be reproduced, so the pin is stable.

## §8. Open questions

These are surfaced for resolution in a future cut.

- **Signer registry vs raw pubkey.** This spec encodes `signer` as a raw pubkey. A future spec MAY define a signer registry (a memento mapping human-readable names to pubkeys) so consumers can express trust at the name layer. For now, registry membership is consumer policy, not protocol surface.
- **Attestation file paths.** This spec does not mandate an on-disk layout. A SHOULD-level recommendation: store envelopes at `.provekit/attestations/<artifactKind>/<artifactName>.json`. Catalog signatures already live at `.provekit/catalog-signatures/<version>.json`; future versions SHOULD migrate to the nested path with `<artifactKind> = "catalog"`. Out of scope for this cut.
- **Multi-signer envelopes.** A single artifact may warrant attestation from multiple producers (foundation + vendor + auditor). This spec defines a single-signer envelope; multi-signer support is a possible v1.5.x extension via either an envelope array or a `cosigners` field. Deferred.
- **Revocation.** A producer may wish to retract an envelope (key compromise, mistaken attestation). Per the analogous decision in `2026-05-02-binary-attestation-protocol.md` §12, revocation is a trust-layer concern, not a protocol-layer concern. Consumers wanting revocation define it at the trust layer. Open.
- **Algorithm agility.** This spec inherits algorithm choices from the consuming catalog's `algorithms` entry. If a future catalog admits multiple hash algorithms or multiple signature algorithms, the envelope's self-identifying CID and signer prefixes already disambiguate. No spec change anticipated.

## §9. What this spec is NOT

- **Not a runtime protocol.** This spec governs build-time and release-time attestation. It does not define on-wire handshake, network transport, or runtime resolution.
- **Not a key-management spec.** Producers manage their own keys; key generation, storage, rotation, and revocation are out of scope.
- **Not a transport spec.** Envelopes are files. How they reach consumers (cache, registry, sidecar in a release tarball, content-addressed lookup) is consumer-defined.
- **Not a multi-version catalog merge.** A consumer with envelopes from multiple protocol catalog versions resolves trust per its own policy.
- **Not a contract-discharge spec.** The `discharges` field on `.proof` bundles is governed by `2026-04-30-proof-file-format.md` and `2026-04-30-memento-envelope-grammar.md`. This spec governs the CID-attestation surface only.

## §10. Conformance

This spec is satisfied by:

- A reference verifier that performs §4 steps 1 through 9 end-to-end on every envelope shape (`catalog`, `self-contracts`, `binary`, `proof-bundle`).
- A reference producer (`provekit attest` or equivalent) that mints envelopes per §5.
- Integration tests covering:
  - The §4 happy path: an artifact at `cid`, an envelope claiming that `cid`, valid signature, trusted signer; verifier accepts.
  - The §4 step 5 negative path: an envelope's `cid` mutated post-signing; verifier rejects with `BundleCidMismatch` despite valid signature.
  - The §4 step 8 negative path: an envelope's `signature` corrupted; verifier rejects with `BundleSignatureInvalid` despite matching `cid`.
  - The §3.2 schema-violation path: an envelope missing a required field; verifier rejects before any cryptographic check.
  - The §3.3 kind-literal path: an envelope with `kind: "wrong-string"`; verifier rejects.
  - The §7.1 discriminator: a CI check that scans the source tree for self-referential pins and fails the build if any are found.

## §11. Related specs

- `2026-05-02-binary-attestation-protocol.md`: binary-specific instance of this shape; the two-pin closure of §5 is a binary-specific elaboration.
- `2026-04-30-protocol-catalog-format.md`: catalog hashing rules; catalog signatures are the historical first instance.
- `2026-04-30-proof-file-format.md`: `.proof` bundle envelope grammar; bundle attestations may be carried as members.
- `2026-04-30-canonicalization-grammar.md`: JCS canonicalization, normative for the signed payload.
- `2026-04-30-signatures-and-non-repudiation.md`: signature semantics.
- `2026-04-30-memento-envelope-grammar.md`: general memento shape; bundle attestations are a specialized memento kind.
