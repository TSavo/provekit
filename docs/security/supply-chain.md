# Supply-chain attacks

ProvekIt's `.proof` bundle is the supply-chain artifact. Under v1.4, the supply-chain anchor is the **rank-3 consumer pin** `(contractCid, witnessCid, binaryCid)`, not a single CID. Each axis catches a distinct attack class; the combination closes the supply-chain perimeter at a level single-axis pinning cannot.

This doc walks through the supply-chain attack scenarios and what the protocol does (and doesn't) defend against.

> See [`multi-dimensional-pinning.md`](multi-dimensional-pinning.md) for the architectural foundation and [`../papers/03-substrate-not-blockchain.md`](../papers/03-substrate-not-blockchain.md) §11–§12 for the manifesto-level framing of address-as-vector-space and rank-N pinning.

## The attack surface

A modern dependency tree is deep. A Rust binary pulls in thousands of crates. An npm app pulls in tens of thousands of packages. A Java service pulls in hundreds of JARs. At each level of the tree, there is a code-trust boundary the consumer typically does not audit.

The supply-chain attack class exploits this. An attacker's goal is to inject malicious code into one of the dependencies, somewhere in the tree, such that the consumer's program runs the malicious code while the consumer remains unaware.

Recent real-world examples (illustrative, not exhaustive):

- **event-stream / flatmap-stream (npm, 2018)**: maintainer transferred ownership; new owner added a malicious dependency that exfiltrated cryptocurrency wallets.
- **codecov bash uploader (2021)**: attacker compromised a CI tool's distribution; replaced the uploader with one that exfiltrated environment variables.
- **xz-utils (2024)**: long-running social-engineering attack to inject a backdoor into liblzma; would have shipped in major Linux distros.
- **various PyPI typosquatting**: attacker registers `requestz` / `selenuim` / `urllib4`; victim mistypes; malicious package ships.

Different attack mechanisms, similar shape: malicious code in the dependency tree.

## What ProvekIt provides at the supply-chain layer

### A signed `.proof` per dependency

Each dependency ships with a `.proof` bundle. The bundle is signed with the maintainer's key. A consumer can verify:

- Bundle integrity (CID matches filename).
- Bundle signature (against trusted maintainer keys).
- Member integrity (each contract memento's CID and signature).

If the bundle is tampered with in transit or in the registry, integrity checks fail. If the bundle is signed by an untrusted key, signature verification fails.

### `binaryCid` pinning

When set, the `.proof`'s `binaryCid` field is the BLAKE3-512 of the compiled binary. A consumer's verifier compares the running binary's hash against `binaryCid`. If the binary has been substituted (say, by a compromised CI), the hash mismatch is detected.

See [what-binaryCid-catches.md](what-binaryCid-catches.md).

### Cross-language proof transfer

A dependency in language A ships a `.proof` with bridges to reference contracts. A consumer in language B verifies the bridges. Cross-language behavioral checks happen at Tier 1 of the handshake — content-addressed, no solver invocation.

This means an attacker cannot satisfy the consumer's contract on the B side while violating it on the A side: the bridge anchors the behavior to a shared canonical contract via its content-only `contractCid` (per `2026-05-03-contract-cid-vs-attestation-cid.md` R3, bridges reference contractCids, not attestationCids), so multiple kits and signers converge on the same contract identity.

### Signed evidence

When Tier 3 fires, the implication memento is signed by the prover. A user investigating "how was this discharged?" can audit the evidence offline.

## What ProvekIt does NOT provide at the supply-chain layer

### Maintainer identity verification

The protocol does not verify that "alice@example.com" is actually Alice. It just verifies that the signature was made by someone holding the key associated with that identity.

Mitigation: Sigstore + Fulcio (OIDC-rooted certificates) provide identity verification. ProvekIt and Sigstore are complementary; a `.proof` can be signed by a Sigstore-rooted key.

### Account compromise of the maintainer

If the attacker compromises the maintainer's account on the registry (npm, crates.io, etc.), they may be able to push a new `.proof` bundle signed by the maintainer's key (if the maintainer's signing key is also accessible from the compromised account).

Mitigation: hardware-key signing, separation between registry credentials and signing credentials, quorum signing.

### Malicious code in the source

If the attacker is the maintainer, or if their code review is bypassed, they can ship malicious code that satisfies its own contracts while doing harm. The contract attests the maintainer's claim; the claim might be wrong or adversarial.

Mitigation: code review, multi-maintainer review, trust-not-binary policy ("trust only the binaries built by our CI from auditable source").

### Subverted CI

If the attacker compromises the maintainer's CI pipeline, the `.proof` can be signed legitimately (the CI has access to the signing key) but reflect malicious source.

Mitigation: hardware-key signing where the human signer is separate from CI, in-toto attestations of build provenance, separation between "who reviewed the code" and "who signed the artifact."

### Network-level interception

If the attacker controls the network between the registry and the consumer, they may attempt to inject a tampered `.proof` or a tampered binary.

Mitigation: TLS for distribution. ProvekIt's content-addressing also defends: the consumer fetches by CID; if the bytes don't hash to the expected CID, the fetch fails. Network injection cannot produce content with a colliding CID without breaking BLAKE3-512.

### DNS poisoning

Similar shape to network interception. The consumer's name resolution is redirected; the consumer fetches `.proof` from an attacker-controlled host. As above, content-addressing defends: bytes that don't hash to the expected CID are rejected.

### Compromised package registry

If the registry itself is compromised (registry operators or registry infrastructure), it can serve tampered `.proof` files. Content-addressing detects tampering. CIDs are publicized in the consumer's lockfile or pinning configuration; a registry that serves bytes hashing to a different CID is caught.

## Layered defenses

ProvekIt is one layer. A robust supply-chain posture combines:

| Defense | Role |
|---|---|
| TLS in transit | Confidentiality + integrity at the network layer |
| Sigstore / Fulcio | Identity-rooted signing (OIDC certificates) |
| in-toto / SLSA | Build provenance (where the binary was built, by what, from what source) |
| ProvekIt | Behavioral contracts + binary CID pinning |
| Reproducible builds | Verifiable that source compiles to the binary |
| Multi-maintainer review | Human-layer integrity |
| SBOM (CycloneDX / SPDX) | Inventory of what's in the build |

Each addresses a different failure mode. ProvekIt specifically covers behavioral verification + binary integrity. Sigstore + in-toto + reproducible builds covers identity + build provenance. SBOM covers inventory.

The combinations: Sigstore-signed `.proof` files, with in-toto attestations for the build, reproducible binaries pinned via `binaryCid`, behavioral contracts verified at Tier 1.

## Practical posture

For most projects:

1. **Sign `.proof` files** with hardware-rooted keys.
2. **Set `binaryCid`** for compiled artifacts.
3. **Pin signing keys** in your verifier configuration.
4. **Update keys' trust** when maintainers rotate.
5. **Combine with Sigstore** for OIDC-rooted identity verification.

For high-stakes projects:

6. **Require multi-signature** on critical dependencies.
7. **Require in-toto attestations** for build provenance.
8. **Require reproducible builds** so the binary CID is independently checkable.
9. **Run independent verification** on a periodic schedule.
10. **Monitor for key compromise**; rotate aggressively.

For ecosystem-level participants:

11. **Publish `.proof` files** alongside packages. Even partial coverage is better than nothing.
12. **Contribute to public implication servers** so the lattice grows.
13. **Curate reference contracts** for your domain.
14. **Document threat models** so consumers know what your `.proof` does and doesn't claim.

## What ProvekIt's role becomes at scale

As the ecosystem matures, ProvekIt's role in supply-chain security is:

- **At fetch time**: content-addressing detects tampering in transit and at the registry.
- **At verify time**: contracts and bridges detect behavioral substitution.
- **At build time**: `binaryCid` detects binary substitution.
- **At runtime** (kit-dependent): kit guards detect monkey-patching.

The protocol's structure is "every step is content-addressed and signed." This makes the supply chain auditable end to end, mathematically rather than by human inspection.

## Read next

- [what-binaryCid-catches.md](what-binaryCid-catches.md) — detailed walkthrough of binary substitution scenarios.
- [what-binaryCid-does-not-catch.md](what-binaryCid-does-not-catch.md) — limits of `binaryCid`.
- [signature-and-non-repudiation.md](signature-and-non-repudiation.md) — signing scheme details.
- [`../explanation/compared-to/slsa-sigstore-in-toto-scitt.md`](../explanation/compared-to/slsa-sigstore-in-toto-scitt.md) (when written) — how ProvekIt complements other supply-chain tools.
- [reporting-vulnerabilities.md](reporting-vulnerabilities.md) — operational security practices.
