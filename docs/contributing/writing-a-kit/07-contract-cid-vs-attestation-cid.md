# Writing a kit, step 7: contractCid vs attestationCid

Two CIDs land on a contract. They are not the same value, they answer different questions, and earlier kit versions conflated them. v1.4 separates them normatively. This step teaches the cut, the function names your kit owes, and the migration if your kit currently exposes only one.

The normative source is [`protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md`](../../../protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md). Read it end to end before touching this layer of your kit.

## The two CIDs

**contractCid.** Hash of the contract's content alone:

```
contractCid := "blake3-512:" || hex(BLAKE3-512(JCS(ContractDecl)))
```

The input is the `ContractDecl` itself, JCS-canonical. No signer, no signature, no timestamp, no envelope field. Anyone holding the same `ContractDecl` bytes computes the same value without consulting a signer.

**attestationCid.** Hash of the signed envelope wrapping the content:

```
ContractAttestation := {
    schemaVersion: "1",
    kind:          "contract-attestation",
    contractCid:   <the value above>,
    signer:        <ed25519 pubkey>,
    declaredAt:    <ISO-8601 UTC timestamp>,
    signature:     <ed25519 over JCS({contractCid, signer, declaredAt})>
}

attestationCid := "blake3-512:" || hex(BLAKE3-512(JCS(ContractAttestation)))
```

The input is the envelope. Change the signer, the timestamp, or the signature and the attestationCid changes. The contractCid inside it does not.

The contractCid identifies a contract. The attestationCid identifies one signer's attestation about that contract.

## Worked example: same contract, two signers

A `ContractDecl` for the obligation `len(arr) <= MAX_LEN`:

```json
{
    "kind": "contract",
    "name": "len_bounded",
    "pre":  { "kind": "true" },
    "post": { "kind": "le", "lhs": { "kind": "len", "of": "arr" },
                            "rhs": { "kind": "var", "name": "MAX_LEN" } }
}
```

Two developers sign attestations over this declaration on different days with different keys.

Signer Alice (`ed25519:AAAA...`, `2026-05-03T09:00:00Z`):

```
contractCid    = blake3-512:9f1e... (function of the JSON above only)
attestationCid = blake3-512:c204... (function of envelope: contractCid + Alice + 9:00 + sig)
```

Signer Bob (`ed25519:BBBB...`, `2026-05-04T11:00:00Z`):

```
contractCid    = blake3-512:9f1e... (same: same content, same canonical bytes)
attestationCid = blake3-512:7d83... (different: different signer, time, sig)
```

Same `contractCid`. Different `attestationCid`s. This is the property that makes witness chains work.

## The five identity rules (R1 to R5)

Direct from `2026-05-03-contract-cid-vs-attestation-cid.md` §2. Your kit must respect all five.

**R1.** The contractCid identifies a contract. Two declarations with byte-identical JCS produce the same contractCid regardless of signer, time, or signing status.

**R2.** The attestationCid identifies an attestation. Two attestations differ in attestationCid if they differ in signer, declaredAt, or signature, even when their underlying contractCid is the same.

**R3.** Bridges reference contractCids, not attestationCids. A `Bridge` declaration's `sourceContractCid` and `target.cid` (when `target.kind == "contract"`) MUST hold contract CIDs. Implementations MUST NOT use attestation CIDs in these fields. (See [`06-bridge-IR.md`](06-bridge-IR.md).)

**R4.** Multiple distinct attestations MAY exist for the same contractCid. They form a witness set per `2026-05-02-bundle-attestation-protocol.md` §7. Each contributes its own attestationCid; all share the underlying contractCid.

**R5.** An implementation MUST expose a function returning the contractCid of an input declaration, computed without consulting any signer, signature, or external artifact.

R5 is the API obligation R1 through R4 imply: kits that lacked a content-only function must add one.

## Naming convention

R5 names the function. Per host language idiom:

| Language          | Signature                           |
|-------------------|-------------------------------------|
| Rust              | `contract_cid(&decl) -> Cid`        |
| Go                | `ContractCid(decl) Cid`             |
| TypeScript        | `contractCid(decl): Cid`            |
| C#                | `ContractCid(decl) -> Cid`          |
| Swift             | `contractCid(decl) -> CID`          |
| Java              | `contractCid(decl)`                 |
| Python            | `contract_cid(decl) -> Cid`         |
| Ruby              | `contract_cid(decl)`                |
| C                 | `contract_cid(decl, out)`           |
| Zig               | `contract_cid(decl)`                |

snake-case for Rust, Python, Ruby, C, Zig. camelCase (with a Pascal-cased entry point where the language demands it) for Go, TypeScript, C#, Swift, Java.

Public APIs that return any CID must document which kind they return. A function called `contract_cid` that secretly returns an envelope hash is the bug this spec exists to retire.

## Witness convergence

Witness convergence is the operational consequence of R1 and R4. It works by content equality on the contractCid:

- An auditor walking the DAG for "all witnesses of contract C" filters attestations by `attestation.contractCid == C`. The filter is a byte comparison on a content hash, not a signer-dependent value. Every signer who attested the same content is in the result set.
- A peer kit bridges to a contract owned by another kit by computing the contractCid locally from the contract declaration's bytes (extracted from the source file or a published JSON of declarations), then writing that contractCid into `target.cid` of its bridge. The peer kit never reads the original signer's mint binary, never trusts a signer-specific value. The substrate's closure property holds: every CID in a header is computable from content.
- A second signer countersigning a contract uses the same contractCid; their attestation extends the witness chain without minting a new contract identity. Witnesses converge instead of forking per signer.

If your kit emits an attestationCid where the substrate expects a contractCid, none of these operations work. Each signer becomes a separate "contract" and the witness set fragments.

## Implementation guidance

If your kit currently exposes only an envelope-hash CID (Rust's pre-v1.4 `mint_contract` returned the signed-envelope hash and called it the "contract CID"), do three things.

1. **Add the contractCid function.** Per R5 and the naming table above. Pure content hash. No signer state in the input. No external lookup. Same input bytes, same output bytes, every run, every kit, forever.
2. **Rename the envelope-hash function.** If a function currently called `contract_cid` returns an envelope hash, rename it. `attestation_cid` or `signed_contract_cid` are reasonable. The function MAY be retained for attestation operations; it MUST NOT keep a name that claims content-only semantics.
3. **Update `mint_contract`'s return.** The flow remains correct; the return shape changes.

## The mint_contract flow

`mint_contract` now returns both CIDs:

```
mint_contract(decl, signer_key) -> (contractCid, attestationCid, attestation_envelope)
```

Where:

- `contractCid` is `contract_cid(decl)`. Bridges reference this. Witness queries filter on this.
- `attestationCid` is the hash of the signed envelope. The on-disk attestation file is identified by this.
- `attestation_envelope` is the envelope itself, ready to write.

Callers that need to bridge to the contract take the first value. Callers that need to address the specific attestation (for example, to fetch it from a content-addressable store) take the second. Callers that need the bytes to write to disk take the third.

A kit that returns only one of these three forces the caller to recover the others, which the caller cannot always do without re-signing or re-canonicalizing. Return all three.

## Migration: on-disk attestation files

Existing kits write self-contracts attestations to `.sugar/self-contracts-attestations/<lang>.json`. Pre-v1.4 files surface only an envelope hash. v1.4 requires both CIDs:

```json
{
    "schemaVersion":    "1",
    "kind":             "self-contracts-attestation",
    "lang":             "rust",
    "contractBundleCid":"blake3-512:<bundle JCS hash>",
    "attestationCid":   "blake3-512:<envelope hash>",
    "signer":           "ed25519:...",
    "declaredAt":       "...",
    "signature":        "..."
}
```

`contractBundleCid` is the content-only hash of the bundle (R5 applied to the JCS-canonical array of declarations). `attestationCid` is the envelope hash. Tooling that pinned to the envelope hash continues to work via `attestationCid`. Tooling that wants signer-independent identity uses `contractBundleCid`.

Bridges minted before v1.4 MAY reference attestation CIDs. They remain valid as historical witness statements. New bridges MUST use contractCids per R3.

## Common mistakes

- **Using attestationCid where R3 requires contractCid.** Bridge fields, witness-set filters, peer-kit references. A `target.cid` populated from the wrong function silently fragments witness sets.
- **Treating the two CIDs as interchangeable.** They are not. They answer different questions and they cannot substitute for each other. A function that "returns the CID" without saying which one is the bug.
- **Hashing the wrong canonical form for contractCid.** The input is JCS of the `ContractDecl`. Not JCS of the envelope. Not the envelope minus the signature. Not a CBOR encoding. JCS of the declaration. Confirm against the conformance fixture.
- **Including signer state in the contractCid hash input.** No pubkey, no signature, no timestamp, no key fingerprint. If your function reads a signer key to compute the contractCid, the function is wrong. R5 is explicit: computed without consulting any signer.
- **Pinning attestationCids in tooling that wants identity.** A package manifest or version chain that pins an attestationCid pins one signer's day. The next signer of the same contract produces a different attestationCid; the pin breaks. Pin the contractCid.

## Conformance test

Per `2026-05-03-contract-cid-vs-attestation-cid.md` §6, your kit's self-contracts test suite SHOULD include `contract_cid_is_signer_independent` (or the equivalent name in your language's test idiom). It MUST assert:

1. `contract_cid(decl)` returns the same value across two runs with the same declaration bytes.
2. `contract_cid(decl)` returns the same value as another kit's `contract_cid` on the same declaration. Cross-kit byte equality.
3. Two `mint_contract` calls with different signer keys but the same declaration produce the same `contract_cid` and different `attestation_cid`.
4. New `Bridge` declarations have `sourceContractCid` and `target.cid` whose values match `contract_cid` of the underlying declarations, NOT `attestation_cid`.

Property (3) is the load-bearing one. It catches the entire failure mode this spec exists to prevent: a kit that hashes signer state into the contractCid passes (1) and (2) but fails (3).

## When this step is done

Your kit exposes `contract_cid` (or the camelCase equivalent) per R5. The function takes a declaration and returns a content-only hash. `mint_contract` returns both contractCid and attestationCid. The on-disk self-contracts attestation surfaces both CIDs in the v1.4 shape. The conformance test `contract_cid_is_signer_independent` is green. Bridges this kit mints reference contractCids, never attestationCids.

## Read next

- [`08-version-chains.md`](08-version-chains.md) (when written). Version chains pin contractCids in sorted-set form. The function this step adds is the input to the next step's pinning.
- [`06-bridge-IR.md`](06-bridge-IR.md). Bridge IR consumes contractCid in `target.cid`. Step 6 details the tagged-union target shape; this step details what goes inside it.
- [`docs/security/multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md) (when written). The contract axis is one of three orthogonal pinning axes. The other two (witness, binary) are body fields per the substrate-layers spec; the contract axis sits in the header.
