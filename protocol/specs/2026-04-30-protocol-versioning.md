# ProvekIt: Protocol Versioning via Self-Reference

> Author: shared session 2026-04-30 (T + Claude). The protocol is
> content-addressed, including the protocol's own spec.

The protocol's spec is itself a catalog memento (per the
memento-envelope-grammar spec). The catalog's `properties` map names
each spec document to its content-addressed CID. The catalog's own
CID is the protocol version.

## v1.1.0

Protocol catalog: `protocol/specs/2026-04-30-protocol-catalog.json`

**Protocol version CID:**

```
blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106
```

(v1.1.0; previous was `sha256:a2d062341e3ca0f0` for v1.0.1, and
`sha256:e04b7cc466911b1d` for v1.0.0. v1.1.0 retires SHA-256 and
truncated CIDs entirely; the protocol now uses BLAKE3-512 only,
with full 128-hex CIDs.)

**How the catalog CID is computed.** The catalog file as committed
(`2026-04-30-protocol-catalog.json`) is human-readable JSON: insertion
order, two-space indent, trailing newline. The CID is NOT computed
over those file bytes. The CID is computed over the JCS-canonical
form (RFC 8785) of the same JSON data: object keys sorted by
Unicode code-point, no whitespace, U+0000..U+001F escaped as
`\u00XX`. This is the same canonicalization the protocol uses for
every memento envelope (canonicalization-grammar pass 7), so a
verifier reads the catalog file, parses it, JCS-encodes it, and
BLAKE3-512s the result. The repository ships a reference
implementation in `tools/recompute-spec-cids/` that anyone can
re-run; `--verify` mode re-derives every CID and fails on any
drift.

**This CID is `provekit.proofHash` for ProvekIt itself.** The same
field a library carries in its `package.json` to declare its
proof-chain root, ProvekIt carries to declare its own protocol
version. The framework eats itself: a library's proofHash is the
CID of its property catalog; ProvekIt's proofHash is the CID of
its protocol-spec catalog. Same primitive, same field name, same
math. ProvekIt is one more library, and the protocol is one more
property catalog.

This CID names a catalog whose entries are (each spec doc's CID is
`blake3-512:` + 128 lowercase hex chars; spec doc CIDs are computed
over the raw .md file bytes, no canonicalization):

| Spec | CID |
|---|---|
| ir-formal-grammar | `blake3-512:6c0127e0d24946d7be75861db20507ccdcfdf968d3333f8aa34083e849d8238d73b3acfaa31880648995a024112182ed6b6002cd489548b4b18f5d4c3768dd96` |
| canonicalization-grammar | `blake3-512:4d8c2940c53a59c678c8fb65e33dc2cb0ae8ae8a283b97b9c69fd678565653d15e6ee9dc3ffc6a32dc1ff035821b0c1a006f0455498d2ea91faef845d7b39830` |
| memento-envelope-grammar | `blake3-512:58bba3e1a9f6439eac5cb0c681faf65d38de9e6b8ad539854acda451ca67562a9d238eb95a5d7df2c0776657015fa026c51059dff61e1ba9aa2438b57425d6a5` |
| signatures-and-non-repudiation | `blake3-512:8b71229fcb7413f18a93a9b260012298311c1ce754850ee717780c181f1fda39a6600b2e5069e775cd7dd15e8c81e40b47bf7585aa0b23ab76c112c85116365c` |
| chain-validity-and-fail-closed | `blake3-512:dd905e8660d855c0c8140ceaafb0e189391234ff981422e714644b23642b24d7ca0253c76d09b46e58c5f8d8362cc8efca436baf2be927ab63a7bacf356e7673` |
| ir-extension-protocol | `blake3-512:cff64b923879548fd54efb63c5ea116ba184adadec126e956387f1ee9d0f7907edbdb1a05866d62442a6fb2142948654464e63830a061efdaed8af9403bb0c13` |
| proof-file-format | `blake3-512:7bb4589af25c6c3992520494869bbbe4cfbcf7a77b91ebd61d6327e78699ef16cd5bc34afbe4cdf88a717c055c16536b5106bc4dca2d9d6b5cfcc1eede68e1b3` |
| semantic-envelope | `blake3-512:6b14a0c4a36877ea77a609f5262257fb4c65940e8e5eefc03647746525d052216761b1707384bffec5fb3c021ac0e07e45d390efefa2c7f5c67c045d93352b56` |
| supply-chain-via-semantic-envelope | `blake3-512:b924576310bf2defcc2e346e01bdaff6dbdb2a33b2554178e3786e484cb4a4da136443f17675a6cd939fcbcff8afb27396222b5b6649a6eaa31672f5446b15d0` |
| handshake-algorithm | `blake3-512:acbf67dda9373c648e591d8ad74b8f8d56f4c92ba9c82bdc6690dc521e6f17012dd195e98a96b099090eeeb5a424312d90ff441c882d0e317a190561aa1a6925` |
| per-language-kit-standard | `blake3-512:7d3e72d58c87864eea2b7b330096d2cc4591292c1905baa447d4f74b8d80327521e284fc37f874fae80ba8f170a2456aed27c37215ee8752f8fd57e2d60b0f88` |
| lattice-tractability-theorem | `blake3-512:b6d7c2772c2929294d7f516f79559bd292e44f51805a6bd6ea0ca7fe365b82ec96b86c434f53dfb003f5acd306533831dc0257e46ead4c7d71081f9f56ec6d07` |
| contract-merge-semantics | `blake3-512:aeb9e2c56603f56372c29cdcbbb11ec3ae6fada0b2004d9fb99955e21230b03a72d02fc7051eea09ed24b7271b758909011948b531b668bb5c20e8ab8a268bee` |

## Conformance declarations

A reference implementation declares which protocol version(s) it
conforms to via the same shape any consumer references a library:

```yaml
# in an implementation's metadata
provekit-protocol-conformance:
  - cid: blake3-512:9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106
    version: v1.1.0
```

A verifier that holds the catalog memento at that CID can check, for
each spec entry, that the corresponding spec's bytes hash to the
listed CID. If any drift, the implementation has changed the spec
without bumping the version — a protocol violation.

## Signing

The catalog itself is JSON, not a memento envelope, and is shipped
with an `_unsigned` field that retains its original draft notes (see
the bootstrap subsection below). Catalog **attestation** lives in a
separate sidecar file:

```
.provekit/catalog-signatures/v1.1.0.json
```

The attestation is the canonical bytes of a six-field JSON object,
signed under the ProvekIt Foundation Root Key:

```json
{
  "schemaVersion": "1",
  "protocolName": "provekit-protocol",
  "protocolVersion": "v1.1.0",
  "catalogCid": "blake3-512:<hex>",
  "declaredAt": "<iso8601>",
  "signer": "ed25519:<base64-pubkey>"
}
```

The signature is computed over the JCS-canonical bytes (RFC 8785) of
that six-field object, then attached as a seventh field (`signature`,
`ed25519:<base64>`). Verifiers reconstruct the six-field message from
the file (dropping `signature`), JCS-encode, and verify against the
foundation public key.

### Foundation Root Key (v0)

```
public key: ed25519:IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=
```

Pinned at `.provekit/keys/foundation-v0.pub`. The CLI binary embeds
this file at compile time so `verify-protocol --signed` works without
filesystem state.

**v0 uses a deterministic, publicly-known test seed (`[0x42; 32]`).**
The seed is checked into `tools/foundation-keygen/src/lib.rs` as
`FOUNDATION_V0_SEED`. Anyone running `cargo run --release --bin
foundation-keygen && cargo run --release --bin sign-catalog` produces
the identical keypair and identical signature byte-for-byte.

This is the bootstrap solution, not the production solution. A
signature under the v0 key is structurally valid but the trust anchor
is "the bytes match the public seed in this repo." Anyone can forge
a v0 attestation. v0 exists to prove the signature path works
end-to-end before a real key is provisioned.

### Bootstrap problem (v1.1.0)

The protocol catalog itself contains the `signatures-and-non-repudiation`
spec; signing the catalog requires the signature machinery defined
inside it. v1.1.0 resolves this by treating the catalog as JSON (still
flagged `_unsigned` in its inline metadata) and shipping a separate
**signed attestation** file that references the catalog's CID. The
attestation can use the signature machinery the catalog itself
defines, because the attestation is not the catalog.

The remaining gap in v1.1.0 is the trust anchor: the v0 foundation
seed is publicly known. v1.1.0 has the right structure with the
wrong key. Future versions will rotate to a real HSM-backed key, with
the rotation event itself being a signed attestation referencing
both the old and new keys (so verifiers that trusted v0 can chain
forward to v1).

## Signature verification procedure

```
provekit verify-protocol --signed
```

The CLI checks four things and reports each:

1. **CID match** — recomputed CID of the embedded catalog equals the
   expected CID.
2. **attested CID** — the CID claimed by the signed attestation
   matches the expected CID.
3. **signer match** — the attestation's `signer` field matches the
   embedded `foundation-v0.pub`.
4. **signature** — the Ed25519 signature, computed over the
   JCS-canonical six-field message (no `signature` field), verifies
   against the signer's public key.

All four must pass. Override the embedded sources with
`--pubkey-file <path>` and `--signature-file <path>` to verify against
external artifacts (useful for verifying a freshly re-signed catalog
without rebuilding the CLI).

## Versioning rules

1. **Any spec change requires a new catalog CID.** Changing a spec's
   bytes changes its CID, which changes the catalog's `properties`
   map, which changes the catalog's CID. Same content addressing as
   propertyHash: the math forces a version bump.

2. **Implementations pin specific catalog CIDs.** Saying "I support
   v1" without a CID is meaningless; v1 is whatever bytes anyone
   happens to have. CID pinning makes the conformance claim
   verifiable.

3. **Multi-version implementations** declare multiple CIDs. A future
   v1.0.1 that adds a single spec doc can be supported alongside
   v1.0.0 by an implementation that declares both CIDs.

4. **Bootstrap signing.** v1.1.0's catalog JSON is still flagged
   `_unsigned` in its own metadata, but a signed attestation
   (`.provekit/catalog-signatures/v1.1.0.json`) over the catalog's
   CID exists, signed by the v0 foundation key. The signature path
   is real and verifiable; the trust anchor is the deterministic
   test seed `[0x42; 32]`. v1.2 may promote the catalog itself into
   a memento-envelope shape; for v1.1.0 the JSON catalog plus
   sidecar attestation is the shipped contract.

5. **Spec evolution is mechanical.** A working group editing a spec
   produces a new bytes; a new CID; a new catalog candidate; the
   project's signing authority signs the new catalog; the new version
   is published. Implementations decide whether to upgrade by reading
   the diff of the catalog (which spec CIDs changed) and the
   migration spec (if any) for that bump.

6. **Key rotation.** A future v1 foundation key (HSM-backed) will be
   introduced via a rotation attestation: a signed message that
   references both the old (v0) and new (v1) public keys. Verifiers
   that trusted v0 can chain forward; verifiers that only trust v1
   can ignore v0 attestations. The rotation attestation file format
   is a future spec.

## The recursive payoff

ProvekIt's protocol uses content addressing as its core primitive. The
spec describing that protocol is itself content-addressed via the same
machinery. The version of the protocol is a CID. Implementations
verify their conformance via CID comparison. There is no
out-of-protocol authority deciding what "v1.1.0" means; v1.1.0 is
the bytes whose JCS-canonical form hashes (BLAKE3-512) to
`9d57c5e47083b92e8cc5dab365a718fc0afee6556d34ffe40b303dd7ad4d9caa88dbbc6248e318cc76e57b30a0b2ad49f6f9dbf1916ac164a89df44324d6c106`.

This is the same self-reference shape as Git (commit hashes refer to
trees that may include other commits), IPFS (DAG addresses include
references to other DAGs), and Bitcoin (block hashes chain backward
through prior blocks). ProvekIt is one more application of the same
primitive.

The framework's promise is total within its scope. The version is a
CID. Conformance is a CID comparison. The TypeScript implementation
in this repository is one realization; alternative implementations in
any language conform to the same CID-pinned spec set or they are not
ProvekIt.
