# After Single-Party Transport: Witness Pluralism and Jurisdiction-Neutral Transport

> The marketplace property, the security model, and the standardization restructure that fall out of v1.4's substrate-vs-metadata cut.

## Abstract

The v1.4 substrate is content-agnostic. Static analyzer attestations, formal verifier outputs, test runs, build provenance, audit signoffs, reproducibility claims, regulator policies, and adversarial witnesses are all the same envelope/header/body shape. Different signers, different body conventions, same wire format. Six operational consequences follow:

1. **Witness pluralism in kind.** Every category of claim about a binary rides on one substrate.
2. **Witness pluralism in valence.** Negative attestations ("binary X fails contract Y") are first-class. Adversarial witnesses share the infrastructure with positive witnesses; consumer policy decides what to do with each. CVE, NVD, GHSA become memento streams. Yank-by-counter-witness is a primitive operation, not a registry feature.
3. **Linear scaling.** New witnesses, kits, consumers compose at O(N+M+K), not O(N×M×K). This is the marketplace property, made literal.
4. **Existing frameworks reduce to body conventions.** SLSA, Sigstore, in-toto, SCITT, CycloneDX, OSCAL: every supply-chain framework is expressible as body schema plus tooling under the substrate.
5. **Transport is fungible.** Content-addressing makes distribution mechanism irrelevant. Mementos flow through IPFS, BitTorrent, HTTPS, git, S3, USB sticks, Bitcoin OP_RETURN. There can be no ProvekIt server because there can't be one.
6. **Spec authorship is not spec ownership.** The architect authored the substrate; the marketplace forms downstream. The reference implementation bootstraps adoption without controlling it. This is the cURL/HTTP relationship made explicit.

These consequences restructure the standardization story. Regulators publish content-addressed policy mementos naming their (contract spec + trusted signer set) requirements. Consumers pin to policy CIDs. The per-regulator standardization ask collapses from "amend this regime's text" to "publish a memento under your authority key", measured in 1-2-year horizons rather than 10-15-year ones for the regulator-acceptance phase, while the substrate-level engineering work (TCB minimization, tool qualification, conformance harness) continues on the longer track.

This paper makes the claims precise, develops the security and governance implications, and proposes the standardization restructure that follows.

---

## §1. The shape

The v1.4 substrate-layers spec (`2026-05-03-substrate-layers-envelope-header-body.md`) names the three layers of every signed memento:

```
+-----------------+
|    ENVELOPE     |  signer, declaredAt, signature
+-----------------+
|     HEADER      |  data: what the substrate verifies
|     (data)      |  (kind, content cid, required references)
+-----------------+
|      BODY       |  metadata: what tooling interprets
|   (metadata)    |  (everything else)
+-----------------+
```

The substrate verifies envelope (signature against signer over canonical bytes) and header (kind-specific invariants on content references). Body is opaque to the substrate. The signature transitively covers all three layers via JCS of `(header, body)`, so body content inherits cryptographic provenance without being substrate-load-bearing.

The three substrate primitives are sign, hash, reference. The four-invariant verifier operates on envelope plus header. Everything else, including every external claim a tool wants to make about a binary, is composition over those three primitives plus the layered shape.

This paper develops what falls out of "everything else." The cut is mechanical and exhaustive: any new field a contributor wants to add is either header (substrate spec change required, with verifier validation rule) or body (no spec change, ecosystems iterate). The substrate stays small by construction. The composition layer is unbounded.

---

## §2. Witness pluralism in kind

Every category of external claim about a binary rides on the substrate as a body memento. The signer differs, the body schema differs, the wire format does not. Twelve categories suffice to make the claim concrete; the list is illustrative, not exhaustive.

**Static analyzer attestations.** Semgrep, Coverity, Infer, MIRAI, CodeQL, Clang Static Analyzer, SpotBugs, FindSecBugs, Brakeman. The body carries `analyzerName`, `analyzerVersion`, `astWalkCid` (a content-addressed reference to the AST traversal proof artifact), `claim`, and an `evidenceTerm` capturing the analyzer's structured output. The signer is the analyzer team's authority key.

**Formal verifier attestations.** Z3, CVC5, Yices, Bitwuzla, Lean, Coq, F\*, Isabelle, Verus, Kani, Prusti, Creusot, KLEE, CBMC, JBMC. The body carries the proof certificate (Z3 unsat core, Coq proof term, Lean tactic script, etc.) plus the formula being discharged. The signer is the prover's foundation key or a deployed instance's key.

**Test-run attestations.** "I ran the suite, here are the results, here is the discharge fraction." The body carries `testHarnessCid`, `testCorpusCid`, results breakdown, statistical evidence, environment fingerprint. The signer is the CI system or the test runner's authority key.

**Build provenance attestations.** SLSA-style. "This binary was built from these sources by this compiler in this environment." The body carries `sourceCommitCid`, `compilerCid`, `buildEnvironmentCid`, hermeticity claims, reproducibility witnesses. The signer is the build system's authority key.

**Audit attestations.** "This security firm reviewed these contracts and signs off." The body carries `auditScope`, `auditMethodologyCid`, findings, sign-off statement, auditor identity. The signer is the audit firm's key.

**Reproducibility claims.** "I rebuilt the binary deterministically from the same inputs and got bit-identical output." The body carries `inputCids`, `outputCid`, environment fingerprint, the rebuild proof. The signer is the rebuilder's key.

**Performance attestations.** "p99 latency for input class X is bounded by N microseconds on hardware H under workload W." The body carries `workloadCid`, hardware identifier, statistical evidence, methodology. The signer is the benchmarker's key.

**Regulatory compliance attestations.** "FDA 21 CFR Part 11 compliance demonstrated." The body carries `controlSetCid`, evidence-of-compliance per control, regulator-recognized methodology. The signer is the compliance authority's key, often a regulator-recognized auditor.

**License attestations.** Notarized statements about license terms, SPDX identifiers, license-compatibility findings. The body carries `licenseDocCid`, the asserted license terms, compatibility statements with named upstream dependencies.

**Provenance and chain-of-custody attestations.** "This binary was reviewed by these N reviewers in this order, signed by each." The body carries an ordered chain of reviewer signatures, each contributing one link.

**Sensor data attestations.** A signed memento bounding a measurement: "this temperature reading falls within calibration." The body carries calibration provenance, raw measurement data, instrument firmware CID, environmental conditions. The signer is the instrument's signing key.

**Identity attestations.** "Bearer of this key holds this identity claim, attested by this issuer." The body carries identity attributes, expiration, issuer authority chain. The signer is the identity provider's key.

Twelve categories. All same envelope shape, all same signing flow, all same content-addressed reference mechanism. A new category is "design a body schema, ship a tool that emits memos with that schema, sign with your key." No coordination with kits, with consumers, with the substrate spec. The substrate does not learn what an AST is, what a temperature calibration is, what an audit methodology is. The substrate transports bytes; tooling interprets them.

The §10 closure-by-composition manifesto claim is the structural justification: every body convention is a derived view over the existing substrate primitives. Adding a body convention is composition, not extension. The substrate's surface area is fixed; the composition layer carries the world.

---

## §3. Witness pluralism in valence

Witness pluralism is more than a list of claim kinds. The substrate is also indifferent to the *direction* of a claim. Positive witnesses say "binary X satisfies contract Y." Negative witnesses say "binary X fails contract Y." Same envelope, same signing flow, opposite epistemic content.

This section makes adversarial witnesses first-class. The security implications are substantial.

### §3.1 The shape of a negative attestation

A counter-witness has the same envelope/header/body layering. The body discriminates:

```json
{
  "envelope": { "signer": "ed25519:<security-researcher-key>", ... },
  "header":   { "schemaVersion": "1", "kind": "adversarial-witness", "cid": "..." },
  "metadata": {
    "claim":          "post → pre fails",
    "contractCid":    "blake3-512:<the contract being challenged>",
    "binaryCid":      "blake3-512:<the binary that fails it>",
    "counterexample": { /* concrete input + observed output */ },
    "evidenceTerm":   { /* solver-produced witness of failure */ },
    "advisoryRefs":   ["CVE-2026-XXXX", "GHSA-xxxx-yyyy-zzzz"]
  }
}
```

A defender attesting `post → pre` and an attacker (or auditor, or researcher) attesting `not (post → pre)` produce mementos that differ only in body content and signer key. The substrate transports both. Verifiers walk both. The trust set decides.

### §3.2 Yank-by-counter-witness

Pre-substrate, "yanking" is a registry capability: the registry retroactively withdraws a published version. Yanks are honor-system in most ecosystems; they require trusting the registry operator to act.

Under the substrate, yank is a primitive operation, not a feature. A maintainer or auditor publishes a counter-witness:

```json
{
  "metadata": {
    "yanksContractSetCid": "blake3-512:<the withdrawn set>",
    "yankReason":          "security:CVE-2026-XXXX",
    "yankSeverity":        "critical",
    "supersededByCid":     "blake3-512:<the new contract set>"
  }
}
```

This is a memento under the maintainer's (or auditor's, or anyone's) signing key. The substrate transports it. Consumers walk the DAG and apply yanks per their policy. Different consumers honor yanks differently:

- **Strict policy:** any yank by any signer in the consumer's trust set excludes the yanked set from resolution.
- **Audit policy:** only yanks by specific trusted security signers (e.g., a CSIRT key, a regulator's vulnerability-coordination key) are honored.
- **Permissive policy:** yanks are informational; the consumer continues to install the yanked version with a warning.

The substrate does not pick a yank semantics. It carries the signed yank claim and lets tooling decide. Different consumers can disagree on what counts as yanked; the substrate is neutral.

### §3.3 CVE / NVD / GHSA as memento streams

The vulnerability-disclosure ecosystem (MITRE's CVE program, NIST's NVD, GitHub's GHSA, vendor-specific advisories) currently operates as parallel registries with their own formats, identifiers, and trust models. Under the substrate, each authority publishes adversarial-witness mementos under its own signing key.

CVE-2026-XXXX becomes:

```json
{
  "envelope": { "signer": "ed25519:<MITRE-CVE-authority-key>", ... },
  "header":   { "kind": "vulnerability-attestation", "cid": "..." },
  "metadata": {
    "cveId":         "CVE-2026-XXXX",
    "affectedCids":  ["blake3-512:<binary or contract or contractSet>"],
    "severity":      "critical",
    "cvssScore":     9.8,
    "fixedInCids":   ["blake3-512:<the patched contractSet>"],
    "advisoryDocCid":"blake3-512:<the full advisory text>",
    "yanksContractSetCid": "blake3-512:<the affected set>"
  }
}
```

Consumers configure their trust set: trust MITRE for CVEs, trust GitHub for GHSAs, trust their own internal CSIRT, trust specific vendors for vendor advisories. The substrate transports all of them under one wire format. Cross-referencing is content-addressed: a GHSA pointing at a CVE references the CVE's memento CID; a vendor advisory pointing at both references both CIDs. The graph of vulnerability claims is content-addressed and signed at every edge.

This is what existing supply-chain attestation frameworks (CSAF, OpenVEX) approximate but cannot achieve in the substrate's natural shape. Both layer signing and content-addressing on top of pre-existing inventory schemes; the substrate handles both natively.

### §3.4 Defenders and attackers share infrastructure

This is the security-theoretic consequence. The substrate has no anti-adversarial layer. An attacker can publish whatever signed memento they wish, including fraudulent positive witnesses for malicious binaries, fraudulent counter-witnesses against legitimate binaries, fake advisories, fake compliance attestations.

The substrate does not stop them. Two reasons it doesn't need to:

**First, signing is the gate.** A fraudulent memento is signed by some key. If that key is not in the consumer's trust set, the memento is unconsumed. Adversaries can publish; consumers ignore. The cost of publishing fraudulent claims is bounded by what the adversary spends to acquire keys consumers trust.

**Second, the asymmetry is structural.** Defenders publish under their authority keys; attackers must compromise those keys (or convince consumers to trust their own keys) to influence consumer policy. The trust set is local; the adversarial bar is high.

The result is a security model where infrastructure is shared but trust is local. CVE authority publishes under MITRE's key; an attacker publishing fake CVEs under a stolen MITRE key is doing key-compromise, not supply-chain attack. An attacker publishing under their own key is publishing into the void; no consumer honors it. The substrate's neutrality is the security model.

This is structurally identical to TLS certificate authorities. The CA system is shared infrastructure; CAs publish certificates; clients pick which CAs to trust via root stores. An attacker can issue a certificate under their own CA, but no client trusts it. Compromise of a trusted CA is the attack class; the protocol's neutrality enables both honest and adversarial issuance, with trust gating consumption.

### §3.5 Implications for consumer policy

Consumer policy combines positive and negative witnesses:

```yaml
[policy.production]
require:
  - kind: positive-witness
    contractCid: "blake3-512:<required contract set>"
    minWitnesses: 2
    fromSigners: ["ed25519:<auditor-A>", "ed25519:<auditor-B>", "ed25519:<analyzer-foundation>"]
exclude:
  - kind: adversarial-witness
    fromSigners: ["ed25519:<MITRE-CVE>", "ed25519:<our-CSIRT>", "ed25519:<vendor-PSIRT>"]
    severity: ["high", "critical"]
```

A binary is acceptable iff it has at least two positive witnesses from approved signers AND no adversarial witness from the named authorities at high or critical severity. Both conditions are checked against the same memento DAG; both use the same wire format; both compose under the consumer's trust calculus.

Pre-substrate, "vulnerability scanning" and "verification" are separate tools with separate data formats and separate operational integrations. Under the substrate they are the same operation: walk the DAG, apply the policy, accept or reject. The unification is a consequence of the substrate's content-agnosticism.

---

## §4. Linear scaling

The witness-pluralism claim is structural. The §10 closure-by-composition claim makes it operational. Together they yield a scaling property that distinguishes the substrate from coordinating systems. Sections 4.1 through 4.3 motivate. §4.4 states the formal claim as the Substrate Independence Theorem with three constituent lemmas. §4.5 enumerates seven corollaries that follow mechanically from the theorem.

### §4.1 The math

Consider an ecosystem with N witness producers (analyzers, verifiers, auditors), M kits (per-host-language ProvekIt implementations), and K consumers (each with their own trust policy).

Without composition closure, each new participant must coordinate with every existing participant on the other axes. A new analyzer must be tested against every kit (does this analyzer's output round-trip through this kit's lifter?), and every consumer must be configured to recognize this analyzer's signing key. Adding an analyzer is M×K coordination events. Adding a kit is N×K. Adding a consumer is N×M. Total ecosystem coordination cost: O(N×M×K).

With §10 closure, the three axes are independent. An analyzer ships its tool with no kit-knowledge: it emits signed mementos in the standard envelope, and any kit's lifter can extract claims from any source language without analyzer-specific code. A kit lifts its source-to-IR independently of which analyzers will witness the lifted contracts. A consumer picks their policy without coordinating with analyzer or kit teams. Each axis scales O(1) per new participant. Total ecosystem cost: O(N+M+K).

### §4.2 The marketplace property

The N+M+K vs. N×M×K distinction is the marketplace property, made literal. The npm registry has approximately 2.5 million packages because adding a package is O(1): the package author publishes; consumers pull; no per-package consumer-side coordination. A coordinating registry with the same number of participants would have O(N×M×K) coordination cost; the system would not scale, and ecosystems empirically don't reach those scales unless they shed coordination.

The witness layer scales the same way. Adding semgrep does not require coordination with Coverity, with the Rust kit, with the Python kit, or with any specific consumer. semgrep ships its tool; consumers who trust semgrep's signing key add it to their policy; nothing else needs to change. The same is true for adding a new kit (e.g., the planned C++ lift adapter for `[[expects:]]` and `[[ensures:]]`) and for adding a new consumer (a new project, a new team, a new regulated environment).

Without this property, the substrate would be just another supply-chain attestation framework, requiring per-tool integration work to onboard. With it, the substrate is the foundation an ecosystem of arbitrarily many analyzers, verifiers, auditors, kits, and consumers can compose without per-pair coordination.

### §4.3 The architectural humility behind the leverage

The N+M+K scaling is not a feature added to the substrate; it is what falls out when the substrate refuses to coordinate. Each refusal (to validate body fields, to centralize signer trust, to mandate witness types, to bind to a distribution mechanism) is what enables the next composition step. Architectural humility is the leverage. The substrate is small precisely so that the composition layer can be unbounded.

A counterfactual substrate with more centralization (a registry of approved analyzers, a mandated witness-type taxonomy, a trust-anchor program) would have lower ecosystem-bootstrap friction at the cost of marketplace scaling. It would look more like a traditional standards body: high coordination cost per participant, low total participant count, slow evolution. The witness layer would not host arbitrarily many analyzers because each would need to be approved.

The substrate's design choice is the opposite. The architect specifies; nobody owns. The composition layer takes care of itself.

### §4.4 The Substrate Independence Theorem

The motivation in sections 4.1 through 4.3 is now stated as a formal claim. Three independence lemmas establish that producers, consumers, and policy authors each operate without cross-axis coordination. The theorem is their composition.

**Lemma 4.4.1 (Producer Independence).** Let two witness producers P_i and P_j each emit signed mementos under the canonical envelope using their respective signing keys. Verification of either P_i's or P_j's mementos requires only the substrate spec at its content-addressed CID and the relevant signer's public key. P_i has zero shared state with P_j; the coordination cost between any pair of producers is exactly zero.

*Proof.* By construction of the envelope. The envelope's signature commits to the JCS-canonical bytes under the signer's key alone; verification reads the signature, the signer's public key, and the bytes. It does not consult any other producer's state, output, or identity. ∎

**Lemma 4.4.2 (Consumer Independence).** Let two consumers C_k and C_ℓ each pin their dependencies as content-addressed tuples (e.g., the rank-3 pin `(contractCid, witnessCid, binaryCid)`). Pin resolution for C_k requires fetching the referenced mementos by CID, verifying them, and applying C_k's local trust policy. The resolution is independent of C_ℓ's pins, of any registry, and of any centralizing oracle. Adding consumer C_{M+1} costs O(1) at onboarding and is invisible to all other consumers.

*Proof.* By content-addressing. A CID is verifiable by recomputation; a memento's signature is verifiable against the public key referenced in or carried by its envelope. C_k's verification path consults only the substrate spec, the relevant memento bytes, and the relevant public keys. No global mutable state is consulted; no other consumer's pins are observed. ∎

**Lemma 4.4.3 (Policy Independence).** Let two policy authors K_a and K_b each publish policy mementos under their authority keys. Existing producers do not need to know that policy K_a or K_b exists; consumers adopt by pinning the relevant policy CID; K_a's policy and K_b's policy do not interact unless a consumer chooses to honor both. Adding policy K_{L+1} costs O(1) at the policy author level and zero everywhere else.

*Proof.* A policy is a body memento under the substrate. Lemma 4.4.1 (producer independence) applies to its publication: the policy author signs and publishes; nobody coordinates. Lemma 4.4.2 (consumer independence) applies to its adoption: consumers pin its CID. The policy's existence is invisible to every party who does not consult it. ∎

**Theorem 4.4 (Substrate Independence).** Given a content-addressed signed-memento substrate satisfying the three primitives (sign, hash, reference) and the substrate-vs-metadata cut (envelope/header/body) of `2026-05-03-substrate-layers-envelope-header-body.md`, with N witness producers, M consumer-side kits, K policy authors, J jurisdictions, and L distribution channels, the total coordination cost is O(N + M + K + J + L), not O(N × M × K × J × L).

*Proof.* Lemma 4.4.1 bounds producer-side coordination at O(N): each producer attaches independently. Lemma 4.4.2 bounds consumer-side coordination at O(M): each consumer attaches independently. Lemma 4.4.3 bounds policy-side coordination at O(K): each policy attaches independently. Jurisdictions J are bounded by Lemma 4.4.3 applied at the jurisdictional layer (each jurisdiction's authority is one or more policy authors). Distribution channels L are bounded by content-addressing: each channel's correctness is verifiable from the bytes alone, so adding a channel coordinates with nothing. Summing: O(N+M+K+J+L). The cross-product term does not appear because no triple (P, C, K), no quadruple (P, C, K, J), and no quintuple (P, C, K, J, L) requires multi-way agreement before functioning. ∎

The N+M+K bound stated informally in §4.1 is now the conclusion of Theorem 4.4. The marketplace property of §4.2 is its operational manifestation. The architectural humility of §4.3 is its design-philosophy expression.

### §4.5 Corollaries

Seven corollaries fall out of Theorem 4.4. Each cites the theorem directly.

**Corollary 4.5.1 (Centralization breaks the theorem).** Every centralizing assumption (a coordinating registry, a global truth source, an enforcement intermediary, a centrally-approved producer list) re-introduces the cross-product term. By Theorem 4.4, this changes the bound from O(N+M+K) to O(N×M×K) in the affected axis pair. The substrate's refusal of centralizing assumptions at every design choice is precisely what preserves the theorem's bound. TCP/IP, npm, BitTorrent, Git are existence proofs of the same theorem applied to different content; their scale is mechanical, not accidental. Centralized ecosystems empirically cap at medium size for the same mechanical reason.

**Corollary 4.5.2 (Valence symmetry).** The theorem is symmetric over positive and adversarial witnesses. Producer independence (Lemma 4.4.1) is independent of the body content's epistemic direction; a memento attesting `post → pre` and a memento attesting `not (post → pre)` are equally valid producer outputs under the same envelope cryptography. Consequence: CVE researchers and Coverity attestation producers both attach at O(1) under the same shape. Defensive and offensive scaling are mechanically symmetric. Most security architectures privilege defensive publication; the substrate does not, by Theorem 4.4.

**Corollary 4.5.3 (Launch viability).** The substrate does not require pre-launch coordination with N producers, M consumers, or K policies. By Theorem 4.4, each integrates at O(1) at any time after the substrate spec is frozen and the reference implementation works. Network effect kicks in once the spec is frozen, not before. The marketplace forms from the theorem, not from outreach.

**Corollary 4.5.4 (Spec evolution viability).** Adding a new memento kind (a new claim category, a new body schema, a new policy type, a new bridge variant) is O(1) at the spec-author level. By Theorem 4.4, existing kits, consumers, and policies do not need to coordinate or change unless they choose to interpret the new kind. The substrate's evolutionary capacity is itself bounded by the theorem. The v1.4 additive bump (substrate-layers, contract-cid-vs-attestation-cid, contract-set-extension, version-chains-pinning, bridge-target-dimensionality, bridge-linkage-protocol, binary-attestation-protocol) is one instance: each new spec is a body convention added at O(1) cost; existing implementations either upgrade or continue against v1.1+ mementos unchanged.

**Corollary 4.5.5 (Standardization viability).** Once the substrate spec is frozen and any standards body ratifies it, per-regulator adoption is O(1). By Theorem 4.4, the standards body's work is one-time; each regulator's adoption is independent of every other regulator's. RTCA, ENISA, ISO TC 22, CCRA, FDA, and FedRAMP each publish a policy memento under their authority key without coordinating with each other. This is the structural justification for §10 of this paper: not "this should be faster" but "the cross-product term does not appear in the standardization landscape either."

**Corollary 4.5.6 (Org-topology gauge invariance).** The theorem holds independent of the organizational topology of the participants. A 1-person team can ship a kit at O(1); a 1000-person consortium can ship a policy at O(1); a sovereign nation-state can ship a regulator-level policy at O(1); none coordinates with the others. By Theorem 4.4, the coordination cost is bounded per attachment, regardless of the attaching party's organizational scale. This is stronger than Conway's Law inverted: Conway's Law says system architecture mirrors organizational structure; Theorem 4.4 says system architecture is *invariant* under choice of organizational gauge. The substrate works at all org scales because it refuses to require coordination at any of them. This is the gauge-theoretic analog of Theorem 4.4: the substrate is invariant under choice of organizational gauge, in the same sense in which physical laws are invariant under choice of coordinate system.

**Corollary 4.5.7 (Scale-freeness).** Theorem 4.4 applies recursively at every granularity at which the substrate decomposes work into producer / consumer / policy axes. The eleven-kits-in-parallel instantiation tonight is one frame; the lift-adapter-authors versus conformance-harness-authors interaction inside one kit is another; the spec-authoring versus spec-implementation versus spec-adoption decomposition is a third; the writer / reader / reviewer triangle of any individual document is a fourth. Each scale has its own (P, C, K) instantiation; each instantiation gets the same O(N+M+K) bound. The theorem is fractal: the same proof applies at different granularities without modification, because content-addressing, signing, and reference are scale-invariant primitives. Architectural humility scales because the theorem is scale-free.

The seven corollaries are not seven separate consequences. They are gauge-invariance statements applied to three orthogonal choice-spaces, and together they exhaust the consequence space.

**C1 and C6 are the same fact with opposite signs along the organizational axis.** C1 (centralization breaks the theorem) is the negative form: it names which preferred frames would break O(N+M+K). C6 (gauge invariance under organizational topology) is the positive form: it names the invariance preserved when those frames are refused. This is what gauge symmetry is in physics: the property preserved precisely when no preferred coordinate frame is chosen. C1 spells out the would-be preferred frames; C6 states the symmetry their absence preserves. A reader debugging a leaky abstraction cites C1; a reader defending the substrate against a centralization argument cites C6. Same content, different sign.

**C3, C4, and C5 are the same invariance applied to the temporal axis.** Time-of-attachment never enters the cost equation. The substrate is invariant under whether a participant attaches pre-launch (C3), during inter-version evolution (C4), or after regulatory adoption (C5). The lemma operates identically across these three time horizons because the lemma's premises do not refer to time. The substrate is gauge-invariant under choice of attachment moment.

**C2 and C7 are the same invariance applied to two more axes.** C2 names invariance under content valence (positive vs. adversarial witnesses); the lemma does not privilege epistemic direction, so positive and negative attestations scale identically. C7 names invariance under decomposition scale (kit-level vs. adapter-level vs. document-level); the lemma does not privilege a level of decomposition, so the same proof applies to any (P, C, K) instantiation regardless of granularity.

All three pairings are gauge-invariance statements. The substrate refuses to privilege any choice along any axis: not organizational topology, not time of attachment, not content valence, not decomposition scale. Theorem 4.4's bound is what this universal refusal preserves. The seven corollaries exhaust the consequence space because they cover the orthogonal axes along which a substrate could have privileged a choice and didn't.

---

## §5. Existing frameworks as body conventions

If the substrate's content-agnosticism is real, existing supply-chain frameworks should be expressible as body conventions plus tooling. They are. This section maps the major frameworks onto the substrate explicitly.

### §5.1 SLSA

The Supply-chain Levels for Software Artifacts framework defines build-provenance attestations. SLSA Level 1-4 each prescribes attestation content. Under the substrate:

- A SLSA L3 attestation is a memento with `kind: "build-provenance"` in the header, `metadata.slsaLevel: 3`, and SLSA's specified content (builder identity, source CID, build parameters, materials, byproducts) in the body.
- The signer is the build system's authority key, typically rooted via Sigstore's OIDC-issued certificates.
- Consumers configured for "SLSA L3 required" check `metadata.slsaLevel >= 3` plus the signer trust.

SLSA's framework can be carried natively. SLSA's content is body schema. ProvekIt's substrate transports it without modification.

### §5.2 Sigstore (Cosign + Fulcio + Rekor)

Sigstore signs artifacts under OIDC-rooted certificates with transparency-log inclusion proofs.

- A Cosign signature is a memento whose envelope's `signer` field is a Sigstore-issued certificate's public key, and whose body includes the Fulcio certificate chain and the Rekor inclusion proof.
- Consumers configured to trust Sigstore identities verify the certificate chain rooted at Fulcio plus the Rekor inclusion proof from the body.

Sigstore's identity binding is body convention; the substrate transports the certificate chain and inclusion proof without interpretation.

### §5.3 in-toto

in-toto attestations capture pipeline steps with named functionaries.

- Each in-toto step is a memento with `kind: "pipeline-step"`, body fields naming step inputs, outputs, materials, products, and the executing functionary.
- A pipeline is a chain of such mementos linked by content-addressed references (input mementos' CIDs in `metadata.inputCids`).
- Consumers verify the chain by walking the DAG.

in-toto's link metadata is body schema; the substrate transports the pipeline structure as a memento DAG.

### §5.4 SCITT

SCITT (Supply Chain Integrity, Transparency, and Trust) is the IETF's standardization track for attestation transparency logs.

- A SCITT entry is a memento that records inclusion in a transparency log. The body carries the log's identity, the inclusion proof, and the timestamp.
- The substrate transports SCITT entries as ordinary mementos; the transparency log is one consumer of the substrate among many, providing inclusion proofs for mementos transported on the substrate.

SCITT and the substrate compose: SCITT can serve transparency over substrate mementos; the substrate can transport SCITT inclusion proofs as body content.

### §5.5 CycloneDX and SPDX

These are inventory formats. The body of an inventory memento carries the CycloneDX or SPDX document; the signer is the inventory generator (typically the build system).

- A CycloneDX SBOM becomes a memento with `kind: "inventory"`, `metadata.format: "cyclonedx-1.5"`, the SBOM in body.
- An SPDX SBOM is the same shape with `metadata.format: "spdx-2.3"`.

Both inventory formats are body schemas. The substrate transports them. Consumers can request SBOMs via memento lookup, walk inventory references via content-addressed CIDs, and combine inventory with behavioral attestations under one trust calculus.

### §5.6 OSCAL

NIST's Open Security Controls Assessment Language is the most directly aligned existing standard with policy-as-memento. OSCAL profiles author controls; OSCAL component definitions describe what a component implements; OSCAL system security plans (SSPs) document compliance.

- An OSCAL profile becomes a policy memento under the substrate. The body carries the OSCAL profile document; the signer is the regulator or the controls-authoring authority.
- An OSCAL component definition becomes a contract memento or a compliance attestation depending on whether it asserts capability or describes implementation.
- An SSP becomes an audit attestation citing the OSCAL profile by CID.

OSCAL is mechanically the closest existing framework to the substrate's policy-as-memento shape. NIST, ISO, and federal procurement programs that already use OSCAL have minimal adaptation cost: the OSCAL document is body content; signing under an authority key is the substrate adaptation.

### §5.7 The implication

Existing supply-chain frameworks are not competitors to the substrate. They are body conventions the substrate transports natively. A SLSA-conformant build system, a Sigstore-rooted signing pipeline, an in-toto pipeline, a SCITT transparency log, a CycloneDX SBOM generator, an OSCAL profile authoring tool: all are tools that emit body content the substrate transports.

The substrate is the wire under all of them. Adoption does not require choosing one framework over another. The substrate is multilingual: it carries SLSA and Sigstore and in-toto and SCITT and CycloneDX and OSCAL simultaneously. Consumers configure their policy to require whichever combination of frameworks their jurisdiction or threat model demands.

---

## §6. Policy as memento

Consumer trust policy and regulator policy are both body content under the substrate. This generalizes the version-chains-pinning spec §6 ("a consumer's trust policy is itself a memento") to the regulatory-authority case.

### §6.1 The shape of a policy memento

```json
{
  "envelope": {
    "signer":     "ed25519:<authority-key>",
    "declaredAt": "2026-Q3",
    "signature":  "ed25519:<...>"
  },
  "header": {
    "schemaVersion": "1",
    "kind":          "regulatory-policy",
    "cid":           "blake3-512:<self-cid>"
  },
  "metadata": {
    "policyName":            "DO-178C-DAL-A",
    "version":               "2026-Q3",
    "previousPolicyCid":     "blake3-512:<prior version, optional>",
    "trustedSigners":        ["ed25519:<Coverity-validated>", "ed25519:<CompCert-foundation>", ...],
    "requiredContracts":     ["blake3-512:<DAL-A-required-contract-set>"],
    "requiredWitnesses": {
      "minBackends":  2,
      "requiredFrom": ["ed25519:<CompCert-foundation>", "ed25519:<Frama-C-foundation>"]
    },
    "yankPolicy":      "strict",
    "channelRequirement": "stable-only",
    "advisoryAuthorities": ["ed25519:<MITRE-CVE>", "ed25519:<DHS-CISA>"]
  }
}
```

The policy is signed by the regulator's authority key. Its CID is content-addressed. Consumers in regulated environments pin to the policy CID, not to the regulator's enforcement infrastructure.

### §6.2 The pin file

A consumer's pin file references policies by CID:

```toml
[policy]
aviation = "blake3-512:<DO-178C-DAL-A-policy-cid>"
internal = "blake3-512:<our-internal-policy-cid>"

[dependencies.flight-control]
attestationCid = "blake3-512:<the maintainer's attestation>"
policy         = "aviation"
```

Resolving a dependency goes through the policy: walk the dependency's `.proof`, validate each witness against `policy.metadata.trustedSigners`, validate `contractCid` is in `policy.metadata.requiredContracts`, walk `previousContractSetCid` chains back to the policy's required base, check for adversarial witnesses from `policy.metadata.advisoryAuthorities`. The policy IS the verification logic, content-addressed, signed by the authority that authored it.

### §6.3 Policy versioning

Policies evolve: new analyzers approved, new contracts required, new advisories integrated, deprecated controls removed. A new policy version is a new memento with `previousPolicyCid` pointing at the old one. Consumers update their pin when they're ready.

This is structurally identical to the contract-set-extension chain: the policy is also a content-addressed sequence of mementos, each linked to its predecessor, with a verifiable chain. A consumer pinned to v2026-Q3 can audit the chain back to genesis; an auditor can verify what the policy required at any historical point.

The maintenance surface is push (new policy memento) plus pull (consumer-side pin update); neither party coordinates with the other. Same shape as version chains for libraries.

### §6.4 Multiple policies, parallel chains

Different authorities publish parallel chains. A consumer in regulated avionics pins to RTCA's policy; the same consumer's CI pipeline also pins to the company's internal policy; the same consumer also honors NIST SP 800-218 for federal contracts. Three policies, three pins, one substrate.

A binary is acceptable in a context iff it satisfies the relevant policy for that context. Cross-policy enforcement is consumer-side: "for production deployments, require both internal and aviation policies; for development, only internal policy applies."

This is jurisdictional pluralism without registry centralization. Each authority's policy is independently authored, signed, and distributed. Consumers compose them by intersection (require all) or by selection (pick the relevant one for context). The substrate does not pick.

### §6.5 The standardization implication preview

The standardization story (§10 below) follows directly. Per-regulator adoption of substrate-rooted verification is "publish a policy memento under the regulator's authority key." That is the work. The substrate spec gets standardized once, by some standards body. Each regulator publishes their policy independently. The 10-15 year horizon for "the regime accepts hash-bounded verification" becomes a 1-2 year horizon for "the regime publishes its policy memento."

---

## §7. The TCP/IP analogy

The substrate-as-transport, policy-as-application-layer architecture has a structural precedent in TCP/IP. Making the analogy explicit clarifies what the substrate is and is not.

TCP carries packets. The protocol is content-agnostic: the transport does not know whether a packet carries HTTP, SMTP, SSH, BitTorrent, Tor, or anything else. Different application protocols ride on top, each defining its own conventions for what packet payloads mean. None requires TCP to amend.

TCP got standardized once: RFC 793 in 1981, with subsequent maintenance. Application protocols are independent: HTTP/0.9 in 1991, HTTP/1.0 in 1996 (RFC 1945), HTTP/1.1 in 1997 (RFC 2068), HTTP/2 in 2015 (RFC 7540), HTTP/3 in 2022 (RFC 9114). Each application protocol's specification is a separate document; each has its own working group; each evolves on its own schedule. TCP itself is essentially unchanged.

The composition produces an ecosystem in which different jurisdictions, networks, and applications all share infrastructure. Different countries' firewall policies layer on top of TCP without re-specifying TCP. Different content-moderation regimes layer on top without re-specifying. PCI-DSS for payments, HIPAA for healthcare, FedRAMP for federal cloud: each is a regime layered on top of TLS (which is layered on top of TCP). None requires TCP or TLS to amend; they layer policy.

ProvekIt's substrate has the same architectural shape. The substrate (sign + hash + reference + envelope/header/body, plus the four invariants) is the wire. Application protocols ride on top: SLSA-style provenance, Sigstore-rooted signing, in-toto pipelines, SCITT transparency, OSCAL controls, regulator policies. Each application protocol defines its own body conventions.

The standardization implication is the same as for TCP. The substrate gets standardized once. Application protocols and policies evolve independently. Different regulators, jurisdictions, and ecosystems compose on the same wire.

This is unfamiliar terrain for regulatory bodies because most existing supply-chain frameworks (SLSA, in-toto, SCITT) standardize both transport and content together. The substrate's separation is what enables policy-as-memento. Without the separation, every regulator's adoption ask requires amending whichever framework's spec they chose; with the separation, it requires publishing a body-content memento under the regulator's authority key.

The architectural move is not novel. TCP/IP did it forty years ago. Applying the same move to verification transport is the contribution.

---

## §8. Transport fungibility

The substrate's content-addressing produces a third architectural property: distribution mechanism is independent of substrate semantics. Mementos are bytes; their CIDs verify their integrity; the channel that delivers them is irrelevant.

### §8.1 Channels the substrate runs on

Concrete examples:

- **IPFS** (InterPlanetary File System): native fit; IPFS is built on content-addressing. Mementos retrieved by CID, replicated through the DHT.
- **BitTorrent**: chunk-content-addressed; torrents can carry mementos as payload, with infohashes deriving from the same chunking the substrate uses.
- **HTTPS**: serve the bytes at any URL; the consumer recomputes the CID and verifies against the pin. The URL is hint, not authority.
- **git**: blob-content-addressed (SHA-1 today, SHA-256 in flight). Mementos can be stored as git blobs in a repo; their substrate CID is BLAKE3-512(blob bytes), independent of git's hash.
- **S3 / Azure Blob Storage / GCS**: object storage with the CID as key. Any cloud's storage works.
- **USB sticks, microSD, CD-ROM, DVD, paper**: file system or printed bytes. The CID verifies regardless of physical medium.
- **Email attachments, Slack messages, QR codes**: any byte-passing channel. The CID verifies on receipt.
- **Bitcoin OP_RETURN**: 80 bytes of arbitrary data per Bitcoin transaction, sufficient to carry a CID anchor. Mementos can be notarized in the Bitcoin chain by anchoring their CIDs in OP_RETURN, inheriting Bitcoin's timestamping at the cost of a transaction fee.
- **Mesh networks, peer-to-peer gossip, Tor hidden services, decentralized social networks**: any peer-to-peer channel that moves bytes.

The substrate does not specify a distribution mechanism because it cannot. Content-addressing is what makes intermediaries fungible: the channel does not certify the bytes; the bytes certify themselves.

### §8.2 No "ProvekIt server"

A consequence of transport fungibility: there can be no central ProvekIt server, because there cannot be one. The substrate is content-addressed and signed; any party with the bytes is an equally authoritative source. A "ProvekIt registry" would be one indexing service among many, helpful for discovery but not load-bearing for trust.

This is the property that made BitTorrent approximately 30% of peak internet traffic in the mid-2000s. Once content was addressable by hash, every peer holding the bytes was a server. The protocol's value scaled with adoption rather than depending on central capacity.

The substrate inherits this property. Adoption does not bottleneck on hosting capacity. Replication is a derived view; mirroring is automatic for any party that finds value in serving mementos. Discovery indices are optional layers; the substrate works without them, just less conveniently.

### §8.3 Mixed-channel deployments

A realistic deployment uses different channels for different mementos by their cost-and-availability profile:

- **Policy mementos** by regulators: served via HTTPS from the regulator's domain (familiar trust ergonomics, manageable revocation).
- **Witness mementos** by analyzer vendors: served via vendor HTTPS plus IPFS replication (vendor-controlled availability with content-addressed verifiability).
- **Binary mementos** for popular open-source packages: served via package registry (npm, crates.io, PyPI) plus BitTorrent and IPFS for redundancy.
- **High-assurance attestations**: notarized via Bitcoin OP_RETURN (timestamping, inheritable security budget).
- **Air-gapped environments**: distributed via USB or CD-ROM, with consumer-side CID verification.
- **Regulated-jurisdiction mirroring**: each jurisdiction's mirror serves the bytes locally, simplifying export-control or data-residency requirements.

The substrate works across all of these simultaneously. A single memento may be available via five different channels; consumers fetch from the cheapest, fastest, or most-trusted-locally channel and verify via CID. Channels do not need to coordinate; new channels can be added without protocol changes.

### §8.4 The architectural consequence

Transport fungibility produces three operational properties:

- **No single point of failure.** Loss of any one channel does not break verification. The bytes are recoverable from any other channel.
- **No gatekeeper.** No party can prevent participation by withholding distribution. Any peer with the bytes is a source.
- **Trust-decoupled distribution.** The party serving the bytes need not be trusted. The bytes' CID is the trust anchor.

These three are what content-addressing enables. The substrate inherits them by construction. Existing supply-chain frameworks that bind to specific distribution mechanisms (registry-rooted SBOMs, transparency-log-bound SCITT entries) sacrifice these properties; they purchase convenience at the cost of architectural flexibility. The substrate makes the opposite trade.

---

## §9. Spec authorship is not spec ownership

The substrate's specification is content-addressed and frozen at a CID. Once authored, the spec has the same status as any other content-addressed memento: anyone can read it, verify it, implement against it, but no party can unilaterally change the bytes that hash to that CID. Future versions are different bytes hashing to different CIDs. The spec's authorship is identifiable; its ownership is not.

This is the cURL/HTTP relationship made explicit, and it is the governance property that distinguishes a protocol from a tool.

### §9.1 The cURL/HTTP relationship

Tim Berners-Lee invented the World Wide Web at CERN in 1989-1990 and authored the first HTTP specification (HTTP/0.9 in 1991). HTTP/1.0 was standardized as RFC 1945 in 1996; HTTP/1.1 in RFC 2068 (1997) and RFC 2616 (1999); HTTP/2 in RFC 7540 (2015); HTTP/3 in RFC 9114 (2022). The IETF HTTPbis Working Group governs the spec's evolution.

Berners-Lee authored HTTP. He does not own it. The IETF's standards process governs evolution; nobody approves new HTTP versions without working-group consensus; nobody can force HTTP to evolve unilaterally. The spec is a public artifact.

cURL is an HTTP client. Daniel Stenberg started cURL in 1996 and continues to lead its development. cURL is one of the most widely-deployed HTTP implementations in the world. Stenberg does not own HTTP. cURL is one client among many (browsers, libraries, CLI tools, embedded clients). The cURL project's authority is over cURL's source code; it has no authority over HTTP itself.

The split between spec authorship and spec ownership is what enabled HTTP's marketplace. Every browser vendor, every server vendor, every middlebox manufacturer, every library author can implement HTTP independently. None coordinate with cURL's team to ship; none coordinate with the IETF to change cURL's behavior; the working group's spec is the invariant, and implementations form an ecosystem around it.

Without this split, HTTP would have been one project's product. With it, HTTP is the wire of the modern web.

### §9.2 The substrate's analog

The substrate's specification was authored by an architect (or a small architectural team) and committed in the v1.4 catalog at content-addressed CID `blake3-512:b0f2030d...`. The catalog plus the seven 2026-05-* specs constitute the substrate's wire RFC: substrate-layers-envelope-header-body, contract-cid-vs-attestation-cid, contract-set-extension, version-chains-pinning, bridge-target-dimensionality, bridge-linkage-protocol, binary-attestation-protocol, plus the manifesto that articulates the design philosophy.

Once committed, the spec is content-addressed and immutable. Future versions are new bytes producing new CIDs. The architect's authorship is identifiable in commit history, in attestations the architect signs (provenance attestations committing to "I authored these spec bytes"), and in the manifesto's voice. But once published, the spec is everyone's: anyone can read it, implement against it, audit it, propose changes via whatever standardization process subsequently governs evolution.

The reference implementation in this repository (the Rust kit, the per-language conformance harness, the `provekit prove` CLI) bootstraps adoption. It is the equivalent of cURL: a high-quality, well-maintained client that demonstrates the spec is implementable and provides early adopters with a working tool. It is not the spec's authority. The spec is its own authority because the spec is content-addressed.

### §9.3 Implementations are forkable

A consequence: any party can implement the substrate against the spec without coordinating with the original authors. A government wanting an air-gapped implementation can fork the Rust reference, harden it, certify it under their own quality regime, and deploy. A vendor wanting a commercial implementation can write one in Rust, C++, Go, OCaml, Haskell, or any language; if it conforms to the spec's CID, it interoperates with every other conformant implementation.

This is what conformance harnesses are for. The harness verifies that a candidate implementation produces bytes that match the spec's expectations on a fixture set; passing the harness is what makes an implementation conformant. The substrate's harness lives in the repository under `make conformance`; conforming kits in Rust, Go, C++, TypeScript, C# all pass it; the harness itself is content-addressed.

A fork that passes the harness is conformant. Conformance is not granted by authority; it is verified by the harness. Implementations are interchangeable.

### §9.4 Authorship attestation, not spec amendment

The architect's continuing role is to attest authorship. A provenance attestation under the architect's signing key commits to "these bytes are the substrate spec at this version." The attestation is a memento; its CID is content-addressed; consumers can verify it. The attestation does not control implementation choices; it identifies the spec's origin.

Future spec evolution proceeds via whatever standardization process the ecosystem chooses. The spec might be brought to the IETF, to W3C, to IEEE, to a new dedicated standards body, or to no formal body at all (the substrate would still work; standardization ratifies practice rather than enabling it). The architect can participate or step back. The spec is fixed by its CID; evolution is new CIDs; legitimacy is the consumers who pin to specific catalog versions.

This is the post-central-authority governance shape. The architect chose not to control downstream because controlling would prevent the marketplace property (§4). The architectural humility is the leverage.

### §9.5 The strategic implication

A protocol that the architect tries to control is a tool, not a protocol. Tools have product roadmaps, feature requests, paid support, vendor lock-in, deprecated APIs, and centralized release cycles. Protocols have specifications, conformance tests, multiple implementations, marketplace dynamics, and evolution by consensus.

The substrate is positioned as a protocol. The reference implementation is the bootstrap. The marketplace forms downstream. Implementations are forkable. The architect's continuing contribution is authorship attestation and (optionally) participation in the standardization process, not gatekeeping over which kits, analyzers, consumers, or jurisdictions get to participate.

This is the only positioning that makes the §4 marketplace property real. Any other positioning (architect-controlled implementation list, architect-approved analyzer registry, architect-issued conformance certificates) would re-introduce the coordination cost the substrate refuses to bear. The architectural humility and the marketplace property are the same property viewed from two angles: from the architect's perspective, it is humility; from the ecosystem's perspective, it is the leverage that makes the substrate scale.

---

## §10. Standardization restructure

§5 (existing frameworks as body conventions), §6 (policy as memento), §7 (TCP/IP analogy), §8 (transport fungibility), and §9 (spec authorship vs. spec ownership) compose into a restructure of the standardization story.

Paper 04 walked per-regulator engagement paths assuming substrate-level amendment (each regulator's standard amended to recognize hash-bounded verification). Under this paper's framing, that level of engagement is not what regulators need to do.

What regulators do is publish policy mementos under their authority keys. The substrate spec gets standardized once, by whichever standards body chooses to ratify it. After that, each jurisdiction's adoption is independent.

This section restates three representative regulator paths under the new framing. The remaining four (ISO 26262, FDA, FedRAMP, NIST SSDF) follow the same shape; see paper 04 for their substrate-level engineering paths.

### §10.1 DO-178C / DO-333 (avionics)

**Old framing (paper 04):** RTCA SC-205 / EUROCAE WG-71 amend DO-333 to recognize hash-bounded verification. Tool qualification kit (TQL-1) for a ProvekIt implementation. 10-15 year horizon.

**New framing:** RTCA publishes a policy memento under the RTCA authority key. The memento body specifies:

- `requiredContractSetCid`: the CID of the contract set DO-178C DAL-A requires.
- `trustedAnalyzers`: signing keys of qualified verification tools (Astrée, Frama-C, CompCert, plus anything else qualified under DO-330 tool qualification).
- `requiredWitnesses.minBackends`: 2 (or another number per the assurance level).
- `advisoryAuthorities`: signing keys of CSIRTs the regulator recognizes.
- `previousPolicyCid`: the prior version's policy memento CID, when revising.

Consumers in regulated avionics pin to the policy memento's CID. Their build verifier walks each artifact's `.proof` against the policy: confirm contract CID is in `requiredContracts`, witness signers are in `trustedAnalyzers`, no adversarial witnesses from `advisoryAuthorities` apply. The verifier's logic is determined by the policy memento; no per-deployment configuration.

The substrate-level engineering work (TCB minimization, constructive-proof backend integration, conformance harness governance) is paper 04's track and continues at 5-15 year horizons. The regulator-level work, what RTCA actually does to adopt, is publishing the policy memento. Estimated horizon: 1-2 years from sustained engagement to first published policy.

### §10.2 Common Criteria EAL5+

**Old framing (paper 04):** CC Recognition Arrangement (CCRA) coordinates 31 nations on a CC revision recognizing hash-bounded verification. Protection Profile (PP) work via national schemes (BSI, CCN, NIAP). 10-year horizon.

**New framing:** Each Protection Profile becomes a policy memento. A PP for a smart-card OS, an HSM, a TEE, or a high-assurance database is signed by the PP authoring authority (typically a national scheme); its body specifies the same shape as §10.1's DO-178C policy.

Consumers in CC-certified products pin to the relevant PP's policy CID. National schemes can publish their own variants; the CCRA's mutual-recognition mechanism becomes "PP A's policy memento is recognized by jurisdictions X, Y, Z", itself a memento of recognitions, signed by each jurisdiction's authority.

Estimated horizon: 1-2 years per PP, parallelizable across PPs. CCRA mutual recognition becomes a side question; jurisdictions independently honor PPs by including their CIDs in their own policy memento sets.

### §10.3 EU Cyber Resilience Act

**Old framing (paper 04):** ENISA guidance for CRA Article 13 (essential cybersecurity requirements) recognizes ProvekIt-backed verification for high-risk products. 5-year horizon.

**New framing:** ENISA publishes a CRA-compliance policy memento under ENISA's authority key. The policy specifies:

- `requiredContractSetCid` per product category (consumer-software, infrastructure-software, critical-products).
- `trustedAnalyzers` per category (national-scheme-validated tools).
- `requiredAttestations`: SLSA L3 build provenance, SBOM (CycloneDX or SPDX), behavioral verification per the contract set.
- `vulnerabilityAuthorities`: ENISA's own CSIRT key plus EU national CSIRTs.

Vendors of CRA-scope products pin to the ENISA policy CID. Their build pipeline produces attestations satisfying the policy; importers and consumers verify them by walking the policy.

Estimated horizon: 1-2 years from ENISA engagement to first published policy memento, alongside CRA's force-of-effect timeline (currently 2027).

### §10.4 The aggregate horizon

Restated:

| Horizon | Achievable by |
|---|---|
| 1 year | NIST SSDF / OpenSSF SLSA recognize substrate-rooted attestations as one mechanism; first policy memento under any major authority key |
| 2 years | EU CRA / ENISA publishes baseline policy memento; first regulator-published memento; conformance gates for regulator-authored policies |
| 3 years | FedRAMP / FIPS 140-3 cryptographic-module policy memento; ISO 26262 ASIL-D policy via ISO TC 22 |
| 5 years | Common Criteria PPs published as policies under national-scheme authority keys |
| 7 years | DO-178C / DO-333 policy memento under RTCA authority |
| 10-15 years | Substrate spec ratified by some major standards body (W3C, IETF, IEEE, OpenSSF), enabling broader recognition |

The substrate-level engineering work proceeds independently; paper 04's roadmap for TCB minimization, tool qualification, and conformance harness governance applies. The regulator-level work is publishing policy mementos; that work parallelizes per regulator, has clear shape (sign a memento, distribute it, name the trust set), and does not require regulators to coordinate with each other.

The standardization reframe is not "this is faster." It is "this is differently shaped." The substrate-level work is still the substantial engineering; the regulator-level engagement collapses to memento publication. Whoever invests in the substrate engineering invests in unblocking the entire regulator-level layer.

---

## §11. Counterarguments

A complete paper engages plausible counterarguments. Eight of them follow.

### "This sounds too easy. Real standardization is harder."

Yes, the substrate-level standardization is hard. TCB minimization (constructive-proof backends, multi-backend concurrence, formal semantics for the IR), tool qualification kits (TQL-1 evidence packages), conformance harness governance, multi-vendor interoperability testing: all are years of engineering work. This is paper 04's track and is not collapsed by anything in this paper.

What collapses is the regulator-level adoption ask. Pre-substrate, "DO-178C accepts hash-bounded verification" was a 10-15 year project of amending DO-333. Post-substrate, "RTCA publishes a DO-178C-DAL-A policy memento" is a 1-2 year publication project. The regulator's work shrinks; the substrate-level work does not.

### "Won't this fragment the ecosystem by per-jurisdiction policy?"

Yes, and that is correct. Different jurisdictions have different requirements; their policies should differ. EU CRA and FDA SaMD do not have identical safety regimes, and pretending they do would do violence to the actual law. What does not fragment is the wire format. A consumer in one jurisdiction can read mementos minted under another jurisdiction's policy; their own policy decides whether to accept.

This is the same shape as TLS certificate handling. Each jurisdiction has its own root certificate authorities (eIDAS in EU, FBCA in US, JCN in Japan); each operating system or browser ships its own root store; products operating in multiple jurisdictions handle multiple root stores. TLS itself does not fragment; trust stores and policies do. The fragmentation is the right shape for jurisdictional sovereignty.

### "Why isn't SLSA / Sigstore / SCITT good enough?"

These frameworks are good and complementary, not replaced. §5 explicitly maps them onto the substrate as body conventions. The substrate's value-add is composition: SLSA can attest build provenance, Sigstore can root identity, SCITT can provide transparency, OSCAL can author controls, all simultaneously, on the same wire, with one trust calculus. Existing frameworks specify both transport and content together; the substrate separates them, enabling composition.

### "What about discovery? Without a central registry, how do consumers find policy mementos?"

The same way they find any memento: by CID, fetched from any content-addressable channel. Indexers exist as convenience layers; they are not load-bearing for trust. Multiple competing indexers can exist; all are correct as long as they verify the CIDs they advertise. A consumer looking for ENISA's CRA policy memento can fetch from ENISA's HTTPS endpoint, from an IPFS replica, from a cached mirror, from a peer in the same regulated industry. The CID is the trust anchor; the channel is fungible.

### "What stops a malicious party from publishing a fake policy memento?"

Nothing. Anyone can publish a memento under their own signing key. Adoption requires explicit consumer choice: a consumer pins to a policy CID, which means they have decided to honor that authority. A fake policy memento exists on the substrate but is unconsumed if no consumer pins to it. The substrate is permissionless on the publishing side and policy-driven on the consuming side.

This is the same shape as DNSSEC (anyone can publish a zone, but trust is anchored at root keys), TLS (anyone can issue a certificate under their own CA, but trust is anchored at root stores), and PGP/GPG (anyone can sign, but trust is web-of-trust or explicit). The substrate inherits the shape and the security model.

### "Adversarial witnesses produce a denial-of-service attack: flood the network with fake CVEs."

A flood of fake adversarial witnesses (under non-trusted signing keys) consumes bandwidth but does not affect verification. Consumers honor only adversarial witnesses from their `advisoryAuthorities` trust set. A fake CVE under an unknown key is unconsumed. The flood is wasted effort.

A flood under a trusted signing key (compromised authority) is a key-compromise attack, not a flooding attack. Mitigation: hardware-key signing for advisory authorities, revocation lists, multi-signature requirements for high-severity advisories. These are operational practices around the substrate; the substrate provides the hooks.

### "Linear scaling claims are theoretical. In practice, integration is hard."

The N+M+K vs. N×M×K math is structural. Integration friction in practice comes from per-tool quirks (an analyzer's specific output format, a kit's specific lift-adapter constraints, a consumer's specific policy requirements). The substrate does not eliminate these frictions; it bounds them. Each axis still has implementation cost; the cost does not multiply across axes.

Empirically, npm has 2.5 million packages because the integration cost per package is bounded. The substrate's witness layer aims for the same property, with a bound determined by the body schema each tool emits and the lift adapter each kit implements. Both are linear in the participants on each axis.

### "The architect's authorship attestation is just centralization in disguise. Who counter-signs the architect?"

Nobody, structurally. The architect's authorship is identifiable but not authoritative; the spec is content-addressed and authoritative on its own. A future revision under the architect's key is one revision among many; if the ecosystem chooses to follow it, they update their pins; if they don't, they don't. Forks are first-class.

This is the same governance question that applies to Linux (Torvalds), Bitcoin (Satoshi, then the post-Satoshi maintainers), Git (Torvalds), and HTTP (Berners-Lee initially, then IETF). In each case the original architect is a maintainer (or in Bitcoin's case, a memory) but not an authority. The protocol's authority is its content-addressed specification plus the consumers who choose to honor it. This paper claims that pattern is structurally correct for a verification-transport protocol and architecturally identical to how the internet's foundational protocols evolved.

---

## §12. Relationship to paper 04

Paper 04 (the vertical-stack and standardization paper) and paper 05 are complementary, not redundant. Their division of labor:

**Paper 04:**

- The vertical-stack thesis (a `.proof` is structurally identical to a chain of formally verified software from quantum mechanics to bytecode).
- Per-layer state-of-the-art survey (HOL4 ARM, CompCert, CakeML, Vellvm, seL4, HACL\*).
- The composition gap (no common substrate connects existing verifications).
- ProvekIt as the missing transport layer.
- Per-regulator engagement paths assuming substrate-level amendment.
- TCB minimization, tool qualification, and conformance harness engineering work.

**Paper 05:**

- Witness pluralism in kind and valence.
- The marketplace property (linear scaling) made literal.
- Existing frameworks as body conventions.
- Policy as memento (consumer and regulator).
- TCP/IP analogy for substrate-as-transport.
- Transport fungibility.
- Spec authorship vs. spec ownership.
- Standardization restructure (per-regulator paths under policy-memento publication).

The papers compose. Paper 04 makes the architectural claim and walks the engineering work. Paper 05 makes the standardization restructure precise and develops the security and governance implications. A reader wanting the full standardization story reads both.

§7's per-regulator paths in this paper supersede §11-§14 of paper 04 for the regulator-level adoption work. Paper 04's substrate-level engineering paths are not superseded. A future revision of paper 04 may fold this paper's reframe into its standardization sections; until then, paper 05's §10 is the authoritative restatement.

---

## §13. Conclusion

The substrate is content-agnostic. Every external claim about a binary (analyzer attestation, formal verifier output, test run, build provenance, audit signoff, reproducibility claim, regulator policy, adversarial witness) is the same envelope/header/body shape. Different signers, different body conventions, same wire format.

This produces six operational consequences. Witnesses pluralize in kind, in valence. Composition is free, so ecosystem extension scales linearly. Existing frameworks reduce to body conventions the substrate carries natively. Transport is fungible, so distribution mechanism is independent of substrate semantics. Spec authorship is not spec ownership, so implementations are forkable and the marketplace forms downstream.

The standardization story restructures. Regulators publish content-addressed policy mementos under their authority keys. Consumers pin to policy CIDs. The per-regulator adoption ask collapses to "publish a memento" rather than "amend a standard's text." Substrate-level engineering work continues at the longer horizon; regulator-level engagement collapses to a 1-2 year publication horizon per regulator, parallelizable.

The marketplace property is not a metaphor. It is the literal property of the architecture: N witness producers plus M kits plus K consumers plus L policies plus J jurisdictions, with costs that add not multiply across the axes. The architectural humility (the architect's choice not to control implementations, the spec's choice not to bind to a transport, the substrate's choice not to validate body content) is the leverage that makes the marketplace work.

The substrate stays small. The composition layer carries the world.

---

## References

**Internal (this repository):**

- Paper 01 (Whitepaper): `docs/papers/01-whitepaper.md`
- Paper 02 (Bluepaper): `docs/papers/02-bluepaper.md`
- Paper 03 (Substrate, not Blockchain): `docs/papers/03-substrate-not-blockchain.md`: manifesto §10 closure, §11 multi-dimensional address, §12 rank-N tuple
- Paper 04 (Vertical Stack and Standardization): `docs/papers/04-vertical-stack-and-standardization.md`
- Multi-dimensional pinning: `docs/security/multi-dimensional-pinning.md`
- Threat model (v1.4): `docs/security/threat-model.md`

**v1.4 specifications (in this repository):**

- `protocol/specs/2026-05-03-substrate-layers-envelope-header-body.md`
- `protocol/specs/2026-05-03-contract-cid-vs-attestation-cid.md`
- `protocol/specs/2026-05-03-contract-set-extension.md`
- `protocol/specs/2026-05-03-version-chains-pinning.md`
- `protocol/specs/2026-05-03-bridge-target-dimensionality.md`
- `protocol/specs/2026-05-03-bridge-linkage-protocol.md`
- `protocol/specs/2026-05-02-binary-attestation-protocol.md`
- `protocol/specs/2026-05-02-bundle-attestation-protocol.md`

**External standards and frameworks:**

- TCP/IP: RFC 793 (1981), RFC 9293 (2022)
- TLS: RFC 5246 (TLS 1.2, 2008), RFC 8446 (TLS 1.3, 2018)
- HTTP: RFC 1945 (HTTP/1.0, 1996), RFC 9110 (HTTP semantics, 2022), RFC 9114 (HTTP/3, 2022)
- DO-178C / DO-333 (avionics): RTCA SC-205 / EUROCAE WG-71
- Common Criteria: ISO/IEC 15408
- ISO 26262 (automotive): ISO/TC 22/SC 32/WG 8
- FDA SaMD: FDA Center for Devices and Radiological Health
- FedRAMP: GSA
- NIST SP 800-218 (SSDF)
- EU Cyber Resilience Act (CRA): European Commission, ENISA implementation guidance
- SLSA: OpenSSF
- Sigstore: Cosign + Fulcio + Rekor; Linux Foundation
- in-toto: ITE specification family
- SCITT: IETF SCITT Working Group
- CycloneDX: OWASP
- SPDX: Linux Foundation
- OSCAL: NIST
- CSAF: OASIS Common Security Advisory Framework
- OpenVEX: OpenSSF
