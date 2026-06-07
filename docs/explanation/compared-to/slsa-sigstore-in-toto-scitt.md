# Sugar compared to SLSA, Sigstore, in-toto, SCITT (supply-chain attestation)

The supply-chain attestation space has multiple frameworks with different goals. Sugar has a unique role in this space and complements the others.

## The cleanest one-line summary per framework

- **SLSA** (Supply-chain Levels for Software Artifacts): a tiered specification for build provenance. "What was built, by what, from what source." Levels 1-4 increasing in rigor.
- **Sigstore**: identity-rooted signing infrastructure. Cosign + Fulcio (OIDC certificates) + Rekor (transparency log).
- **in-toto**: a framework for capturing build steps as signed attestations. The "ITE" (in-toto Enhancement) family of specs.
- **SCITT**: Supply Chain Integrity, Transparency, and Trust (IETF). Standardizes attestation transparency logs.
- **CycloneDX / SPDX**: SBOM (Software Bill of Materials) formats. Inventory of components.
- **Sugar**: a protocol for content-addressing **behavioral verifications**. Not inventory, not provenance, not identity; what the code *does*.

Sugar is in a different category from the others. It complements all of them.

## What each framework attests

| Framework | Attests |
|---|---|
| SLSA | "This build was produced by [builder] from [source] at [timestamp]." |
| Sigstore | "This artifact was signed by [identity], rooted in [OIDC issuer]." |
| in-toto | "This pipeline ran these steps in this order with these inputs and outputs." |
| SCITT | "This attestation is in the transparency log; here's a Merkle proof." |
| CycloneDX/SPDX | "This artifact contains these components at these versions." |
| **Sugar** | "This function satisfies these behavioral contracts." |

The first five address build-process and identity questions. Sugar addresses behavioral correctness questions. The questions are orthogonal.

## Worked example: a complete supply-chain posture

Imagine you ship a Rust crate. A complete posture combines all of these:

1. **Source review.** Maintainers and reviewers approve PRs. Out of scope for any of these frameworks.
2. **CycloneDX SBOM.** The crate's compile-time and runtime dependencies are enumerated.
3. **SLSA Level 3 build provenance.** The build is reproducible, runs in a hardened environment, produces a signed provenance attestation.
4. **Sigstore signing.** The build artifact and its provenance are signed with a Sigstore identity rooted in the maintainer's OIDC issuer.
5. **in-toto attestation chain.** Each pipeline step (build, test, sign, publish) is a signed in-toto attestation.
6. **SCITT transparency log entry.** The Sigstore signature is recorded in a transparency log; consumers can verify.
7. **Sugar `.proof`.** Behavioral contracts on the crate's exported functions are signed, content-addressed, with a rank-3 pin (`contractCid`, `witnessCid`, `binaryCid`) per [`multi-dimensional-pinning.md`](../../security/multi-dimensional-pinning.md).

Each layer answers a different question:

- "What's in the build?" → SBOM.
- "Where was it built?" → SLSA / in-toto.
- "Who built and signed it?" → Sigstore.
- "Has the signature been tampered with after the fact?" → SCITT.
- "Does the code do what's claimed?" → Sugar.

A consumer downloading the crate runs all the verifications:

- Verify SBOM matches actual contents.
- Verify SLSA provenance matches expected build environment.
- Verify Sigstore signature against trusted identity.
- Verify SCITT transparency log entry.
- Verify Sugar `.proof`: `binaryCid` matches the running artifact, `contractCid` resolves to the pinned contract, `witnessCid` chains to a trusted prover, all signatures verify, the handshake discharges the consumer's call sites.

If all five layers pass, the consumer has high confidence in identity, provenance, integrity, inventory, and behavior. Each layer alone is partial; the combination is strong.

## Where Sugar and SLSA overlap (and don't)

SLSA Level 4 includes "two-party review" and "hermetic builds." It does NOT include "verifies the artifact does what it claims to do." That's the Sugar slice.

A Level 4 SLSA artifact could still implement parseInt with a backdoor that exfiltrates data. SLSA verifies *how* it was built; it doesn't verify *what* it does.

Sugar on top of SLSA: SLSA gives you confidence in the build pipeline; Sugar gives you confidence in the artifact's behavior. Both layered together cover both axes.

## Where Sugar and Sigstore overlap

Sigstore signs artifacts. Sugar signs mementos. Different artifacts:

- Sigstore: signs the binary, the SBOM, the SLSA provenance, etc.
- Sugar: signs each contract memento, each implication, each bridge, each `.proof` bundle.

A `.proof` bundle can be signed with a Sigstore-rooted key. The bundle's signature is Ed25519 (per the protocol); the public key in the bundle's `publicKey` field can be a Sigstore-issued certificate's public key. The consumer verifies both:

- Sugar: the bundle's signature is valid against `publicKey`.
- Sigstore: `publicKey` is rooted in a trusted OIDC issuer.

This combination buys identity-rooted Sugar signing without Sugar needing to define its own identity infrastructure.

## Where Sugar and in-toto overlap

in-toto attests to pipeline steps. Sugar attests to behavioral claims. Different scopes:

- in-toto: "step `build` ran with these inputs and produced these outputs."
- Sugar: "the artifact's `parseInt` function returns positive integers for short input strings."

A complete pipeline:

```
in-toto step "fetch": fetched source from git@... at hash X.
in-toto step "build": ran cargo build in env Y, produced artifact Z.
in-toto step "test": ran cargo test, all pass.
in-toto step "verify": ran provekit prove, discharge fraction 0.91.
in-toto step "sign": signed with key K.
in-toto step "publish": published to crates.io.
```

The "verify" step's output is the Sugar `.proof`. in-toto attests that the verification step ran; Sugar attests to what the verification claimed.

Cleanly orthogonal.

## Where Sugar and SCITT overlap

SCITT standardizes how attestations land in a transparency log. Sugar is one source of attestations; SCITT is the transparency story.

A Sugar `.proof` bundle can land in SCITT. The bundle's signature is the attestation; SCITT's Merkle proof confirms the bundle was published at a specific time and hasn't been retroactively modified.

SCITT integration is forwards-looking; not yet shipping in v1.x. The protocol is compatible: Sugar's signed mementos are first-class SCITT attestations.

## Where Sugar and SBOMs overlap (very little)

SBOMs are inventory: "this artifact contains `lodash@4.17.21` and `axios@1.6.0`." Sugar is behavior: "the function `lodash.parseInt` satisfies contract X."

An SBOM with no behavioral claims is silent on whether the listed components are correct. A `.proof` with no inventory is silent on what's actually shipping.

Combine them: SBOM tells the consumer what's there; `.proof` tells the consumer how it behaves; both layered are stronger than either alone.

## What Sugar uniquely provides

The slice no other framework provides:

- **Content-addressed behavioral contracts**: a memento's CID identifies its claimed behavior.
- **Federated discharge**: a Z3 (or other backend) discharge of `(post, pre)` is content-addressed evidence; once minted, it's reusable across the dependency graph.
- **Cross-language transfer**: bridges to reference contracts let a Rust consumer benefit from a TypeScript implementation's verification (and vice versa).
- **Cache amortization**: most call sites discharge at Tier 1 (hash equality) once the lattice is warm.

None of SLSA / Sigstore / in-toto / SCITT / SBOM provide these.

## When you don't need Sugar

If your supply-chain posture is satisfied by:

- SBOM for inventory.
- SLSA for provenance.
- Sigstore for identity.
- in-toto for pipeline.
- SCITT for transparency.

...then Sugar is optional. It adds behavioral verification on top, which not every consumer needs.

For a project shipping straightforward CRUD code with no behavioral correctness requirements beyond "tests pass," the existing supply-chain stack is sufficient.

## When you do need Sugar

When your consumers care that:

- Your `parseInt` actually parses integers.
- Your validation actually validates.
- Your encryption actually encrypts.
- Your input sanitization actually sanitizes.

These are behavioral claims. The existing supply-chain frameworks don't address them. Sugar does.

## Ecosystem positioning

Sugar's relationship to the supply-chain space:

- **Not competitive with SLSA / Sigstore / in-toto / SCITT.** They cover identity, provenance, transparency. Sugar covers behavior.
- **Strongly complementary.** Sugar's signatures should ride on Sigstore identities. Sugar's discharge should be in-toto-attested. Sugar's bundles should land in SCITT.
- **Independently valuable.** Even without the others, Sugar's rank-3 pin (`contractCid`, `witnessCid`, `binaryCid`) and content-addressed contracts add a layer not present in any other framework.

For organizations with mature supply-chain practices: add Sugar as the behavioral layer.
For organizations bootstrapping supply-chain practices: Sugar is one of several first-tier components; pick what's load-bearing for your threat model.

## Read next

- [`../../security/supply-chain.md`](../../security/supply-chain.md): supply-chain attack scenarios in depth.
- [`../../security/what-binaryCid-catches.md`](../../security/what-binaryCid-catches.md): binary integrity defense.
- [coq-fstar-lean.md](coq-fstar-lean.md): interactive theorem provers (different category).
- [sbom-formats.md](sbom-formats.md) (when written): CycloneDX / SPDX in depth.
- [`../boundaries.md`](../boundaries.md): what Sugar is NOT.
