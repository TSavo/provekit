# Binary Attestation Protocol

**Status:** v1.4.0 normative draft
**Date:** 2026-05-02
**Catalog property:** Listed in the v1.4.0 catalog as `binary-attestation-protocol`; CID is computed from this file's bytes per `2026-04-30-protocol-catalog-format.md` §2.1 (raw-byte BLAKE3-512). No CID line appears in this header by design: a self-referencing CID would invalidate on every edit.
**Owner:** verifier crate + producers minting `.proof` bundles.
**Related:**
- `2026-04-30-proof-file-format.md` (the `.proof` envelope grammar this spec governs the lifecycle of)
- `2026-04-30-memento-envelope-grammar.md` (memento shape; discharges nest as members)
- `2026-05-02-multi-solver-protocol-v2.md` (consumer of discharges from `coverage_required` consensus)
- `2026-04-30-ir-formal-grammar.md` (BridgeDeclaration `targetProofCid`, the forward pin)

## §0. Letter-envelope framing

A binary attestation is a *letter* and an *envelope*.

- **LETTER** = the binary, immutable, content-addressed by `bcid = hash(binary_bytes)`.
- **ENVELOPE** = the `.proof` bundle, minted AFTER the binary is built, contains `binaryCid: bcid`, contains discharge mementos, signed externally.

The binary doesn't know its envelope. The envelope knows the binary's hash. One-way reference. No circularity.

This shape is the resolution of the chicken-and-egg problem: "how does a binary attest to its own hash without containing its own hash?" It doesn't. The envelope does. The same pattern appears in Sigstore/cosign (the bundle references the artifact digest), Apple notarization (the ticket attests to a signed binary's hash), Authenticode (the catalog signs hashes of files), TLS server certs (the cert binds a public key to a name, not the bytes of the server), and JWT (the signature attests to a payload it does not contain). Prior art is uniform: the attestation names the artifact; the artifact never names the attestation.

This spec is the protocol surface for that pattern in Sugar.

## §1. Why this matters

`2026-04-30-proof-file-format.md` integrity rule 5 ("Binary CID matches running artifact (when present)") was softened to MAY in the v1.3.0 catalog cut, with explicit rationale: no reference verifier had yet shipped the `hash(running_binary)` routine end-to-end, so producers could emit `binaryCid` without consumers being able to enforce it.

**v1.4.0 promotes the verifier-side hash check to MUST given a reference verifier ships alongside this spec.** Rule 5 in the proof-file-format spec remains the consumer-side check; this spec defines the producer-side lifecycle and the verifier procedure that closes the loop.

The supply-chain anchor is only an anchor when both ends are pinned. v1.3.0 had the field; v1.4.0 has the protocol.

## §2. Build-mint-sign pipeline

The producer-side lifecycle is five steps. Each step's output is the next step's input. No step references its own output; the references run only in the build direction.

1. **Build** binary `B`. `B` contains contract CIDs `[c1, c2, ...]` it CLAIMS to satisfy. The contract list is hash-stable, derived from the source IR; recompiling the same source produces byte-identical contract CIDs.
2. **Hash** `B` → `bcid = blake3-512(binary_bytes)`. The hash is computed once, after the build is final.
3. **Discharge.** Run the prover against (`B`'s source IR, `c1..cn`) → discharge mementos `[d1, d2, ...]`. Each `d_i` is a `coverage_required` consensus discharge (per `2026-05-02-multi-solver-protocol-v2.md`) or another memento kind admissible under `2026-04-30-memento-envelope-grammar.md`.
4. **Mint** a bundle:
   ```
   {
     "binaryCid": bcid,
     "discharges": [d1, d2, ...],
     "signer": <signer-pubkey-memento-cid>,
     "signature": <Ed25519 signature over canonical bytes with signature field omitted>
   }
   ```
   The bundle's other required fields (`formatVersion`, `members`, etc.) carry forward from `2026-04-30-proof-file-format.md` unchanged; this spec governs the `binaryCid`, `discharges`, `signer`, `signature` fields' lifecycle.
5. **Sign externally** with a foundation key, a producer key, or any third party's key (see §6 on permissionless attestation).
6. **Distribute** `(B, bundle.proof)` as a pair, OR detached with content-addressable lookup (§3).

**INVARIANT (build order):** `bcid` MUST be computed from the final binary bytes. A producer that hashes a pre-strip, pre-sign, or pre-compress artifact and ships a different artifact ships a mismatched bundle; verifiers MUST reject (§10).

**INVARIANT (no self-reference in the binary):** `B` MUST NOT contain `bcid` as a hardcoded constant. Self-reference is not a build technique; see §9.

## §3. Distribution: detached + content-addressable

A `.proof` bundle is a separate file at `<bcid>.proof` (the existing convention from `2026-04-30-proof-file-format.md`). The bundle's filename is its content-address; the file's bytes hash to a CID equal to the filename.

**Manifest hint is advisory.** A package manifest (`package.json.sugar.proofHash`, `Cargo.toml`'s equivalent, `go.mod`'s equivalent) MAY carry a hint pointing at the bundle. The hint accelerates discovery; it does NOT establish trust. The verifier validates by content (§4), not by manifest claim.

**Alternative considered: stapled bundles.** Stapling the bundle into the binary (Authenticode-style, OCSP-stapling-style) was considered and rejected. Two reasons:
1. It complicates binary builds: the binary must be re-linked after the bundle is minted, which changes `bcid`, which invalidates the bundle, requiring fixed-point iteration.
2. It conflicts with content-addressability: a stapled binary's hash includes the bundle's bytes, so the bundle cannot reference its own container without circularity.

Detached + content-addressable preserves the letter-envelope shape: `B`'s bytes are fixed; `bundle.proof` references `B`'s hash one-way.

## §4. Verifier procedure

Given binary `B` presented at runtime (a `.dylib`, `.so`, `.exe`, Wasm module, JS bundle, or other content-addressable artifact):

1. **Hash** `B` → `bcid_observed`.
2. **Look up** the bundle by `bcid_observed`. Sources, in priority order: a manifest hint (advisory), a content-addressed cache, a side-channel (e.g., a signature server, an attached `.proof` file at the binary's path).
3. **Verify** the bundle's signature against the bundle's `signer` pubkey memento. Per `2026-04-30-proof-file-format.md` §3 rule 3.
4. **Confirm** `bundle.binaryCid == bcid_observed`. The bundle's claim about the binary must match the binary actually presented.
5. **Treat** the bundle's `discharges` as authoritative for `B`, subject to the consumer's key-trust policy (§8 on multi-source attestation).

**INVARIANT (signature-alone is insufficient):** A bundle whose `binaryCid` does NOT match the running binary's hash MUST be rejected, EVEN IF the signature is valid and the signer is trusted. Signature attests to bundle integrity, not to binary identity. See §9 on this forbidden pattern.

**INVARIANT (verifier procedure is total order):** Steps 1-4 are gating. Step 5 is consumption. A bundle that fails any gate MUST NOT contribute to the verifier's verdict for `B`.

## §5. Two-pin closure

The binary-attestation protocol works alongside the bridge-side pin established by `targetProofCid` in `2026-04-30-ir-formal-grammar.md` §BridgeDeclaration. Together they close a two-pin loop:

- **Forward pin (call site → bundle):** A bridge declaration's `targetProofCid` pins the bundle that attests to the bridge's target. This is a CID-to-bundle reference written at IR-mint time.
- **Back pin (bundle → binary):** A bundle's `binaryCid` pins the binary the bundle attests to. This is a hash-to-binary reference written at mint-time per §2.

**The verifier checks both pins.** A call-site bridge points at a bundle (forward pin). The bundle's `binaryCid` matches the running binary's hash (back pin). Either pin alone leaves a hole:
- Forward pin alone: the bundle is identified, but a substituted binary at runtime goes undetected.
- Back pin alone: the running binary's bundle is found, but a substituted call-site bridge points at a *different* bundle, and the consumer doesn't notice.

Both together = closed. The call site names the bundle; the bundle names the binary; the verifier confirms both.

**INVARIANT (two-pin closure):** A consumer that performs only the forward pin OR only the back pin has NOT verified supply-chain integrity. The protocol REQUIRES both checks.

## §6. Monotonic envelope accretion (the cosmic property)

The binary is closed. The contract surface is open.

Once published at `bcid`, the binary never changes. New bundles can be minted indefinitely, by anyone, each attesting to the same `bcid` with new discharges.

**Worked example.** `parseInt-v1.0.0` at `bcid = blake3-512:abc...`:

- 2026-Q1: foundation publishes initial bundle attesting `[parseInt_correct]`. Signer = foundation key.
- 2026-Q3: a security researcher mints a new bundle. Same `bcid`. Discharges = `[memory_safe]`. Signer = researcher key. The binary is unchanged; the contract surface gained a claim.
- 2027-Q1: a regulator mints another bundle. Same `bcid`. Discharges = `[FDA_21CFR_Part_11_compliance]`. Signer = regulator key.
- 2030: someone discovers a new contract class (a property nobody had named in 2026), mints another bundle attesting `bcid` against that property.

Three architectural consequences:

1. **Permissionless attestation.** Third parties mint bundles for binaries they didn't author. The producer's role at build-time and the attester's role at mint-time are separable; the latter requires only the binary bytes and a key.
2. **Monotonic verification surface.** Pinning a `bcid` is forward-compatible: a 2026 pin gains 2030 contracts by fetching new bundles, no binary upgrade required. The set of contracts a binary is known to satisfy grows over time without altering the binary.
3. **Trust decisions stay at the key layer.** A binary's attestation set is a multi-source DAG of claims, each anchored to a specific signer. Consumers select bundles by which signers they trust; the protocol does not centralize trust.

**INVARIANT (monotonicity):** A bundle minted at time `t` attesting to `bcid` REMAINS valid at time `t' > t` as long as its signature verifies. Future bundles do NOT invalidate past bundles. The verification surface accretes; it does not replace.

## §7. Connection to witness minting

Witness minting is the operational realization of monotonic envelope accretion. Given a binary at `bcid` and a set of discharge mementos under a chosen signer's key, the verifier/emitter path produces a new bundle attesting to that `bcid`. This is not a public raw-IR `sugar witness` command.

This spec normatively documents what that witness-minting flow produces. The input contract:
- A binary file path (or `bcid` directly, if the consumer already has it).
- A set of discharges to include (per §2 step 3).
- A signer key (foundation, producer, third party, etc.).

Output: a `.proof` file at `<bcid>.proof` (or a name suffixed by signer if the consumer keeps multiple bundles per binary).

**INVARIANT (witness mint shape):** witness minting MUST NOT alter the binary. It MUST NOT mutate any existing bundle. Each invocation produces ONE new bundle; existing bundles remain bit-for-bit unchanged.

## §8. Re-signing semantics

Multiple bundles can attest to the same `binaryCid`. The verifier picks bundle(s) based on which discharges the consumer needs.

- Old bundles stay valid. New bundles add coverage.
- A consumer wanting to verify `parseInt_correct` fetches the foundation bundle. A consumer wanting `memory_safe` fetches the researcher bundle. A consumer wanting both fetches both.

**Conflict policy: protocol surfaces, consumers resolve.** If two bundles claim contradictory discharges (one says "B satisfies C," another says "B does NOT satisfy C"), the verifier surfaces the conflict in its diagnostic output. The protocol does NOT mandate the resolution. Consumers choose:
- Prefer the foundation key (centralized trust).
- Fail closed (any conflict invalidates the verdict).
- Prefer the most recent bundle by signature timestamp (recency policy).
- Other consumer-defined policies.

**INVARIANT (conflict surfacing):** A verifier presented with bundles claiming contradictory discharges for the same `bcid` MUST report the conflict to the consumer, with both bundles' CIDs and signers. The verdict is consumer-policy-determined.

## §9. Forbidden patterns

Three patterns are forbidden by this spec's design.

1. **Binary embedding `hash(binary)` as a hardcoded constant.** Self-reference is impossible to compute: changing the constant changes the hash, which changes the constant. A fixed-point iteration is required, and even if found, it pins the binary to one specific hash that any modification breaks. The letter-envelope pattern exists precisely to avoid this.

2. **Bundle without a `binaryCid` field claiming to attest to a binary.** A bundle's authority comes from naming what it attests to. A bundle that does not name its target attests to nothing; it is not interpretable as a binary attestation.

3. **Verifier accepting a bundle whose `binaryCid` does NOT match the running binary's hash, even if the bundle is signed by a trusted key.** Signature verifies bundle integrity; `binaryCid` match verifies binary identity. Both are required. A trusted signer mis-attesting a binary is still mis-attesting; the protocol catches this regardless of trust.

## §10. Conformance

| Producer state | Consumer obligation |
|---|---|
| Ships a binary WITHOUT an accompanying bundle | The binary is UNATTESTED. Consumers MAY choose to run it; they get no protocol-level integrity guarantees. |
| Ships a binary WITH a bundle whose signature is invalid | The bundle is CORRUPTED. Verifiers MUST reject. |
| Ships a binary at hash `X` with a bundle whose `binaryCid = Y`, where `Y ≠ X` | The bundle is MISMATCHED. Verifiers MUST reject (per §4 step 4 and §9 pattern 3). |
| Ships a binary at hash `X` with a bundle whose `binaryCid = X` and valid signature | The bundle is CONFORMANT. Verifiers proceed to consume `discharges` per consumer policy. |

**INVARIANT (no partial acceptance):** A verifier MUST NOT accept a bundle whose signature verifies but whose `binaryCid` mismatches, NOR a bundle whose `binaryCid` matches but whose signature is invalid. Both gates are required (§4).

## §11. Acceptance

This spec is satisfied by:

- A reference verifier implementation that performs steps 1-5 of §4 end-to-end on the host platforms Sugar targets.
- Witness minting produces bundles whose shape conforms to §2 and whose lifecycle conforms to §8.
- Integration tests cover:
  - The §4 happy path: a binary at `bcid`, a bundle at `<bcid>.proof`, a successful verdict.
  - The §9 pattern 3 negative path: a bundle whose `binaryCid` is altered post-signing; verifier rejects despite valid signature.
  - The §6 monotonic accretion: a binary with two bundles from different signers; verifier surfaces both attestation sets.
  - The §8 conflict path: two bundles with contradictory discharges; verifier surfaces the conflict to consumer-policy resolution.
  - The §5 two-pin closure: a bridge with `targetProofCid` pointing at one bundle, runtime binary matching another `bcid`; verifier rejects.

## §12. Open follow-ups

- **Bundle revocation.** §6 states that bundles are monotonically accrued and never invalidated by future bundles. A separate revocation mechanism (e.g., a signed revocation memento naming a bundle CID as withdrawn by its signer) is deferred. The protocol as specified has no revocation; consumers wanting revocation define it at the trust layer.
- **Signer rotation.** The bundle's `signer` field references a public-key memento. Key rotation is the public-key memento spec's concern, not this spec's.
- **Cross-platform `bcid` semantics.** A single source can produce multiple binaries (one per platform). Each binary has its own `bcid`; each requires its own bundle. A future spec may define a "binary group" memento that names a set of `bcid`s as variants of the same source. Out of scope for v1.4.0.

## §13. Related specs

- `2026-04-30-proof-file-format.md` — the bundle envelope's grammar; this spec governs the lifecycle.
- `2026-04-30-memento-envelope-grammar.md` — the memento shape; discharges are mementos.
- `2026-05-02-multi-solver-protocol-v2.md` — produces the discharges this spec packages.
- `2026-05-02-opacity-manifest-grammar.md` — manifests carried inside discharges via SolveResult envelopes.
- `2026-04-30-ir-formal-grammar.md` — `BridgeDeclaration.targetProofCid`, the forward pin in §5's two-pin closure.
- `2026-04-30-protocol-catalog-format.md` — the rule by which this spec's CID is computed.
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative for bundle bytes prior to signing.
