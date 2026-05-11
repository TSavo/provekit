# After Standardization: the vertical stack as a content-addressed registry

> **Status.** Sustained argument. Engages counterarguments. Written to be cite-able.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [03 Substrate, not Blockchain](03-substrate-not-blockchain.md), [05 Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md), [14 After Trust](14-after-trust-the-universal-correctness-bundle.md).
>
> **Premise the earlier papers established.** A protocol for content-addressable, cryptographically-signed, byte-deterministic claims about software behavior, federated across signers, composable end-to-end, jurisdiction-neutral, and machine-checkable.
>
> **What this paper argues.** That standardization, in the form an industry-grade verification substrate requires, is not the multi-decade per-regulator amendment process the industry has assumed. It is cryptographic re-derivation. The substrate ProvekIt ships is the lift of Schneier's defining cipher identity into structured plaintext; standardization in this setting is signing a `.proof` that re-derives in an hour on a developer laptop. The institutional path remains real and is the appendix; the primary path is individual.

## Abstract

The structure of a ProvekIt `.proof` is not a design choice. It is the canonical encoding of "a chain of content-addressed, signed implications." Any system attempting to compose formal verifications across vendors, languages, and decades requires this data structure; ProvekIt is one canonical instantiation.

The substrate has two roots. The mathematical root is Cousot 1977: abstract interpretation lifted to content-addressed federation. The design root is Schneier's *Applied Cryptography* Volume 1, Chapter 1: the substrate's round-trip identity `k(k'(I)) = t` is the direct lift of the defining cipher identity `Pk(Pk'(P)) = P` into structured plaintext. The substrate IS a cipher. Trinity is plaintext, algebras are keys, lift and realize are the inverse-key-pair, CIDs are message authenticators, cross-language federation is a mode of operation.

This paper argues three claims:

1. **Cipher-substrate correspondence.** The substrate is the move that takes Applied Cryptography's Chapter 1 seriously and asks "what if the plaintext is structure." Every primitive in the substrate has a Chapter 1 analog, including AES (the C11 signature CID with its catalog of 114 op-specs) and the PKI registry (the catalog of published `.proof` files).
2. **Structural identity with the vertical stack.** A `.proof` and the vertical stack of formal verification share an identical data structure: a chain of content-addressed signed tuples of arbitrary rank. The v1.4 multi-dimensional pinning architecture (rank-3 consumer pin: `(contractCid, witnessCid, binaryCid)`) is the operational realization of this claim.
3. **Standardization as cryptographic re-derivation, via the Forgery-Equals-Production lemma.** A signed catalog `.proof` certifying that an algebra is complete at named levels L0-L3, with each level's witness re-derivable in a clean environment, is what standardization becomes. The full re-derivation cost is roughly one hour on a laptop. The lemma states that the set of algebras admitting a verifying `.proof` equals the set of algebras with the property; forging a `.proof` and producing the algebra honestly are the same activity. This is an asymmetry of kind, not of degree, in the cryptographic sense. Bitcoin's seventeen-year existence at planetary scale is the precedent. ProvekIt generalizes the mechanism from proof-of-expenditure (money) to proof-of-substance (standardization).

The per-regulator engagement paths the earlier draft of this paper enumerated remain real institutional work and are preserved as an appendix. Under v1.4's policy-as-memento architecture (paper 05 §10) and under the cost structure §9 develops, the primary path is no longer institutional. It is individual.

---

## §1. The vertical stack

Every running computation is the execution of layered abstractions. Each layer is a formal model of the layer below. Each transition between layers is a theorem: "given the assumptions of layer N and the structure of layer N-1, the higher abstraction is faithful to the lower one."

The full stack, top to bottom:

| Layer | What it abstracts | Key formal models |
|---|---|---|
| **Application contracts** | Behavioral specs on functions | Annotated source code, `.proof` files |
| **Source language semantics** | What the source code *means* | Operational semantics, type systems |
| **Compiler IR** | Optimization-preserving translation | LLVM IR semantics (Vellvm), MIR (Rust), Core (Haskell) |
| **Bytecode / object code** | Machine-loadable representation | JVM spec, BEAM spec, WASM spec, x86/ARM ISA |
| **Microarchitecture** | How the ISA actually executes | Pipeline models, cache coherence, memory models |
| **Register-transfer level (RTL)** | Synchronous digital logic | Verilog, VHDL semantics |
| **Logic gates** | Boolean operations on signals | Boolean algebra, NMOS/CMOS gate models |
| **Circuit physics** | Voltages, currents, timing | Kirchhoff's laws, SPICE-level transistor models |
| **Transistor physics** | Charge transport in MOSFETs | BSIM4, PSP, drift-diffusion equations |
| **Semiconductor physics** | Band structure, doping | Bloch theorem, density functional theory |
| **Quantum mechanics** | Atomic-scale behavior | Schrödinger equation, QED |
| **Standard Model** | Particle physics underneath QM | QED, QCD, electroweak unification |

Each row is a formal model. Each transition between rows is a theorem. Together they describe the entirety of "what happens when you run this code."

Most of this is, today, informal. The gap between "the running CPU obeys the ISA" and "the ISA's bytes correspond to the source program" is rarely formalized end to end. But each individual transition has been formalized in some setting, somewhere, by someone, and the union of those formalizations is the vertical stack of formal verification as it exists today.

The vertical stack is, structurally, a chain of theorems with antecedent and consequent. Quantum mechanics implies semiconductor physics. Semiconductor physics implies transistor behavior. Transistor behavior implies gate-level logic. Gate-level logic implies register-transfer level. RTL implies microarchitecture. Microarchitecture implies the instruction set architecture. The ISA implies the semantics of compiled bytecode. Bytecode semantics imply the source language semantics. Source semantics, plus annotations, imply behavioral contracts. Today, these implications exist in disconnected silos: HOL4 proofs of ARM, Coq proofs of CompCert, Lean proofs of cryptographic primitives, F\* proofs of TLS. None compose without ad-hoc bridging.

## §2. The cipher-substrate correspondence

The substrate has two roots. The mathematical root, named in paper 07, is Cousot 1977's abstract interpretation lifted to content-addressed federation. This section names the second root.

The defining identity of a cipher, from *Applied Cryptography* Volume 1 Chapter 1, is:

```
Pk(Pk'(P)) = P
```

Encrypt then decrypt is identity on plaintext. Decrypt then encrypt is identity on ciphertext. The two operations form an inverse-key-pair. This is the operational definition of "cipher" the discipline has worked from for thirty years.

The substrate's round-trip identity, named in paper 17 and operationalized by the asm-link-edge smoke that landed in PR #582:

```
k(k'(I)) = t
```

Where `I` is the operational term, `k'` is realize (term to source), `k` is lift (source to term), and `t` is the round-tripped term. Lift-then-realize is identity on terms. Realize-then-lift is identity on source representatives. The two operations form an inverse-key-pair.

The substrate IS a cipher.

The map, ten rows, made explicit:

| Crypto (Schneier, Chapter 1) | Substrate (ProvekIt) |
|---|---|
| Plaintext space `P` | Trinity: terms, contracts, implications |
| Key `k` | Language algebra (C11, x86-64, JVM, Rust, ...) |
| Encrypt `Pk(·)` | Lift |
| Decrypt `Pk'(·)` | Realize |
| `Pk(Pk'(P)) = P` | `k(k'(t)) = t`, round-trip closure |
| Ciphertext | Term |
| MAC / authenticator | CID (BLAKE3-512 over JCS-canonical bytes) |
| Mode of operation | Federation across languages |
| Cipher correctness | Substrate soundness |
| Symmetric vs asymmetric distinction | One-algebra closure vs cross-algebra closure |

Beyond the ten rows, the rest of *Applied Cryptography*'s Chapter 1 has substrate analogs that the table compresses. Encryption `E(P) = C` and decryption `D(C) = P` are lift and realize. Symmetric ciphers are one-algebra closures (a C11 term that lifts and realizes within C11 alone). Asymmetric ciphers are cross-algebra closures (a C11 term that round-trips through x86-64 and back). Modes of operation (CBC, CTR, GCM) are federation conventions (asm-link-edge composition, ORP serialization modes). Hash functions are content-addressing. Key management is the question of which signing keys a consumer pins. Digital signatures are envelope signatures over canonical bytes. Zero-knowledge proofs are witness mementos with hidden bodies. Protocols are the composition of all of the above into operational flows.

**The AES analog, named specifically: the C11 signature CID with its catalog of 114 op-specs.** AES is a specific cipher with specific block size, key size, and round structure, ratified by NIST after a competitive process, and from that ratification all of high-assurance cryptography descends. The C11 signature CID is the substrate's first ratified algebra: a specific operation alphabet (114 op-specs, each carrying explicit `arity_shape`), with a published catalog memento and a signed `.proof` (§5). Other algebras will follow (x86-64.proof, aarch64.proof, jvm.proof, ...); each is its own ratified cipher.

### §2.1 The thirty-year arc

The substrate is not a new project. It is the current point of a continuous thought that runs from the mid-1990s cypherpunks list to now. Each link applies one move from cryptographic discipline to a substrate that the prior era could not yet hold:

- 1995: content-addressable deduplication of file bytes by their hash. A message authenticator used as identity.
- 1998: Digital Confetti. Forward-error-corrected ciphertext under chunk-keys, plus the anti-DRM thesis that the receiver verifies, not the broadcaster.
- ShareReactor and MST3kDAP whitelists. Hash trust-anchoring made operational: MAC verification of unknown sources by reference to a curated list of known-good hashes.
- BitTorrent's file format. File-with-content-hashes-plus-FEC, the same primitives.
- 2010 Bitcoin. Proof-of-work chain over a content-addressed transaction graph, signed.
- 2026 ProvekIt. The same cryptographic discipline applied to algebraic substrates rather than to byte streams.

The thread is one move repeated at higher abstraction: identify the artifact by a computed value over its content, sign claims about the artifact, treat the substrate as authoritative for identity and let policy decide trust. Each generation could only apply this move to the kind of artifact its compute and its theory could hold. Bytes first; transactions next; structure now. The substrate is what happens when you can finally apply the move to algebraic objects.

### §2.2 The two roots

The mathematical root, Cousot 1977, is what makes the substrate sound. Abstract interpretation gives the lattice-theoretic foundation for "the contract is a lossy boundary projection of the term." Federation does not invent the lossiness; it inherits it from the predicate transformer. The substrate is Cousot 1977 plus content-addressing plus federation.

The design root, Schneier Chapter 1, is what makes the substrate recognizable. A cryptographer reading the substrate spec sees a cipher and immediately knows what to ask: what is the plaintext space, what are the keys, what are the modes, how is correctness defined, what is the MAC, how is key management handled. Every one of those questions has a direct answer in the substrate. The recognition is not metaphor. The map is exact at the data-structure level.

A reader trained in one root can pick up the other through the substrate. A cryptographer learns that the plaintext can be structure; a programming-languages researcher learns that lift and realize are an inverse-key-pair under the same discipline as encrypt and decrypt. Both arrive at the same artifact.

### §2.3 Operational consequence

Every substrate design call traces back to one of two questions. The math-root question: what does Cousot 1977 say about this lattice operation? The design-root question: what would Schneier do? The two converge. If a proposed primitive doesn't fit the cipher analogy, suspect it; if it doesn't fit the abstract-interpretation framing, suspect it harder. The substrate's primitives are the intersection of the two roots' agreement.

`source-unit(bytes, operational_term)` with a lift-witness discharge obligation, the primitive that PR #582 mints, IS the substrate's authenticated-encryption primitive. Bytes are the plaintext, `operational_term` is the structured ciphertext, the witness is the MAC that ties them. That mint is the cipher correspondence operationalized at the source-acquisition boundary. The rest of the substrate has the same shape at every layer.

## §3. Each layer is an implication

Take the transition between two adjacent layers. Concrete example: gate-level logic to register-transfer level.

The claim: "a synchronous digital circuit specified in RTL, when synthesized to gates and simulated under the same input sequence, produces the same output sequence as the RTL specification."

This is a theorem. Its antecedent is "the RTL is what we wrote." Its consequent is "the gate-level circuit behaves equivalently." Its evidence is the synthesis tool's correctness proof (or, in cases where synthesis is not formally verified, the test suite that exercises the equivalence).

This pattern is universal:

- **Bloch theorem implies semiconductor band structure**: given periodic crystal potential, electron states factor into plane waves and periodic Bloch functions.
- **BSIM4 model implies drift-diffusion behavior**: given gate voltages and operating point, the model predicts drain current within tolerances.
- **Boolean algebra implies CMOS gate behavior**: given steady-state inputs, gate output is determined by boolean function.
- **ISA semantics implies machine code execution**: given an instruction stream, the CPU's observable state evolves per the ISA spec.
- **CompCert implies C compilation correctness**: given C source, the compiled assembly preserves the source's behavior under specified semantics.
- **Hash-bounded contract implies API behavior**: given a function annotated with `@Min(0)`, the function's domain is constrained accordingly.

Every link is `(antecedent_layer ⊢ consequent_layer)` with evidence. The evidence varies (kernel-checked proof terms, peer-reviewed mathematical arguments, simulation results, formal model checking outcomes) but the structural shape is the same.

The shape: **claim X implies claim Y, here is the evidence**.

Under the cipher correspondence of §2, every such implication is a signed discharge under some algebra-key. The antecedent and consequent are CIDs (MACs) under canonical bytes; the evidence carries whatever the proof method produced; the signer is who attests. Implications federate the way ciphertext federates across modes of operation: same plaintext space, different keys, explicit re-encryption at boundaries.

## §4. The data structure of an implication

Strip the claim of domain content and look at the data structure:

```
ImplicationClaim {
    antecedentId: <stable identifier for the antecedent>,
    consequentId: <stable identifier for the consequent>,
    evidence:     <whatever the proof method produced>,
    signer:       <who is making this claim>,
    signature:    <cryptographic commitment by the signer>
}
```

Five fields. No domain content. No specifics.

This is the data structure of any chain of content-addressed signed implications. It is what every link in the vertical stack would need to look like, if the chain were to be composed at scale.

### §4.1 Rank: an implication is a rank-N tuple

A single CID is rank-1: it expresses "this content exists." An implication is rank-2 at minimum: it relates antecedent to consequent. The relations the vertical stack actually requires are higher-rank, and the protocol must transport tuples of arbitrary rank without modification.

A consumer's pin on a verified library is the canonical rank-3 tuple `(contractCid, witnessCid, binaryCid)`:

- **`contractCid`**: what the library claims (signer-independent, content-only projection).
- **`witnessCid`**: which prover attested it (signer-specific evidence chain).
- **`binaryCid`**: the bytes that are running (compiled artifact's hash).

Each axis is a different content projection (manifesto §11 in [`03-substrate-not-blockchain.md`](03-substrate-not-blockchain.md)); each catches a different attack class; the rank of the pin matches the rank of the assertion (manifesto §12).

The vertical stack's links are also rank-N tuples. A claim about gate-level logic implying RTL semantics is structurally `(rtlSpecCid, gateNetlistCid, equivalenceProofCid, signerCid)`, rank-4 at minimum. An implication from BSIM transistor models to drift-diffusion equations is `(bsimModelCid, ddEquationsCid, derivationProofCid, calibrationDataCid, signerCid)`, rank-5.

Each link in the chain has its own rank. ProvekIt's substrate transports tuples of any rank without modification. **This is the structural identity in its sharpest form: the data structure isn't just `(antecedent, consequent, evidence, signature)`; it is a tuple of arbitrary rank, where each component is a content-only projection of one axis of the assertion.**

A protocol that supports only rank-1 pins cannot transport the vertical stack. A protocol that conflates content axes with envelope-state axes (collapsing `contractCid` and `attestationCid` onto one term, the pre-v1.4 mistake) loses predicates and produces drift. ProvekIt v1.4 is the protocol naming the rank-N tuple as primitive.

### §4.2 What single-axis pinning loses

A common mistake in early content-addressing systems is to project rank-N relations onto rank-1 CIDs (the "sign the bundle file's bytes" pattern). This loses a predicate. The discarded axes leak back as drift: the bundle's hash moves on every honest re-mint because envelope state varies, and pins break that should hold.

For institutional acceptance this matters concretely. Reviewers at DO-178C, Common Criteria, ISO 26262 evaluations always ask: **how do you know the running binary corresponds to the formally verified specification?** Single-axis pinning answers "trust the signature", an answer acceptable to no high-assurance regime.

Rank-3 pinning answers: the binary's hash is checked at runtime against `binaryCid`; the contract is identified by its content-only `contractCid`; the witness chain is signed by a prover whose backend the regime accepts; all three are bound together by the consumer's own signed attestation. Each axis has a distinct adversarial model and a distinct verification mechanism. **This is the shape regulators have always asked for, expressed as content-addressed CIDs with mathematically defined composition.**

The institutional paths in Appendix A assume rank-3 pinning is the protocol's posture. Single-axis pinning would not satisfy any of the regimes named there. Multi-dimensional pinning is the substrate the institutional track needs, and it shipped in v1.4.

Some properties this data structure must have, in any deployment:

- **Stable identifiers.** The antecedent and consequent must be referenced unambiguously. A version string is not enough; an attacker (or an honest mistake) can change the version's bytes. The identifier must be the bytes themselves, content-addressed by hash.
- **Tamper-evidence.** Modifications to the antecedent, consequent, or evidence must be detectable. Cryptographic hashing achieves this.
- **Non-repudiation.** The signer must not be able to deny having claimed the implication. Digital signatures achieve this.
- **Permissionless publication.** Any party with appropriate inputs can mint such a claim; no central authority pre-approves. Content-addressing combined with signatures achieves this.
- **Composability.** Two implications can be chained: `A ⊢ B` and `B ⊢ C` give `A ⊢ C`. This composes the chain.

Every property in this list is required for the vertical stack to compose. Every property is provided by the data structure above. Every property is the substrate analog of a property the Chapter 1 cipher framework already requires of any sound primitive: identity, integrity, non-repudiation, permissionless use, compositional soundness.

## §5. Algebra Certification mementos: the catalog `.proof`

A ProvekIt `.proof` is a CBOR-encoded catalog of mementos. The earlier ontology of memento kinds enumerated three:

- **Contract memento**: `(canonicalIR, signature)`. A claim about behavior.
- **Implication memento**: `(antecedentCid, consequentCid, evidence, signature)`. A claim that one contract implies another.
- **Bridge memento**: `(sourceCid, targetCid, targetProofCid, evidence, callSiteBinding, signature)`. A claim binding an implementation symbol to a reference contract.

A fourth species is required for what standardization-as-cryptographic-re-derivation actually does: the **Algebra Certification memento**, instantiated as the catalog `.proof`.

### §5.1 What it certifies

A catalog `.proof` certifies that a language algebra (the C11 signature, the x86-64 signature, the JVM signature, ...) is *complete at named levels L0-L3* against a named, pinned corpus.

The four levels:

- **L0, structural.** Every cursor kind in the lifter's frontend dispatches to a catalog op. The dispatch table is total. The witness names every observed cursor kind and the op-CID it dispatches to. `missing` MUST be empty.
- **L1, catalog.** Every op listed in the algebra's signature memento has a published spec, and every spec carries explicit `arity_shape`. The completeness gate PR #582 added enforces this. `ops_missing_arity_shape` MUST be empty.
- **L2, algebraic closure on corpus.** Every file in the named corpus lifts without emitting "unknown_op" or "unexposed-expr" fallbacks. The corpus is pinned by `source_cid_root`, the CID of a manifest enumerating every input file's bytes. The audit query that produces the witness is itself part of the recipe. `failed`, `with_unknown_op_fallback`, and `with_unexposed_expr_fallback` MUST all be zero.
- **L3, round-trip on corpus.** For every file in the corpus, `lift(I)` and `lift(ORP_serialize(lift(I)))` produce byte-identical `{source_cid, signature_cid, term_cid}` triples. This is `k(k'(I)) = t` evaluated on every file in the corpus. The cipher-correctness test from §2, instantiated for the C11-x86-64 key pair on the linux-libkunit+net+crypto corpus. `failure_cids` MUST be empty.

L0 catches "the lifter doesn't even recognize this construct." L1 catches "the catalog has a hole." L2 catches "the catalog has a hole on real code." L3 catches "the cipher isn't a cipher", lift and realize don't actually compose to identity, the substrate is a lossy encoder pretending to be lossless.

Each level's witness is itself a content-addressed memento with a re-derivable recipe. The recipe is a small structured object: a tool name, arguments, an expected exit code, the environment variables that must be set. The verifier in `full-rederive` mode re-runs every recipe in a clean environment and compares outcomes to the witness's claimed results.

L4 (semantic completeness on all valid inputs, not just a named corpus) is undecidable mechanically and is explicitly not what the `.proof` claims. The `.proof` certifies only the levels named; each named level is itself re-derivable, no level "trusts the signer beyond identity."

### §5.2 The envelope

The signed `.proof` envelope is JCS-canonical JSON. Field order normalized, absent fields omitted, defaults JCS-omitted (matching the catalog v1 discipline):

```json
{
  "kind": "catalog-proof/v1",
  "subject": {
    "algebra_name":  "c11",
    "signature_cid": "blake3-512:a27e0770...",
    "catalog_cid":   "blake3-512:..."
  },
  "witnesses": [
    {"level": "L0", "kind": "cursor-kind-dispatch-coverage", "result_cid": "blake3-512:..."},
    {"level": "L1", "kind": "catalog-completeness-gate",     "result_cid": "blake3-512:..."},
    {"level": "L2", "kind": "corpus-algebraic-closure",      "result_cid": "blake3-512:..."},
    {"level": "L3", "kind": "corpus-roundtrip",              "result_cid": "blake3-512:..."}
  ],
  "signature": {
    "algorithm":      "ed25519",
    "public_key_cid": "blake3-512:...",
    "signed_bytes":   "<128-hex-chars>"
  },
  "provenance": {
    "git_sha":          "...",
    "git_tag":          "v1-c11-...",
    "builder_identity": "...",
    "timestamp_iso":    "...",
    "ots_proof_cid":    null
  }
}
```

The signature covers the JCS-canonical bytes of everything in the envelope except `signature.signed_bytes` itself. The verifier strips that field, recanonicalizes, verifies the Ed25519 signature against the public key resolved from `public_key_cid`. The `ots_proof_cid` field is reserved for an OpenTimestamps anchor to a Bitcoin block, giving a third-party witness for the timestamp and making "trust identity plus signature" robust against post-hoc key compromise.

The `catalog_cid` in `subject` is the CID of a separate memento bundling everything the algebra ships: every op-spec, the cursor-kind map, the index, the signature memento itself. The bundle is its own memento. Hold the bundle, you hold the whole algebra and can verify every part of it by CID equality.

### §5.3 Verifier modes

Three modes, parameterized by how much trust the consumer wants to import:

- **Mode 1, trust identity plus signature.** Read the algebra, accept it as standardized by you. Microseconds. The whole `.proof` reads in the time a TLS handshake takes. This is the everyday consumer mode.
- **Mode 2, trust identity plus signature plus verify CIDs.** Also recompute every CID from the canonical bytes it references; reject on any mismatch. Catches tampering of either the envelope or the referenced witness mementos. Still seconds.
- **Mode 3, full re-derivation.** Also re-run every recipe in a clean environment, compare outcomes to the claimed results. Catches recipe drift, environment drift, silent regressions. Hours. This is the adversarial-verifier mode.

The verifier supports all three via flags. Default is Mode 2.

The mode taxonomy is the cryptographic-trust gradient applied to standardization. The same `.proof` serves the consumer who wants to ship today (Mode 1), the auditor who wants to verify hashes (Mode 2), and the adversary who wants to confirm that the algebra actually behaves as claimed (Mode 3). All three are honored by the same artifact.

### §5.4 The catalog as PKI registry

One algebra, one `.proof`. The catalog of all `.proof` files becomes the substrate's PKI registry of standardized algebras. Each algebra's `signature_cid` is its public identity, the way `(certificate-subject, public-key)` is a TLS identity. Each algebra's catalog memento is its certificate body. Each algebra's `.proof` is the signature certifying that body.

Federation in this setting is straightforward registry inclusion. A consumer trusting `c11.proof` and `x86-64.proof` accepts artifacts in either algebra and accepts cross-algebra round-trips. A consumer trusting only `c11.proof` rejects pure-x86-64 artifacts. A jurisdiction publishing its own policy memento naming a subset of catalog `.proof`s constrains its consumers to that subset, exactly the policy-as-memento mechanism paper 05 §10 develops.

The catalog `.proof` is what makes the policy memento concrete. Paper 05 names what regulators sign; this section names what they sign about. The regulator's policy memento references a set of catalog `.proof` CIDs; consumers in that jurisdiction trust the set, and through it the algebras the set ratifies.

This is the substrate's analog of NIST publishing FIPS 197 (the AES standard) plus the validated implementations list. The `.proof` IS the FIPS publication, content-addressed, signed, byte-deterministic, re-derivable.

## §6. The 1:1 correspondence (and its boundaries)

The structural identity between a `.proof` and the vertical stack of formal verification is exact. The semantic identity is not.

**Where the identity holds:**

- The data structure of one link in the vertical stack is identical to the data structure of one ProvekIt memento.
- The composition pattern of multiple links in the vertical stack (chaining implications) is identical to the composition pattern of multiple ProvekIt mementos (DAG of bridges).
- The trust posture of one link (non-repudiation by signing, tamper-evidence by hashing) is identical to ProvekIt's trust posture.
- The deployment model (permissionless publication, content-addressed lookup, no central authority) is identical.
- The cipher-substrate correspondence of §2 holds: every primitive at every layer maps to a Chapter 1 analog.

**Where the identity does not hold:**

- The IR. ProvekIt's IR captures behavioral contracts in canonical form (a quantifier-free or first-order fragment over Int/String/Bool/Real). It does not capture quantum mechanical theories, semiconductor band structures, or microarchitectural pipeline invariants directly.
- The proof methods. ProvekIt's evidence terms are Z3 unsat cores and similar SMT outputs. The vertical stack's lower layers use very different proof methods: many-body physics simulations, SPICE simulations, model checking, theorem prover scripts.
- The TCB. ProvekIt's TCB is the protocol primitives plus configured solver backends. The vertical stack's TCB at each layer varies dramatically: Coq's kernel, Lean's kernel, HOL4, the SPICE simulator, the synthesis tool.
- The cipher analogy is structural, not operational. The substrate does not actually encrypt the trinity in the sense of producing unreadable bytes; it computes on the trinity in the open. What the cipher analogy says is that *the algebraic shape of lift and realize is identical to the algebraic shape of encrypt and decrypt*. The discipline applies; the operational secrecy does not. Cryptographers reading this paper should read "cipher" as "primitive obeying the cipher equations," not "primitive producing ciphertext as opacity."

So the 1:1 claim is precisely: at the data-structure level, the encoding ProvekIt uses for behavioral verifications is exactly the encoding required for any link in the vertical stack of formal verification. At the algebraic level, the lift-realize pair obeys the cipher identity. The protocol can transport any link's evidence; the protocol does not produce that evidence. Each layer of the stack must encode its own claims into the data structure ProvekIt provides.

This is a stronger claim than "ProvekIt is one of many possible verification protocols." It is "ProvekIt is the canonical content-addressed encoding of the vertical stack's natural composition pattern, rendered explicit." A different protocol would have to be either isomorphic (same structure, different cryptographic choices) or strictly weaker.

## §7. State of the vertical stack today

Each layer has had formal verification work. None of the layers are universally verified, and nothing connects them.

### Quantum mechanics to semiconductor physics

Largely informal. Density functional theory has rigorous foundations but is rarely used in chip design. Semiconductor manufacturers rely on empirical models calibrated to fabrication processes. The gap from first-principles physics to industry-standard transistor models is a research domain (computational materials science) rather than an engineering practice.

### Semiconductor physics to transistor models

BSIM4, PSP, and other compact models are derived from drift-diffusion equations, with calibration constants fit to manufacturing data. The derivations are rigorous in published literature but rarely formalized in a theorem prover. The models are widely used but not formally verified end to end.

### Transistor models to circuit behavior

SPICE simulators are widely deployed but not formally verified. SPICE-level simulations are the industry standard for analog and mixed-signal circuits; they produce empirically-validated results, not formally-verified ones.

### Circuit behavior to gate-level logic

Boolean algebra is fully formalized. The translation from CMOS gate networks to boolean functions has been verified for specific gate libraries (e.g., the Sail-x86 work, ARM gate-level proofs). Industry-standard logic synthesis tools (Synopsys Design Compiler, Cadence Genus) are not formally verified themselves; their outputs are validated by simulation.

### Gate-level logic to register-transfer level

RTL design languages (Verilog, VHDL) have formal semantics in academic settings. Industrial RTL is typically not formally verified against gate-level outputs; equivalence checking tools (Cadence Conformal, Synopsys Formality) provide structural proofs but rely on tool correctness.

### Register-transfer level to microarchitecture

Significant academic work. The HOL4 ARM model formalizes ARM's microarchitecture; CHERI's capability machine is formally verified; RISC-V efforts (Sail-RISC-V, Cambridge work) cover ISA-to-RTL refinement for specific cores. Industry verification is mostly proprietary and uncoordinated.

### Microarchitecture to ISA

Sail (CHERI / Cambridge) provides ISA semantics for ARM, RISC-V, MIPS, x86 (partial). The HOL4 ARM model is the most thorough; ARM v8.6+ semantics are still being filled in. x86 is partially modeled but not complete.

### ISA to machine code

x86, ARM, RISC-V machine code semantics are well-specified in ISA manuals. Sail formalizes them. Compilers (CompCert) produce machine code from C with formal correctness guarantees against the ISA spec.

### Machine code to bytecode (where applicable)

JVM bytecode semantics (formalized in Coq by various efforts), BEAM (formalized in HOL), WASM (formalized as the WebAssembly Reference Interpreter, with mechanized semantics in HOL/Coq). Each is verified independently.

### Bytecode to compiler IR to source

CompCert verifies C → Cminor → ... → assembly. CakeML verifies Standard ML → assembly. Vellvm formalizes LLVM IR semantics. None compose with the lower-level work cited above.

### Source to application contracts

ProvekIt's slice. Lift adapters promote source-level annotations to canonical IR; verifiers discharge the resulting `(post, pre)` pairs. Content-addressed and signed, currently disconnected from lower layers.

### The composition gap

Each layer has been verified in some setting. None of the verifications compose without ad-hoc bridging. A team using CompCert for C compilation cannot today inherit the HOL4 ARM model's correctness; the two efforts use different proof systems, different artifact formats, no common substrate.

The result: practical end-to-end formal verification (such as seL4's complete stack) requires bespoke engineering at every layer, with single-vendor or single-research-group control over each piece.

ProvekIt's structural claim addresses exactly this gap. If each layer's evidence were encoded as content-addressed signed mementos, composition would be automatic: chain the bridges, walk the DAG, discharge by hash equality. If each layer's algebra had a published `.proof`, the registry would catalog the stack the way TLS root stores catalog certificate authorities.

## §8. ProvekIt as the substrate

The substrate role is precise:

- ProvekIt does not replace any layer's verification framework.
- ProvekIt does not replace any layer's evidence format internally; HOL4 proof terms remain HOL4 proof terms, Coq remains Coq, SPICE remains SPICE.
- ProvekIt provides a content-addressed, signed envelope around each link's evidence.
- ProvekIt provides bridges between adjacent links, content-addressing the implication.
- ProvekIt provides composition: DAG walking, transitive verification, cache amortization.
- ProvekIt provides algebra certification via catalog `.proof` mementos (§5).

A vertical-stack-aware ProvekIt deployment would publish:

- Each layer's canonical claims as contract mementos.
- Each cross-layer implication as bridge mementos with `evidence` carrying the layer-specific proof artifact (HOL4 term, Coq term, SPICE result, etc.).
- Each language and ISA algebra at the relevant layers as catalog `.proof` files.
- A composed `.proof` for the entire stack, with `binaryCid` pinning the deployed binary at the application layer.

A consumer verifying this `.proof` would:

1. Verify the bundle's outer CID and signature.
2. Walk the DAG: contract → bridge → contract → bridge → ... down to the lowest verified layer.
3. At each step, the bridge's evidence is sufficient (per the kit's trust policy), and the relevant algebra's catalog `.proof` is in the consumer's accepted set.
4. The leaf-most claim (probably "Bloch theorem applies to silicon" or similar) is either externally trusted or recursively expanded into another ProvekIt-encoded chain.

The whole stack is one composable artifact. A consumer's verification cost is hash-bounded at every step. A change at any layer produces a new CID; the change is detected and propagates upward through the DAG.

This is what end-to-end formal verification looks like at scale, and ProvekIt's data structure is its natural transport layer.

## §9. The Forgery-Equals-Production lemma

This is the load-bearing section of the paper. The rest of the paper builds the substrate; this section names what falls out when the substrate is used to certify itself.

### §9.1 The cost gap

Traditional certification of a cryptographic primitive or a verification tool costs years and money. The numbers, drawn from public records:

- **FIPS 140 cryptographic module validation**: roughly $300,000 plus 6 to 18 months of evaluation labor at a CMVP-accredited lab, per module, per revision. Re-validation on minor changes.
- **Common Criteria EAL evaluation**: roughly $1,000,000 plus 1 to 2 years for EAL4+ products, more for higher levels. Re-evaluation on major changes.
- **AES standardization**: 5 years from NIST's 1997 call for candidates to FIPS 197 publication in 2001, during which fifteen candidate ciphers underwent open cryptanalysis by the global community.
- **PGP web-of-trust verification**: re-establishing trust relationships with N parties is O(N) human labor per consumer, multiplied by the depth of the trust graph.

ProvekIt full re-derivation of `c11.proof` (Mode 3 verification, every recipe re-run in a clean environment) on a developer laptop:

- L0 cursor-kind dispatch coverage: roughly 5 seconds.
- L1 catalog completeness gate: roughly 30 seconds.
- L2 corpus algebraic closure on linux-libkunit + net + crypto: roughly 30 minutes wall time on a 32-core machine, longer on a laptop.
- L3 corpus round-trip on the same corpus: roughly 30 minutes wall time on a 32-core machine, longer on a laptop.

Total: roughly one hour on a developer laptop, faster on a workstation. Mode 2 verification (recompute every CID without re-running recipes) takes seconds. Mode 1 verification (signature plus identity) takes microseconds.

**This is a category change, not a price reduction.** The reason follows.

### §9.2 The lemma, stated formally

**Lemma (Forgery-Equals-Production).**

Let A be a candidate algebra. Let R = (r_1, ..., r_n) be a normative tuple of recipe-functions defining "complete at level i." Let E = (e_1, ..., e_n) be the normative expected outcomes. Let φ(A) = (r_1(A), ..., r_n(A)).

Call A *complete* iff φ(A) = E.

A `.proof` for A is a triple (claim, W, σ) where:

- `claim` asserts "A is complete,"
- W = (w_1, ..., w_n) encodes each (r_i, A, claimed_outcome_i),
- σ signs the JCS-canonical bundle bytes.

The verifier V returns *valid* iff:

1. σ is a valid signature over (claim, W) under the published public key.
2. For each i, re-running r_i in the normative clean environment against A returns claimed_outcome_i.
3. For each i, claimed_outcome_i = e_i.

**Claim.** V accepts (claim, W, σ) iff φ(A) = E.

**Proof.**

(⟹) Suppose V accepts. By condition (2), each r_i(A) = claimed_outcome_i. By condition (3), each claimed_outcome_i = e_i. Therefore r_i(A) = e_i for all i, so φ(A) = E.

(⟸) Suppose φ(A) = E. Construct each witness w_i = (r_i, A, e_i); sign the bundle. Condition (1) holds by signature construction; condition (2) holds because each r_i(A) = e_i by assumption; condition (3) holds because each claimed_outcome_i was set to e_i in the construction.

**Corollary (Forgery-Equals-Production).** The set of algebras admitting a verifying `.proof` equals the set of algebras with φ(A) = E. To produce a verifying `.proof` requires producing an algebra that has the property. There is no shortcut. Forge-cost = production-cost = the cost of doing the work.

### §9.3 Preconditions

The lemma holds when:

- **P1.** Recipes are deterministic in a normatively-defined clean environment.
- **P2.** Recipes are content-addressed and part of the catalog (no silent recipe substitution).
- **P3.** Clean-environment semantics are part of the catalog (no hidden dependencies on system time, network, untracked state).
- **P4.** Cryptographic primitives (BLAKE3-512, Ed25519) hold.

P1 through P3 are catalog hygiene. P4 is the standard cryptographic assumption every signed-content-addressed system inherits. The catalog `.proof` design in §5 enforces P1 through P3 by construction: every recipe is a structured object with its tool, args, environment, and expected outcome; every recipe memento is content-addressed; the normative clean environment is named in the recipe. Drift on any of those fields produces a new recipe-CID, which produces a new witness-CID, which produces a new `.proof`-CID. The `.proof` cannot quietly point at a different recipe than the one it claims.

### §9.4 Asymmetry of kind, not of degree

Classical cryptographic primitives provide an *asymmetry of degree*. RSA: roughly 10^80 effort to forge a signature, microseconds to verify. The gap between forge-cost and verify-cost is large; both are computable numbers; the asymmetry is quantitative.

Collision-resistance for hash functions, signature unforgeability, zero-knowledge soundness: all three are asymmetries of degree. Big numbers separate honest work from cheating work; the cheating work is infeasible but conceptually exists as a distinct activity.

The `.proof` of a re-derivable property provides an *asymmetry of kind*. Forge-cost equals production-cost. There is no number for forgery, because forgery is not a distinct activity. The asymmetry is not "easy verify, hard forge"; it is "all work paths lead to the same place." Forge equals produce.

State this categorically: hash collision-resistance, signature unforgeability, ZK soundness are asymmetries of degree, all bounded by a hardness assumption that could in principle be broken with enough compute. The `.proof` of a re-derivable property is an asymmetry of kind, where breakage would not be "produce a forgery cheaper than honest production" but "produce honest production cheaper than honest production," a self-referential collapse.

This is a different category of cryptographic primitive. The substrate is the first place this primitive appears applied to standardization, but it is not the first place it appears anywhere. The next subsection names where it appears.

### §9.5 Bitcoin as the existence proof

The Forgery-Equals-Production lemma is not a novel theoretical artifact. It has a seventeen-year existence proof at planetary scale, in a different domain: Bitcoin.

Bitcoin's proof-of-work is forgery-equals-production for money. Extending the chain with a valid block requires finding a nonce such that SHA-256(header) is below the difficulty target. The verifier checks: does the hash meet the target. Whoever shows a block whose hash meets the target has done the work; there is no shortcut. Producing a chain-extension and forging a chain-extension are the same activity. The "forging" frame does not survive contact with the mechanism.

The structural mapping:

| Bitcoin | ProvekIt |
|---|---|
| Block contents (transactions) | Algebra structure (trinity) |
| PoW: find nonce s.t. SHA-256(header) < target | PoX: produce A s.t. φ(A) = E |
| Verifier: hash check (microseconds) | Verifier modes 1-3: microseconds (sig+id) / seconds (CIDs) / ~1 hour (full re-derivation) |
| Forgery: produce a valid block | Forgery: produce a valid `.proof` |
| Why forge = produce | The work IS what the verifier checks |
| Eliminates | Trusted third parties for money / for standardization |
| Consensus mechanism | Longest chain (cumulative PoW) / most-pinned CID (cumulative consumer choice) |
| Forkable | Yes / yes |
| Permissionless | Yes / yes |

Bitcoin works. The mechanism that makes it work is the mechanism the substrate generalizes.

**Proof of expenditure versus proof of substance.** Bitcoin's work is intentionally meaningless. Finding partial hash collisions has no value outside the consensus mechanism. The energy spent on PoW is by design wasted; the waste is what gives consensus its teeth. Bitcoin solved Byzantine generals by burning electricity into agreement.

ProvekIt's work is intentionally substantive. Re-running L0 to L3 recipes against an algebra produces actual algebraic facts. The compute spent on verification is useful work that incidentally also serves as the proof. The hour on a laptop is not a deliberate cost; it is the cost of actually knowing whether the algebra has the property. **The proof and the knowledge are the same artifact.**

This generalization is what distinguishes proof-of-substance from proof-of-expenditure. PoW is a special case where the substance is "I burned this much electricity"; the substrate generalizes to "I re-derived this algebraic property," and in principle to "I re-derived this property of any class." Bitcoin is one waypoint of a primitive whose target is broader.

**Longest-chain analog.** Bitcoin's consensus is the chain with most cumulative work. The canonical chain is the one the network has invested in most.

ProvekIt's standardization is the algebra-CID with most cumulative consumer pins. The canonical algebra is the one the substrate has invested in most. Same structural shape: *the canonical thing is the one the network has invested in most.* The investment vehicle differs. Bitcoin's investment vehicle is hashpower; the substrate's investment vehicle is consumer pinning via CID reference.

**Fork dynamics.** Bitcoin: Bitcoin Cash, Bitcoin SV, and other forks split the chain; the market chose which fork to value. Anyone can fork; the market sorts.

ProvekIt: `c11.proof` v1, then someone publishes `c11-prime.proof` with different `_Generic` semantics, or `c11-strict.proof` with no GCC extensions, or `c11-msvc.proof` matching Microsoft's interpretation. Consumers pin to whichever CID they trust. Anyone can mint a competing algebra; the market sorts via downstream CID references.

**Lineage.** Tonight's framing reveals a thirty-year arc that includes Bitcoin as one waypoint, not the destination. Content-addressable identity (1995) gave rise to swarmed delivery with chunk MACs (1998 Digital Confetti), which gave rise to hash-anchored trust (ShareReactor, MST3kDAP), which gave rise to BitTorrent file-format-with-FEC, which gave rise to Bitcoin PoW (2010), which gives rise to ProvekIt PoX (2026). The substrate is the latest step of a continuous thought about forgery-equals-production primitives, with progressively more abstract targets: file bytes, then transactions, then algebras.

Without naming Bitcoin explicitly, a reader would have to discover this analogy themselves. With it named, the lemma becomes obviously credible. Bitcoin works at planetary scale; the same mechanism applied to a substantive target produces a substrate that works at the scale of formal verification.

### §9.6 Audit equals certificate

A deeper consequence is worth naming explicitly.

Every classical certification regime separates the audit from the certificate. The audit produces a document (the certificate) that outlives the audited state. The certificate persists; the underlying state evolves. Two attack surfaces follow:

- **(a) Forgery.** Produce a certificate without performing the audit. Defended by: auditor accreditation, signed certificates, registries of authorized auditors. Classical PKI handles this.
- **(b) Obsolescence.** The audited state changes after the certificate issues; the certificate stops being true. Defended by: re-certification cadence, registry expiration, revocation lists. Operationally expensive, frequently neglected.

Classical defenses handle (a) and (b) separately. They require distinct institutional machinery and they generate distinct kinds of failures (forged certificates vs. stale certificates).

The `.proof` collapses both into one problem. If A changes, φ(A) changes, witnesses become invalid, V rejects. The certificate IS the audit. There is no "stale certificate" state in the substrate. A `.proof` is either currently-valid or currently-invalid; there is no past tense for certification.

This is what eliminates the institutional standardization edifice. Auditor markets exist because the audit-versus-certificate distinction creates space for institutions to bridge them. When forgery equals production AND obsolescence equals forgery, there is no bridge to maintain. The mechanism that prevents forgery is the same mechanism that prevents obsolescence, and both are mechanical properties of the verifier, not properties of an institution.

### §9.7 What the lemma does NOT defend against

Naming the lemma's scope protects it from being misread as load-bearing for the wrong things. Three attack classes remain, none of which the lemma defends against:

- **Spec attacks.** Recipes that don't capture what they claim. If `r_2` is named "L2 corpus algebraic closure" but actually tests something weaker, the `.proof` certifies the weaker property while claiming the stronger one. This is an editorial defense, not a cryptographic one. The catalog maintainer publishes recipes; reviewers challenge; v2 corrects. Sound recipes are an ecosystem property.
- **Cryptographic failure.** A BLAKE3-512 collision or an Ed25519 break. The lemma's preconditions P4 fails. Standard cryptographic agility applies: the catalog re-mints under different primitives, old `.proof` files are migrated, the protocol catalog version bumps. The substrate inherits every assumption Chapter 1 ciphers inherit.
- **Definition disputes.** Parties disagree about what "L0-L3 complete" should mean. One faction publishes a `.proof` claiming completeness against one definition; another publishes a `.proof` claiming completeness against a different definition. Both are valid under their own definitions. The market converges via which catalog CIDs consumers pin to. This is the PKI-analog at the consumer layer, the same mechanism that lets browser vendors disagree about which CAs to trust without breaking TLS itself.

These are ecosystem properties, not failures of the lemma. The lemma defends one thing precisely: that given a normative R and E, the set of algebras admitting a verifying `.proof` is exactly the set of algebras with φ(A) = E. Whether R and E are the right normative choices, whether the cryptographic primitives hold, and whether the editorial community agrees on definitions are separate questions answered by separate mechanisms.

### §9.8 Fork dynamics as the selection mechanism

The classical concern about permissionless standardization is "fragmentation will cause chaos." Under forgery-equals-production with content-addressed pinning, that concern inverts. Fragmentation becomes the selection mechanism.

Multiple competing algebras coexist. `c11.proof` v1, `c11-prime.proof` with different `_Generic` semantics, `c11-strict.proof` with no GCC extensions, `c11-msvc.proof` matching Microsoft's interpretation, possibly more. Consumers pin via CID to whichever algebra they want operating under. Interoperability is solved AT the CID layer: every consumer knows exactly which algebra applies to which artifact. The market sorts via cumulative pinning. Algebras that meet consumer needs accumulate references and become canonical; algebras that don't, die from lack of pins. Standards bodies become publishers, not gatekeepers. Bad algebras die from market disinterest, not from authority revocation.

This is the move from prescriptive standardization (the body declares the standard) to descriptive standardization (the market evolved to pin this CID). It is the same dynamic Bitcoin demonstrated: BCH and BSV exist alongside BTC; the market chose BTC by accumulated hashpower and consumer acceptance; nobody had to outlaw the alternatives. Diversity of certified algebras is to algebra-standardization what biodiversity is to ecosystems: the substrate from which evolution happens, not a failure mode.

The substrate's posture is not apologetic about fragmentation. Fragmentation under forgery-equals-production is healthy market behavior and entirely desirable. It is the mechanism that keeps the substrate live rather than ossified, that lets better algebras displace worse ones without coup, that lets jurisdictional differences and technical disagreements coexist without procedural deadlock.

### §9.9 What this unlocks

Five consequences follow from the lemma plus the fork dynamics:

**Anyone can be a verifier.** A `.proof` file plus a developer laptop is the entire infrastructure. No accreditation, no lab membership, no nation-state recognition. The verifier reads the proof in seconds (Mode 1), checks hashes in seconds (Mode 2), or re-runs every recipe in an hour (Mode 3). The verifier needs only the public artifacts and the substrate runtime.

**Anyone can be an adversary.** This is the strongest verification model. An adversary who wants to attack `c11.proof` runs Mode 3 verification and looks for any recipe that produces a different outcome than the witness claimed. If every recipe re-derives correctly, the adversary has failed; the `.proof` is sound. If any recipe drifts, the adversary has found a defect that the holder cannot deny. The substrate inverts the usual cryptographic-attack economics: defenders pay once to produce the algebra, adversaries pay one hour to verify it, the asymmetric productive labor was already done.

**Anyone can fork.** Per §9.8. Forks are the selection mechanism, not the failure mode.

**Bit-rot detection is automatic.** Re-run the recipes quarterly, or on demand, or after any change to the substrate runtime. Drift fails the witness. The same `.proof` that certifies the algebra today will fail Mode 3 verification in the future if the substrate has changed in a way that breaks round-trip or algebraic closure on the named corpus. The proof of correctness IS the regression suite. Obsolescence equals forgery; the substrate handles both with one mechanism (§9.6).

**Standards bodies become individuals.** "Sir-as-NIST" is a working model. The architect signs `c11.proof`; the substrate runtime verifies it; consumers pin to its CID; the algebra is ratified. The institutional path the appendix preserves is one way to get there, and it remains the right path for jurisdictions whose statutory requirements are written against named bodies (FAA, EASA, ENISA, BSI). The individual path is the other way, and it is what the substrate actually shipped first. The two paths coexist; they do not compete.

The cost of trust collapses from years to one hour. The collapse is not in the engineering work, which is still hard; the collapse is in the institutional certification work, which is replaced. What used to require a CMVP lab now requires a recipe and a laptop. What used to require five years of NIST process now requires the substrate engineering plus a signing key.

This is what standardization looks like when the substrate is a cipher and the lemma is the load-bearing property.

## §10. What the protocol must do to fully enable this

The protocol must satisfy several requirements that current v1.x ProvekIt does not yet fully satisfy. These are the engineering items between today and the cost-structure claim of §9 being true across the full vertical stack:

### TCB minimization

- Constructive-proof backends (Coq, Lean, F\*) shipping. Today: only Z3.
- Multi-backend concurrence as a configurable requirement. Today: supported in spec, no configured deployments.
- Per-kit constructive-proof verification chains documented. Today: not documented.

### Algebra certification across the stack

- `c11.proof` ships first, as the bootstrap. Today: design done (PRs #582 and #583), `.proof` build pending.
- `x86-64.proof` and `aarch64.proof` follow; the same machinery applies. Today: catalog work in flight via the foo-morphism and asm-link-edge.
- `jvm.proof`, `wasm.proof`, `mir.proof` (Rust), `core.proof` (Haskell) over time. The bedrock thesis from paper 14 says each is a finite engineering project.
- Each algebra published with its own re-derivation recipes; the substrate provides the verifier, not the algebra.

### Conformance and interoperability

- Cross-vendor interoperability test suites. Today: implicit in the conformance harness.
- Multi-implementation parity testing (Rust ↔ Coq ↔ HOL4 evidence interchange). Today: not implemented.
- Standard test vectors for fundamental claims. Today: limited.

### Formal semantics for the IR

- A formalized semantics for the canonical IR, in a kernel-checked theorem prover. Today: informal CDDL grammar plus JCS canonicalization.
- Soundness theorems for the canonicalization process. Today: empirical (conformance fixtures).
- Soundness theorems for the bridge composition. Today: argued informally, not formalized.

### Reference contracts library

- A curated set of reference contracts covering the major call-site categories (parsing, validation, arithmetic, cryptographic primitives). Today: stubs.
- Each reference contract formally verified and signed by a quorum of authorities. Today: not yet.
- Bridge anchor maintenance practices documented. Today: documented in this paper, not yet operational.

### Documentation for verifiers

- Mode 3 walkthroughs for the first published `.proof`s, so consumers can rehearse full re-derivation on hardware they control. Today: c11.proof recipes drafted (`/tmp/pk-catalog-proof/`), pending PR.
- OpenTimestamps integration for `.proof` envelopes, anchoring timestamps to a Bitcoin block. Today: reserved field, integration pending.
- Revocation list machinery (`revocation_list_cid` field in `.proof` v2). Today: schema reserved, v1 single-sig only.

These are 18 to 36 months of engineering work, in parallel with the substrate-level evolution. None is research; all is engineering. The path is well-defined.

## §11. Counterarguments

A complete paper engages the most plausible counterarguments. Here are six.

### "ProvekIt is for behavioral contracts; the vertical stack is much broader."

True at the IR level. But the data structure (content-addressed signed implications) is universal, and the cipher correspondence holds at every layer. A different protocol would either be isomorphic or strictly weaker. The argument for ProvekIt-the-instantiation is pragmatic; the argument for ProvekIt-the-data-structure is structural.

### "Why not just use Coq / Lean for the whole stack?"

You could. Several research groups have. The result is a single-vendor stack with no cross-language interchange, no cross-vendor interoperability, no path to industry adoption beyond a single research community. The vertical stack composes within Coq, but Coq's authority is the kernel; the kernel does not federate. ProvekIt's data structure federates because the substrate is a cipher and ciphers federate by mode of operation.

### "But fragmentation will cause chaos. Standards-body authority prevents incompatibility."

This counterargument is well-founded for classical standardization regimes. ISO, IEEE, W3C, RTCA, and the rest exist precisely because uncoordinated standards cause market failure when consumers can't tell which standard a producer used. The historical evidence is real: the browser wars, the early VHS-versus-Betamax era, the multiplicity of incompatible USB-C cables, all show what happens when no body declares a single answer.

CID-pinning eliminates that confusion at the substrate layer.

Every artifact is signed under a specific algebra-CID. The consumer's pin specifies which CID the consumer accepts. Two producers using different algebras simply produce artifacts under different CIDs; a consumer trying to compose them sees the CID mismatch immediately. This is interoperability *by structural rejection at the boundary*, which is stronger than interoperability *by enforced uniformity*. Bitcoin and Ethereum coexist without confusion because every transaction is signed under its protocol's rules; nobody confuses an ETH transfer for a BTC transfer. Same shape for algebras.

The deeper point: classical regimes confuse three things, uniformity, interoperability, and quality. They achieve all three by mandating one standard. The substrate decouples them:

- **Uniformity is consumer-chosen.** Pin one CID and operate under one algebra.
- **Interoperability is structural.** CIDs prevent silent confusion across algebra boundaries; mismatches surface at compose time, not at runtime.
- **Quality emerges from market selection.** Most-pinned algebras are the ones that earned pins; algebras that broke or were displaced lose pins.

The result is more interoperable AND higher-quality than the regime-mandated approach, because algebras must continuously earn their pins or be displaced. Diversity of certified algebras is the substrate's evolutionary mechanism, not its failure mode. Fragmentation under forgery-equals-production is healthy market behavior and entirely desirable.

The substrate inherits the PKI shape exactly: anyone can publish, anyone can decline to trust, trust is policy not protocol, forgery means producing the artifact not faking the signature, recognition is by inclusion in a trusted set. What converges is what consumers actually trust. The market sorts.

### "Standards bodies are slow and politicized; this won't happen institutionally."

The institutional path is the appendix, not the spine. The §9 cost-structure path runs in parallel and is the primary one. The 20-year horizon for full institutional integration assumes standards bodies move at their historical pace; that horizon is preserved in Appendix A. The 1-year horizon for `c11.proof` plus a published policy memento is what the substrate actually delivers.

Where institutional adoption is statutorily required (FAA type certification, EASA, CMVP for FIPS 140 cryptographic modules), the appendix paths apply. Where it is not, individual standardization via `.proof` plus consumer pin is sufficient and is faster.

### "The TCB of ProvekIt + Z3 is much larger than Coq's kernel."

True for Z3. The path forward is constructive-proof backends. Configured ProvekIt-with-Coq has the same TCB as Coq; the protocol layer adds a small auditable surface (BLAKE3, Ed25519, JCS). The `.proof` envelope structure is itself small enough to be hand-audited.

### "Hash collisions could break everything."

BLAKE3-512 is collision-resistant to the best of current cryptographic knowledge. A collision attack would break ProvekIt's content-addressing for the affected pair. Mitigations: cryptographic agility (the protocol catalog is itself versioned; a future version could use a different hash function; old `.proof` files would be migrated). This is the same risk every cryptographic protocol carries; it is not specific to ProvekIt. The cipher correspondence of §2 makes this explicit: the substrate inherits every assumption Chapter 1 ciphers inherit, including hash collision resistance.

## §12. Conclusion

A `.proof` and a chain of formally verified software from quantum physics to bytecode are 1:1 identical at the data-structure level. The data structure is `(antecedentCid, consequentCid, evidence, signature)`, composed via DAG. Each layer of the vertical stack is a chain of such tuples, and the substrate ProvekIt ships is the canonical content-addressed encoding.

The data-structure claim is one third of the story. The cipher correspondence is the second third: the substrate is the move that takes Schneier's *Applied Cryptography* Volume 1 Chapter 1 seriously and asks what happens when the plaintext is structure rather than bytes. The trinity is structured plaintext. Lift and realize are the inverse-key-pair. CIDs are message authenticators. Cross-language federation is a mode of operation.

The Forgery-Equals-Production lemma is the third third, the load-bearing one. Standardization in this setting is signing a `.proof` that certifies an algebra is closed at named levels against a named corpus, with every claim re-derivable in a clean environment in one hour on a laptop. The lemma says the set of algebras admitting a verifying `.proof` is exactly the set of algebras with the property; forge-cost equals production-cost. This is an asymmetry of kind, not of degree. Bitcoin demonstrated the mechanism for seventeen years applied to money; the substrate generalizes it to algebras. The audit IS the certificate; obsolescence equals forgery; the same mechanism prevents both.

The institutional path is real and is the appendix: per-regulator engagement, multi-year horizons, the substrate-engineering work paper 05 §10 still requires. That work proceeds on its own track and gets to its own conclusion in the 10-to-15-year horizon.

The individual path is the spine: a signed catalog `.proof` certifying that an algebra is complete at L0-L3 against a published corpus. The first such `.proof` is `c11.proof`, dependent on PR #582 (catalog mint and source-unit) and PR #583 (walk-c actuals) and pending build. The substrate's PKI registry of standardized algebras is the catalog of all such `.proof`s. Each is one ratified cipher. Fragmentation is the selection mechanism.

Whoever invests in this work invests in the only protocol that can compose the chain. The chain has been built piece by piece for sixty years; the substrate has been waiting; the cipher discipline that makes the substrate recognizable has been on the shelf since 1994; the proof-of-expenditure existence proof has been running since 2010. Now the generalization is here.

Cost of trust collapses from years to one hour. Standards bodies become individuals. Federation is a mode of operation. The substrate is a cipher, the lemma is its correctness test, and the catalog of `.proof`s is its registry.

## References

- Schneier, Bruce. *Applied Cryptography: Protocols, Algorithms, and Source Code in C*, Volume 1, Chapter 1. John Wiley & Sons, 1994. (The defining cipher identity `Pk(Pk'(P)) = P`.)
- Cousot, Patrick and Cousot, Radhia. "Abstract Interpretation: A Unified Lattice Model for Static Analysis of Programs by Construction or Approximation of Fixpoints." POPL 1977. (The math root of the substrate.)
- Schrödinger equation and quantum mechanics: standard physics references (Griffiths, Sakurai).
- Density functional theory: Kohn-Sham 1965; Engel-Dreizler review.
- BSIM4 transistor model: Cheng-Hu, "MOSFET Modeling & BSIM3 User's Guide."
- SPICE: Nagel, "SPICE2: A Computer Program to Simulate Semiconductor Circuits."
- Boolean algebra and gate-level: Knuth, "Art of Computer Programming Vol 4A, §7.1.1."
- HOL4 ARM model: Fox, "Formal Verification of the ARMv8-A Instruction Set Architecture."
- Sail / RISC-V / x86: Gray et al., "Sail: ISA Semantics."
- CompCert: Leroy, "Formal Verification of a Realistic Compiler."
- CakeML: Kumar et al., "CakeML: A Verified Implementation of ML."
- Vellvm: Zhao et al., "Formalizing the LLVM Intermediate Representation for Verified Program Transformations."
- seL4: Klein et al., "seL4: Formal Verification of an OS Kernel."
- DO-178C / DO-333: RTCA / EUROCAE.
- Common Criteria: ISO/IEC 15408.
- ISO 26262: International Organization for Standardization.
- IEC 62304 / 61508: International Electrotechnical Commission.
- NIST SP 800-218: NIST.
- SLSA: OpenSSF.
- EU Cyber Resilience Act: European Commission.
- ProvekIt catalog `.proof` design draft: `docs/specs/catalog-proof-v1.md` (pending PR; current draft at `/tmp/pk-catalog-proof/DESIGN.md`).
- Paper 05, §10: jurisdictional policy as memento.
- Paper 14, §1: the `.proof` file as the universal correctness bundle.
- Paper 17: every programming language is a dialect, and the substrate names the references.

## Read next

- [`05-witness-pluralism-and-jurisdiction-neutral-transport.md`](05-witness-pluralism-and-jurisdiction-neutral-transport.md): §10 develops the policy-as-memento mechanism that pairs with §9 here.
- [`14-after-trust-the-universal-correctness-bundle.md`](14-after-trust-the-universal-correctness-bundle.md): the `.proof` file's role as the universal deliverable.
- [`17-after-babel-we-speak-in-vectors-now.md`](17-after-babel-we-speak-in-vectors-now.md): the address space the catalog `.proof` registers into.
- [`../explanation/thesis.md`](../explanation/thesis.md): the central claim of the protocol.
- [`../explanation/cross-domain-verification.md`](../explanation/cross-domain-verification.md): the bridge mechanism.
- [`../explanation/boundaries.md`](../explanation/boundaries.md): the explicit non-claims.
- [`../security/threat-model.md`](../security/threat-model.md): what the protocol catches and what it does not.
- [`../contributing/proposing-a-spec-change.md`](../contributing/proposing-a-spec-change.md): adding new IR primitives to capture more of the vertical stack.

---

# Appendix A: Institutional acceptance pathways

> The per-regulator engagement paths preserved from the earlier draft of this paper. The §9 cost-structure path is the spine; this appendix is the parallel institutional track for jurisdictions whose statutory requirements name regulatory bodies.
>
> **Note on this appendix.** The per-regulator paths below assume substrate-level amendment as the engagement work for each regime (RTCA amends DO-333, ISO TC 22 amends 26262, etc.). [Paper 05 §10](05-witness-pluralism-and-jurisdiction-neutral-transport.md#10-standardization-restructure) supersedes that framing under v1.4's policy-as-memento architecture: each regulator publishes a content-addressed policy memento under its authority key, and consumers pin to the policy CID. The substrate-level engineering work in §10 of this paper remains; the per-regulator engagement collapses from 10-15 year amendment horizons to 1-2 year publication horizons. And §9 above develops the further point that the individual standardization path (a signed catalog `.proof` plus consumer pin) is the primary path; the institutional path below is preserved for jurisdictions whose statutory requirements are written against named bodies.

No standard today fully accepts hash-bounded verification as equivalent to ITP-checked proofs as a matter of bare regulation. This is not a fundamental obstacle; it is a process problem. Standards bodies move on multi-year cycles; recognition of new methods follows demonstration of equivalent assurance plus working-group consensus.

This appendix maps the landscape and the specific path for each major standard.

## A.1 The standards that govern formal-verification acceptance

| Standard | Domain | Formal-methods acceptance |
|---|---|---|
| **DO-178C / DO-333** | Avionics software | DO-333 supplement explicitly covers formal methods; tool qualification (DO-330) requires verified verification tools |
| **Common Criteria** | IT security | EAL5+ requires formal verification; ITSEF labs assess; CC Recognition Arrangement (CCRA) coordinates 31 nations |
| **ISO 26262** | Automotive | ASIL-D requires formal methods (Part 6 §10); annex C lists accepted techniques |
| **IEC 61508** | Functional safety (general) | SIL 3-4 require formal verification |
| **IEC 62304** | Medical device software | Risk-stratified; Class C requires highest assurance |
| **FDA SaMD** | Software as a Medical Device | Recognizes IEC 62304; FDA-specific guidance evolving |
| **FedRAMP / FIPS 140-3** | Federal cloud / cryptographic modules | FIPS 140-3 requires formal modeling for higher levels |
| **NIST SP 800-218** | Secure Software Development Framework | Aligns with SLSA, in-toto; one of NIST's reference models |
| **EU Cyber Resilience Act** | EU consumer product cybersecurity | Coming into force 2027; "appropriate verification" required |

Each has a different gatekeeping process. Each takes years to update. Together they govern the markets where formal verification is required by regulation: aerospace, automotive, medical devices, federal procurement, cryptographic modules, and (under CRA) consumer products in the EU.

## A.2 The road for DO-178C

DO-178C is the current FAA / EASA-accepted standard for avionics software. DO-333 ("Formal Methods Supplement to DO-178C") explicitly enables formal methods to satisfy verification objectives. Tool qualification per DO-330 governs which verification tools are accepted.

### Where ProvekIt currently stands relative to DO-178C

- **DO-333 §FM.6.7.b**: requires that formal methods be "based on mathematical models and have a well-defined syntax and semantics, including operations." ProvekIt's IR plus canonical form plus handshake satisfies this requirement.
- **DO-333 §FM.6.7.c**: requires that formal methods be sound: "if the analysis claims a property holds, it does." ProvekIt's soundness rests on the configured backend's soundness; with a constructive-proof backend (Coq, Lean), DO-333 soundness is achieved.
- **DO-330**: requires that any formal-verification tool used be qualified at the appropriate Tool Qualification Level (TQL). For DAL A software (the highest), TQL-1 verification tools are required.

ProvekIt's specific gaps:

1. **No TQL-1 qualification kit exists.** A TQL-1 kit requires hundreds of pages of evidence: requirements, design, traceability, test results, hazard analysis. Currently no party has invested in producing this for a ProvekIt implementation.
2. **The Z3 backend is not TQL-1 qualified.** A constructive-proof backend (Coq) would need to be the qualified Tier 3 backend for DAL A software.
3. **Cross-language transfer** is not addressed by DO-178C explicitly. The standard assumes a single development environment.

### The path

**Phase 1 (years 0-3): demonstration and engagement.**

- Produce a TQL-2 reference implementation (lower assurance level; faster to qualify) of a ProvekIt-based verification flow for a non-critical avionics component.
- Demonstrate the flow to RTCA Working Group SC-205 / EUROCAE WG-71 (the joint group that maintains DO-178C and its supplements).
- Engage with AVSI (Aerospace Vehicle Systems Institute) consortium members; they coordinate industry-academia collaboration on avionics tooling.
- Publish papers in DASC (Digital Avionics Systems Conference) and FM (Formal Methods) covering the demonstration.

**Phase 2 (years 3-7): standards-track work.**

- Propose a "Hash-Bounded Verification" annex to DO-333 or a new supplement (DO-XXX), capturing:
  - The protocol's data structure.
  - The conformance harness's role as integrity check.
  - The TCB analysis for different backend choices.
  - Mapping to existing DO-333 verification objectives.
- Draft the proposal in coordination with SC-205 / WG-71 members.
- Submit through the formal process; iterate based on feedback.

**Phase 3 (years 7-12): tool qualification and adoption.**

- Produce a TQL-1 qualification kit for a specific ProvekIt implementation (likely the Rust kit plus a configured Coq backend).
- Submit to certification authorities (FAA AIR, EASA) for type certification approval.
- Pioneer adopters (likely Airbus or Boeing in cooperation with academic partners) deploy the qualified flow on a non-critical system.

**Phase 4 (years 12+): mainstream avionics adoption.**

- Subsequent revisions of DO-178C / DO-333 fold in ProvekIt's data structure as standard.
- Industry tooling (LDRA, Polyspace, Coverity competitors) integrates ProvekIt support.

This is a 10-15 year roadmap. Avionics moves slowly because the cost of a wrong call is catastrophic. The roadmap is feasible; equivalent timelines have been observed for prior verification methods (model checking to DO-333 acceptance took roughly 15 years from research demonstration to standard-supplement acceptance).

## A.3 The road for Common Criteria

Common Criteria ISO/IEC 15408 governs IT security product evaluation. EAL5 ("Semiformally Designed and Tested") requires "semiformal" methods; EAL6 ("Semiformally Verified Design and Tested") requires formal methods at the design level; EAL7 ("Formally Verified Design and Tested") requires full formal verification.

The CC Recognition Arrangement (CCRA) coordinates 31 nations' acceptance of CC certificates. Updates to CC happen through an international working group; major version revisions take roughly 5 years.

### Where ProvekIt currently stands relative to CC

- **EAL5+ formal methods requirements**: CC explicitly accepts formal methods including model checking, theorem proving, and refinement. ProvekIt's data structure transports any of these.
- **EAL6 / EAL7 formal verification**: ProvekIt with constructive-proof backends (Coq, Lean) reaches the assurance level CC requires.
- **Protection Profile (PP) evolution**: PPs define security requirements for product categories. Updating PPs to recognize ProvekIt's content-addressed format is the practical path.

### The path

**Phase 1 (years 0-3): research and demonstration.**

- Produce a complete EAL6 evaluation case study for a small component (e.g., a cryptographic primitive), using ProvekIt to compose the formal proofs.
- Engage with national schemes (BSI in Germany, CCN in France, NIAP in the US) to get informal feedback.
- Publish the evaluation methodology in an academic venue (e.g., LOPSTR, FM, ITP) to build community recognition.

**Phase 2 (years 3-7): PP work and tool integration.**

- Draft a Protection Profile (PP) for a product class using ProvekIt-backed verification (e.g., a smart-card OS, an HSM, or a TEE).
- Submit to a national scheme for evaluation.
- Coordinate with CC technical communities to integrate ProvekIt-formatted evidence into evaluation artifacts.

**Phase 3 (years 7-10): CC revision integration.**

- The next CC revision (post-CC 2022) is an opportunity to integrate hash-bounded verification as an accepted method. Engagement with the CCMC (Common Criteria Management Committee) and CCDB (CC Development Board) drives this.
- Working with CCRA member nations: the protocol must be acceptable to all 31 to gain mutual recognition status.

**Phase 4 (years 10+): product certifications.**

- High-assurance products (HSMs, smart cards, secure enclaves, government-grade encryption) ship with ProvekIt-formatted formal verification artifacts as part of their CC submissions.

The CC path is faster than DO-178C in the protocol-acceptance phase but slower in the recognition-of-tools phase. Realistic horizon: 10 years for full integration.

## A.4 The road for ISO 26262

ISO 26262 governs functional safety in automotive electronics. ASIL-D (the highest assurance level) requires formal methods per Part 6 §10.4.5. The standard's annexes list accepted methods; updating the annex to recognize ProvekIt is the practical path.

### Where ProvekIt currently stands relative to ISO 26262

- **Part 6 Annex C**: lists accepted verification techniques. Includes "formal verification (formal proof of correctness)" as a recommended technique for ASIL-D.
- **Part 8 Clause 11**: covers tool qualification. Requires evidence that verification tools are reliable.
- **Adoption pace**: ISO 26262 is updated every roughly 5-7 years; the next revision (post-2018 second edition) is in flight.

### The path

**Phase 1 (years 0-3): industry pilot.**

- Partner with an automotive OEM (e.g., Bosch, Continental, Mercedes) on a pilot project using ProvekIt for ASIL-D-relevant code.
- Demonstrate the flow on a non-critical ECU (e.g., an infotainment subsystem or a non-functional-safety component).
- Publish results in IEEE Trans. on Computer-Aided Design or Automotive Software Engineering venues.

**Phase 2 (years 3-7): standards engagement.**

- Engage ISO/TC 22/SC 32/WG 8 (the ISO 26262 working group) with formal proposal to update Annex C.
- Work with national standards bodies (DIN, AFNOR, BSI) for input.
- Publish the proposal as a Technical Report (TR) to give the industry visibility before the standard update.

**Phase 3 (years 7-10): standard revision.**

- Inclusion of hash-bounded verification in the next ISO 26262 revision (likely 2026-2030 timeframe based on prior cadence).
- Tool qualification methodology standardized.

**Phase 4 (years 10+): industry rollout.**

- ASIL-D components in production vehicles increasingly use ProvekIt-formatted verification artifacts.

Total horizon: 10 years. The automotive industry moves faster than aerospace because product cycles are shorter and the regulatory pressure is more immediate (autonomous driving safety cases, electric powertrain functional safety).

## A.5 The road for FDA / FedRAMP / IEC

These standards govern medical devices, federal cloud services, and general functional safety. Each has different paths.

### FDA (medical devices)

- **510(k) submissions** for medium-risk devices; **PMA (Premarket Approval)** for high-risk devices.
- IEC 62304 governs medical device software lifecycle.
- FDA's Center for Devices and Radiological Health (CDRH) recognizes IEC 62304 as a consensus standard.
- The FDA's pre-submission consultation process (Q-Sub) lets vendors discuss new methodologies before formal submission.

**Path:**

- Phase 1 (years 0-3): partner with a medical device vendor (e.g., a pacemaker manufacturer) on a pilot using ProvekIt for PMA-required formal verification.
- Phase 2 (years 3-7): submit as a Q-Sub; engage CDRH on hash-bounded verification.
- Phase 3 (years 7-10): IEC 62304 revision (ISO/IEC JTC1/SC42 process) integrates ProvekIt-style verification artifacts.

Realistic horizon: 7-10 years from start of pilot to integration.

### FedRAMP

- FedRAMP authorizes cloud services for US federal use.
- High baseline requires FIPS 140-3 cryptographic module validation.
- FIPS 140-3 (NIST 800-140) requires formal modeling at higher security levels.
- Coordinated through CMVP (Cryptographic Module Validation Program).

**Path:**

- Phase 1 (years 0-3): a cryptographic library (e.g., HACL\* successor, or an HSM firmware project) ships with ProvekIt-formatted formal proofs.
- Phase 2 (years 3-5): CMVP recognizes ProvekIt-formatted artifacts as acceptable formal modeling evidence for FIPS 140-3 Level 3-4.
- Phase 3 (years 5-7): FedRAMP authorizations increasingly cite ProvekIt-backed cryptographic modules.

Faster than the aviation/automotive path because cryptography is already well-formalized; the protocol layer is the easier add.

### IEC 61508

General functional safety. SIL 3-4 require formal verification. The standard is updated infrequently (last major revision 2010); incremental updates via amendments are more common.

**Path:**

- Phase 1: industry pilots in railway, process industries, nuclear (each has its own SIL regime built on IEC 61508).
- Phase 2: amendments to recognize hash-bounded verification.
- Phase 3: industry-specific standards (CENELEC for rail, IEC 61511 for process) integrate ProvekIt format.

### NIST SP 800-218 / SLSA / Cyber Resilience Act

NIST's Secure Software Development Framework (SSDF) and SLSA are aligned and reference each other. The EU Cyber Resilience Act (CRA) coming into force 2027 references existing standards.

**Path:**

- Phase 1 (years 0-2): work with OpenSSF and SLSA maintainers to add ProvekIt-formatted attestation as a recognized SLSA Level 4 mechanism.
- Phase 2 (years 2-4): NIST 800-218 revision references ProvekIt-style behavioral attestations.
- Phase 3 (years 4-6): EU ENISA guidance for CRA Article 13 (essential cybersecurity requirements) recognizes ProvekIt-backed verification for high-risk products.

This is the fastest institutional path: SSDF and SLSA move on shorter cycles than aviation/automotive standards. Realistic horizon: 5 years to mainstream recognition.

## A.6 Industry adoption (parallel to institutional standardization)

Standards lag industry. Industry adopts when the value is clear, then standards catch up. ProvekIt's industry-adoption path is:

1. **Academic and research adoption.** Publications, conference talks, workshops. Build credibility.
2. **Open-source adoption.** Major open-source projects publishing `.proof` files alongside packages. Build a substrate.
3. **Commercial adoption (low-stakes).** SaaS products, internal tools at tech companies adopting for internal CI gates. Build a market.
4. **Commercial adoption (high-stakes).** Cryptographic libraries, security-critical infrastructure adopting. Build assurance evidence.
5. **Regulatory and standards adoption.** Following demonstrated value at scale.

This sequence is faster than institutional standardization. A 5-year industry-adoption ramp can produce enough usage data to make standards-track work meaningful.

## A.7 Timeline summary

Aggregating the per-standard analyses:

| Horizon | Achievable by |
|---|---|
| **3 years** | Research adoption, first commercial pilots, initial OpenSSF / SLSA recognition |
| **5 years** | NIST SSDF / EU CRA recognition; industry-standard `.proof` distribution alongside major open-source packages |
| **7 years** | First-pilot regulatory acceptance: FedRAMP / FIPS 140-3 for cryptographic modules; ISO 26262 amendment in flight |
| **10 years** | Common Criteria PP integration; ISO 26262 next-revision integration; automotive ASIL-D production use |
| **12 years** | DO-178C / DO-333 supplement integration; first avionics use |
| **15 years** | Mainstream regulatory acceptance across all major standards; ProvekIt-equivalent acceptance with ITP-backed kits at the highest assurance levels |
| **20 years** | The vertical stack of formal verification has a content-addressed substrate (this protocol or a successor); composition of formal verifications is industry standard for high-assurance domains |

This is realistic, not aspirational, for the institutional track. Each milestone follows demonstrated work, not expectations. The 20-year horizon for full vertical-stack institutional adoption matches the historical pace of formal-methods adoption (Hoare logic to mainstream contract programming: 30 years; Coq / F\* / Lean to industry deployment: 25 years; ITP-checked compilers to commercial deployment: 20 years from CompCert).

The §9 individual-standardization path runs orthogonally and reaches first `.proof` ratification in months, not decades.
