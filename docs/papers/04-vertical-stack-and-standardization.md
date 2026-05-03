# The vertical stack, and the road to standardization

> A `.proof` and a chain of formally verified software from quantum physics to bytecode and everything in between are 1:1 identical. This paper unpacks why, and what it takes to get there.

## Abstract

The structure of a ProvekIt `.proof` is not a design choice. It is the canonical encoding of "a chain of content-addressed, signed implications." Any system attempting to compose formal verifications across vendors, languages, and decades requires this data structure; ProvekIt is one canonical instantiation.

The vertical stack of formal verification — from the Schrödinger equation to application bytecode — is a chain of mathematical implications. Quantum mechanics implies semiconductor physics. Semiconductor physics implies transistor behavior. Transistor behavior implies gate-level logic. Gate-level logic implies register-transfer level. RTL implies microarchitecture. Microarchitecture implies the instruction set architecture. The ISA implies the semantics of compiled bytecode. Bytecode semantics imply the source language semantics. Source semantics, plus annotations, imply behavioral contracts.

At every link, there is a theorem with antecedent and consequent. At every link, the theorem is in principle content-addressable, signable, and reusable. Today, these implications exist in disconnected silos: HOL4 proofs of ARM, Coq proofs of CompCert, Lean proofs of cryptographic primitives, F\* proofs of TLS. None of these compose without ad-hoc bridging. Together they describe a stack from physics to application, but no current protocol composes them automatically.

This paper argues two claims:

1. **Structural identity.** A `.proof` and the vertical stack of formal verification share an identical data structure: a chain of content-addressed signed tuples of arbitrary rank. Each link is `(antecedentCid, consequentCid, evidenceCid, signerCid, ...)` projected through content-only dimensions per the rank required by the assertion. ProvekIt's protocol is not novel; it is the canonical content-addressed encoding of formal verification's natural composition pattern, rendered explicit. The v1.4 multi-dimensional pinning architecture (rank-3 consumer pin: `(contractCid, witnessCid, binaryCid)`) is the operational realization of this claim.

2. **Standardization roadmap.** No existing standard (DO-178C, Common Criteria EAL5+, ISO 26262, FDA SaMD, FedRAMP, IEC 62304) accepts hash-bounded verification as equivalent to ITP-checked proofs. This paper traces the road from "currently unrecognized" to "explicitly accepted," through the working groups, recognition arrangements, and harmonization processes that govern each standard. The rank-3 pinning posture is the substrate this work needs; reviewers at every regime ask "how do you know the running binary corresponds to the formally verified specification?" and rank-3 is the answer.

Together, these claims position ProvekIt not as a new tool, but as the missing transport layer for an industry that has been generating formal verifications for sixty years without a common substrate to compose them.

---

## 1. The vertical stack

Every running computation is the execution of layered abstractions. Each layer is a formal model of the layer below. Each transition between layers is a theorem: "given the assumptions of layer N and the structure of layer N-1, the higher abstraction is faithful to the lower one."

The full stack, top to bottom:

| Layer | What it abstracts | Key formal models |
|---|---|---|
| **Application contracts** | Behavioral specs on functions | Annotated source code, `.proof` files |
| **Source language semantics** | What the source code *means* | Operational semantics; type systems |
| **Compiler IR** | Optimization-preserving translation | LLVM IR semantics (Vellvm), MIR (Rust), Core (Haskell) |
| **Bytecode / object code** | Machine-loadable representation | JVM spec, BEAM spec, WASM spec, x86/ARM ISA |
| **Microarchitecture** | How the ISA actually executes | Pipeline models, cache coherence, memory models |
| **Register-transfer level (RTL)** | Synchronous digital logic | Verilog, VHDL semantics |
| **Logic gates** | Boolean operations on signals | Boolean algebra, NMOS/CMOS gate models |
| **Circuit physics** | Voltages, currents, timing | Kirchhoff's laws, SPICE-level transistor models |
| **Transistor physics** | Charge transport in MOSFETs | BSIM4, PSP; drift-diffusion equations |
| **Semiconductor physics** | Band structure, doping | Bloch theorem, density functional theory |
| **Quantum mechanics** | Atomic-scale behavior | Schrödinger equation, QED |
| **Standard Model** | Particle physics underneath QM | QED, QCD, electroweak unification |

Each row is a formal model. Each transition between rows is a theorem. Together they describe the entirety of "what happens when you run this code."

Most of this is, today, informal. The gap between "the running CPU obeys the ISA" and "the ISA's bytes correspond to the source program" is rarely formalized end to end. But each individual transition has been formalized in some setting, somewhere, by someone — and the union of those formalizations is the vertical stack of formal verification as it exists today.

## 2. Each layer is an implication

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

Every link is `(antecedent_layer ⊢ consequent_layer)` with evidence. The evidence varies — kernel-checked proof terms, peer-reviewed mathematical arguments, simulation results, formal model checking outcomes — but the structural shape is the same.

The shape: **claim X implies claim Y, here is the evidence**.

## 3. The data structure of an implication

Strip the claim of domain content and look at the data structure:

```
ImplicationClaim {
    antecedentId: <stable identifier for the antecedent>,
    consequentId: <stable identifier for the consequent>,
    evidence: <whatever the proof method produced>,
    signer: <who is making this claim>,
    signature: <cryptographic commitment by the signer>
}
```

Five fields. No domain content. No specifics.

This is the data structure of any chain of content-addressed signed implications. It is what every link in the vertical stack would need to look like, if the chain were to be composed at scale.

### 3.1 Rank: an implication is a rank-N tuple

A single CID is rank-1: it expresses "this content exists." An implication is rank-2 at minimum: it relates antecedent to consequent. The relations the vertical stack actually requires are higher-rank, and the protocol must transport tuples of arbitrary rank without modification.

A consumer's pin on a verified library is the canonical rank-3 tuple `(contractCid, witnessCid, binaryCid)`:

- **`contractCid`**: what the library claims (signer-independent, content-only projection).
- **`witnessCid`**: which prover attested it (signer-specific evidence chain).
- **`binaryCid`**: the bytes that are running (compiled artifact's hash).

Each axis is a different content projection (manifesto §11 in [`03-substrate-not-blockchain.md`](03-substrate-not-blockchain.md)); each catches a different attack class; the rank of the pin matches the rank of the assertion (manifesto §12).

The vertical stack's links are also rank-N tuples. A claim about gate-level logic implying RTL semantics is structurally `(rtlSpecCid, gateNetlistCid, equivalenceProofCid, signerCid)` — rank-4 at minimum. An implication from BSIM transistor models to drift-diffusion equations is `(bsimModelCid, ddEquationsCid, derivationProofCid, calibrationDataCid, signerCid)` — rank-5.

Each link in the chain has its own rank. ProvekIt's substrate transports tuples of any rank without modification. **This is the structural identity in its sharpest form: the data structure isn't just `(antecedent, consequent, evidence, signature)` — it is a tuple of arbitrary rank, where each component is a content-only projection of one axis of the assertion.**

A protocol that supports only rank-1 pins cannot transport the vertical stack. A protocol that conflates content axes with envelope-state axes (collapsing `contractCid` and `attestationCid` onto one term, the pre-v1.4 mistake) loses predicates and produces drift. ProvekIt v1.4 is the protocol naming the rank-N tuple as primitive.

### 3.2 What single-axis pinning loses, and why this matters for standardization

A common mistake in early content-addressing systems is to project rank-N relations onto rank-1 CIDs (the "sign the bundle file's bytes" pattern). This loses a predicate. The discarded axes leak back as drift: the bundle's hash moves on every honest re-mint because envelope state varies, and pins break that should hold.

For standardization, this matters concretely. Reviewers at DO-178C, Common Criteria, ISO 26262 evaluations always ask: **how do you know the running binary corresponds to the formally verified specification?** Single-axis pinning answers "trust the signature" — an answer acceptable to no high-assurance regime.

Rank-3 pinning answers: the binary's hash is checked at runtime against `binaryCid`; the contract is identified by its content-only `contractCid`; the witness chain is signed by a prover whose backend the regime accepts; all three are bound together by the consumer's own signed attestation. Each axis has a distinct adversarial model and a distinct verification mechanism. **This is the shape regulators have always asked for, expressed as content-addressed CIDs with mathematically-defined composition.**

The standardization argument in §9–§14 below assumes rank-3 pinning is the protocol's posture. Single-axis pinning would not satisfy any of the regimes in scope. Multi-dimensional pinning is the substrate the standards-track work needs — and it shipped in v1.4.

Some properties this data structure must have, in any deployment:

- **Stable identifiers.** The antecedent and consequent must be referenced unambiguously. A version string is not enough; an attacker (or an honest mistake) can change the version's bytes. The identifier must be the bytes themselves, content-addressed by hash.
- **Tamper-evidence.** Modifications to the antecedent, consequent, or evidence must be detectable. Cryptographic hashing achieves this.
- **Non-repudiation.** The signer must not be able to deny having claimed the implication. Digital signatures achieve this.
- **Permissionless publication.** Any party with appropriate inputs can mint such a claim; no central authority pre-approves. Content-addressing combined with signatures achieves this.
- **Composability.** Two implications can be chained: `A ⊢ B` and `B ⊢ C` give `A ⊢ C`. This composes the chain.

Every property in this list is required for the vertical stack to compose. Every property is provided by the data structure above.

## 4. ProvekIt's `.proof` is that data structure

A ProvekIt `.proof` is a CBOR-encoded catalog of mementos. Each memento is one of:

- **Contract memento**: `(canonicalIR, signature)`. A claim about behavior.
- **Implication memento**: `(antecedentCid, consequentCid, evidence, signature)`. A claim that one contract implies another.
- **Bridge memento**: `(sourceCid, targetCid, targetProofCid, evidence, callSiteBinding, signature)`. A claim binding an implementation symbol to a reference contract.

The implication memento and the bridge memento are precisely the data structure from §3, with kit-specific framing. The contract memento is the leaf: a claim with no antecedent, just a signed canonical content.

A `.proof` bundle composes these into a directed acyclic graph: contracts at the leaves, implications and bridges as edges, the bundle's outer CID as the root. Content-addressing all the way down.

The map from generic implication to ProvekIt:

| Generic implication property | ProvekIt mechanism |
|---|---|
| Stable identifier | BLAKE3-512 CID of canonical bytes |
| Tamper-evidence | CID mismatch detected on fetch |
| Non-repudiation | Ed25519 signature over canonical bytes |
| Permissionless publication | No central authority; mint and publish anywhere |
| Composability | DAG of mementos; bridges chain across `.proof` files |

ProvekIt is one canonical instantiation. Other instantiations are possible — a different hash function, a different signature scheme, a different canonicalization rule. The protocol's choices (BLAKE3-512, Ed25519, JCS) are pragmatic; another instantiation with different choices would have the same structural shape and would interoperate with ProvekIt only via translation layers.

The relevant claim is not "ProvekIt's specific choices are mandatory." It is "the data structure ProvekIt uses is the data structure any content-addressed verification protocol must use."

## 5. The 1:1 correspondence (and its boundaries)

The structural identity is exact. The semantic identity is not.

**Where the identity holds:**

- The data structure of one link in the vertical stack is identical to the data structure of one ProvekIt memento.
- The composition pattern of multiple links in the vertical stack (chaining implications) is identical to the composition pattern of multiple ProvekIt mementos (DAG of bridges).
- The trust posture of one link (non-repudiation by signing, tamper-evidence by hashing) is identical to ProvekIt's trust posture.
- The deployment model (permissionless publication, content-addressed lookup, no central authority) is identical.

**Where the identity does not hold:**

- The IR. ProvekIt's IR captures behavioral contracts in canonical form (a quantifier-free or first-order fragment over Int/String/Bool/Real). It does not capture quantum mechanical theories, semiconductor band structures, or microarchitectural pipeline invariants directly.
- The proof methods. ProvekIt's evidence terms are Z3 unsat cores and similar SMT outputs. The vertical stack's lower layers use very different proof methods: many-body physics simulations, SPICE simulations, model checking, theorem prover scripts.
- The TCB. ProvekIt's TCB is the protocol primitives plus configured solver backends. The vertical stack's TCB at each layer varies dramatically: Coq's kernel, Lean's kernel, HOL4, the SPICE simulator, the synthesis tool.

So the 1:1 claim is precisely: at the data-structure level, the encoding ProvekIt uses for behavioral verifications is exactly the encoding required for any link in the vertical stack of formal verification. The protocol can transport any link's evidence. The protocol does not produce that evidence. Each layer of the stack must encode its own claims into the data structure ProvekIt provides.

This is a stronger claim than "ProvekIt is one of many possible verification protocols." It is "ProvekIt is the canonical content-addressed encoding of the vertical stack's natural composition pattern, rendered explicit." A different protocol would have to be either isomorphic (same structure, different cryptographic choices) or strictly weaker.

## 6. State of the vertical stack today

Each layer has had formal verification work. None of the layers are universally verified, and nothing connects them.

### Quantum mechanics → semiconductor physics

Largely informal. Density functional theory has rigorous foundations but is rarely used in chip design. Semiconductor manufacturers rely on empirical models calibrated to fabrication processes. The gap from first-principles physics to industry-standard transistor models is a research domain (computational materials science) rather than an engineering practice.

### Semiconductor physics → transistor models

BSIM4, PSP, and other compact models are derived from drift-diffusion equations, with calibration constants fit to manufacturing data. The derivations are rigorous in published literature but rarely formalized in a theorem prover. The models are widely used but not formally verified end to end.

### Transistor models → circuit behavior

SPICE simulators are widely deployed but not formally verified. SPICE-level simulations are the industry standard for analog and mixed-signal circuits; they produce empirically-validated results, not formally-verified ones.

### Circuit behavior → gate-level logic

Boolean algebra is fully formalized. The translation from CMOS gate networks to boolean functions has been verified for specific gate libraries (e.g., the Sail-x86 work, ARM gate-level proofs). Industry-standard logic synthesis tools (Synopsys Design Compiler, Cadence Genus) are not formally verified themselves; their outputs are validated by simulation.

### Gate-level logic → register-transfer level

RTL design languages (Verilog, VHDL) have formal semantics in academic settings. Industrial RTL is typically not formally verified against gate-level outputs; equivalence checking tools (Cadence Conformal, Synopsys Formality) provide structural proofs but rely on tool correctness.

### Register-transfer level → microarchitecture

Significant academic work. The HOL4 ARM model formalizes ARM's microarchitecture; CHERI's capability machine is formally verified; RISC-V efforts (Sail-RISC-V, Cambridge work) cover ISA-to-RTL refinement for specific cores. Industry verification is mostly proprietary and uncoordinated.

### Microarchitecture → ISA

Sail (CHERI/Cambridge) provides ISA semantics for ARM, RISC-V, MIPS, x86 (partial). The HOL4 ARM model is the most thorough; ARM v8.6+ semantics are still being filled in. x86 is partially modeled but not complete.

### ISA → machine code

x86, ARM, RISC-V machine code semantics are well-specified in ISA manuals. Sail formalizes them. Compilers (CompCert) produce machine code from C with formal correctness guarantees against the ISA spec.

### Machine code → bytecode (where applicable)

JVM bytecode semantics (formalized in Coq by various efforts), BEAM (formalized in HOL), WASM (formalized as the WebAssembly Reference Interpreter, with mechanized semantics in HOL/Coq). Each is verified independently.

### Bytecode → compiler IR / source

CompCert verifies C → Cminor → ... → assembly. CakeML verifies Standard ML → assembly. Vellvm formalizes LLVM IR semantics. None compose with the lower-level work cited above.

### Source → application contracts

ProvekIt's slice. Lift adapters promote source-level annotations to canonical IR; verifiers discharge the resulting `(post, pre)` pairs. Content-addressed and signed, but currently disconnected from lower layers.

### The composition gap

Each layer has been verified in some setting. None of the verifications compose without ad-hoc bridging. A team using CompCert for C compilation cannot today inherit the HOL4 ARM model's correctness; the two efforts use different proof systems, different artifact formats, no common substrate.

The result: practical end-to-end formal verification (such as seL4's complete stack) requires bespoke engineering at every layer, with single-vendor or single-research-group control over each piece.

ProvekIt's structural claim addresses exactly this gap. If each layer's evidence were encoded as content-addressed signed mementos, composition would be automatic: chain the bridges, walk the DAG, discharge by hash equality.

## 7. ProvekIt as the substrate

The substrate role is precise:

- ProvekIt does not replace any layer's verification framework.
- ProvekIt does not replace any layer's evidence format internally; HOL4 proof terms remain HOL4 proof terms, Coq remains Coq, SPICE remains SPICE.
- ProvekIt provides a content-addressed, signed envelope around each link's evidence.
- ProvekIt provides bridges between adjacent links, content-addressing the implication.
- ProvekIt provides composition: DAG walking, transitive verification, cache amortization.

A vertical-stack-aware ProvekIt deployment would publish:

- Each layer's canonical claims as contract mementos.
- Each cross-layer implication as bridge mementos with `evidence` carrying the layer-specific proof artifact (HOL4 term, Coq term, SPICE result, etc.).
- A composed `.proof` for the entire stack, with `binaryCid` pinning the deployed binary at the application layer.

A consumer verifying this `.proof` would:

1. Verify the bundle's outer CID and signature.
2. Walk the DAG: contract → bridge → contract → bridge → ... down to the lowest verified layer.
3. At each step, the bridge's evidence is sufficient (per the kit's trust policy).
4. The leaf-most claim (probably "Bloch theorem applies to silicon" or similar) is either externally trusted or recursively expanded into another ProvekIt-encoded chain.

The whole stack is one composable artifact. A consumer's verification cost is hash-bounded at every step. A change at any layer produces a new CID; the change is detected and propagates upward through the DAG.

This is what end-to-end formal verification looks like at scale, and ProvekIt's data structure is its natural transport layer.

## 8. Standardization: the landscape

No standard today accepts hash-bounded verification as equivalent to ITP-checked proofs. This is not a fundamental obstacle; it is a process problem. Standards bodies move on multi-year cycles; recognition of new methods follows demonstration of equivalent assurance plus working-group consensus.

This section maps the standardization landscape and the specific path for each major standard.

> **Note on §9-§14 below.** The per-regulator paths in §9-§14 assume substrate-level amendment as the engagement work for each regime (RTCA amends DO-333, ISO TC 22 amends 26262, etc.). [Paper 05 §10](05-witness-pluralism-and-jurisdiction-neutral-transport.md#10-standardization-restructure) supersedes that framing under v1.4's policy-as-memento architecture: each regulator publishes a content-addressed policy memento under its authority key, and consumers pin to the policy CID. The substrate-level engineering work in §15 below remains; the per-regulator engagement collapses from 10-15 year amendment horizons to 1-2 year publication horizons. Read §9-§14 for the substrate-engineering paths; read paper 05 §10 plus Corollary 4.5.5 for the regulator-level reframe.

### Standards that govern formal-verification acceptance

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

ProvekIt's path to standardization is not one path; it is N paths, one per standard.

## 9. The road for DO-178C

DO-178C is the current FAA / EASA-accepted standard for avionics software. DO-333 ("Formal Methods Supplement to DO-178C") explicitly enables formal methods to satisfy verification objectives. Tool qualification per DO-330 governs which verification tools are accepted.

### Where ProvekIt currently stands relative to DO-178C

- **DO-333 §FM.6.7.b**: requires that formal methods be "based on mathematical models and have a well-defined syntax and semantics, including operations." ProvekIt's IR + canonical form + handshake satisfy this requirement.
- **DO-333 §FM.6.7.c**: requires that formal methods be sound — "if the analysis claims a property holds, it does." ProvekIt's soundness rests on the configured backend's soundness; with a constructive-proof backend (Coq, Lean), DO-333 soundness is achieved.
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

- Produce a TQL-1 qualification kit for a specific ProvekIt implementation (likely the Rust kit + a configured Coq backend).
- Submit to certification authorities (FAA AIR, EASA) for type certification approval.
- Pioneer adopters (likely Airbus or Boeing in cooperation with academic partners) deploy the qualified flow on a non-critical system.

**Phase 4 (years 12+): mainstream avionics adoption.**

- Subsequent revisions of DO-178C / DO-333 fold in ProvekIt's data structure as standard.
- Industry tooling (LDRA, Polyspace, Coverity competitors) integrates ProvekIt support.

This is a 10-15 year roadmap. Avionics moves slowly because the cost of a wrong call is catastrophic. The roadmap is feasible; equivalent timelines have been observed for prior verification methods (model checking → DO-333 acceptance took ~15 years from research demonstration to standard-supplement acceptance).

## 10. The road for Common Criteria

Common Criteria ISO/IEC 15408 governs IT security product evaluation. EAL5 ("Semiformally Designed and Tested") requires "semiformal" methods; EAL6 ("Semiformally Verified Design and Tested") requires formal methods at the design level; EAL7 ("Formally Verified Design and Tested") requires full formal verification.

The CC Recognition Arrangement (CCRA) coordinates 31 nations' acceptance of CC certificates. Updates to CC happen through an international working group; major version revisions take ~5 years.

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

## 11. The road for ISO 26262

ISO 26262 governs functional safety in automotive electronics. ASIL-D (the highest assurance level) requires formal methods per Part 6 §10.4.5. The standard's annexes list accepted methods; updating the annex to recognize ProvekIt is the practical path.

### Where ProvekIt currently stands relative to ISO 26262

- **Part 6 Annex C**: lists accepted verification techniques. Includes "formal verification (formal proof of correctness)" as a recommended technique for ASIL-D.
- **Part 8 Clause 11**: covers tool qualification. Requires evidence that verification tools are reliable.
- **Adoption pace**: ISO 26262 is updated every ~5-7 years; the next revision (post-2018 second edition) is in flight.

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

## 12. The road for FDA / FedRAMP / IEC

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

This is the fastest path: SSDF and SLSA move on shorter cycles than aviation/automotive standards. Realistic horizon: 5 years to mainstream recognition.

## 13. Industry adoption (parallel to standardization)

Standards lag industry. Industry adopts when the value is clear, then standards catch up. ProvekIt's industry-adoption path is:

1. **Academic and research adoption.** Publications, conference talks, workshops. Build credibility.
2. **Open-source adoption.** Major open-source projects publishing `.proof` files alongside packages. Build a substrate.
3. **Commercial adoption (low-stakes).** SaaS products, internal tools at tech companies adopting for internal CI gates. Build a market.
4. **Commercial adoption (high-stakes).** Cryptographic libraries, security-critical infrastructure adopting. Build assurance evidence.
5. **Regulatory and standards adoption.** Following demonstrated value at scale.

This sequence is faster than standardization. A 5-year industry-adoption ramp can produce enough usage data to make standards-track work meaningful.

## 14. Timeline summary

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

This is realistic, not aspirational. Each milestone follows demonstrated work, not expectations. The 20-year horizon for full vertical-stack composition matches the historical pace of formal-methods adoption (Hoare logic to mainstream contract programming: 30 years; Coq/F\*/Lean to industry deployment: 25 years; ITP-checked compilers to commercial deployment: 20 years from CompCert).

## 15. What the protocol must do to enable standardization

The protocol must satisfy several requirements that current v1.x ProvekIt does not yet fully satisfy. These are the engineering items between today and standards-track acceptance:

### TCB minimization

- Constructive-proof backends (Coq, Lean, F\*) shipping. Today: only Z3.
- Multi-backend concurrence as a configurable requirement. Today: supported in spec, no configured deployments.
- Per-kit constructive-proof verification chains documented. Today: not documented.

### Tool qualification kits

- TQL-1 / TQL-2 evidence packages for ProvekIt implementations. Today: not produced.
- Per-implementation safety analysis. Today: ad-hoc.
- Coverage rubrics formalized for tool qualification. Today: informal.

### Conformance and interoperability

- Cross-vendor interoperability test suites. Today: implicit in the conformance harness.
- Multi-implementation parity testing (Rust ↔ Coq ↔ HOL4 evidence interchange). Today: not implemented.
- Standard test vectors for fundamental claims. Today: limited.

### Formal semantics for the IR

- A formalized semantics for the canonical IR, in a kernel-checked theorem prover. Today: informal CDDL grammar + JCS canonicalization.
- Soundness theorems for the canonicalization process. Today: empirical (conformance fixtures).
- Soundness theorems for the bridge composition. Today: argued informally; not formalized.

### Reference contracts library

- A curated set of reference contracts covering the major call-site categories (parsing, validation, arithmetic, cryptographic primitives). Today: stubs.
- Each reference contract formally verified and signed by a quorum of authorities. Today: not yet.
- Bridge anchor maintenance practices documented. Today: documented in this paper, not yet operational.

### Documentation for standards bodies

- Mapping documents linking ProvekIt's data structure to specific standards' verification objectives. Today: not produced.
- Whitepapers explaining the protocol's TCB to assessors. Today: this paper is a start; per-standard versions would be needed.

These are 18-36 months of engineering work, in parallel with standards-track engagement. None is research; all is engineering. The path is well-defined.

## 16. Counterarguments and caveats

A complete paper engages the most plausible counterarguments. Here are five.

### "ProvekIt is for behavioral contracts; the vertical stack is much broader."

True at the IR level. But the data structure (content-addressed signed implications) is universal. A different protocol would either be isomorphic or strictly weaker. The argument for ProvekIt-the-instantiation is pragmatic; the argument for ProvekIt-the-data-structure is structural.

### "Why not just use Coq / Lean for the whole stack?"

You could. Several research groups have. The result is a single-vendor stack with no cross-language interchange, no cross-vendor interoperability, no path to industry adoption beyond a single research community. The vertical stack composes within Coq, but Coq's authority is the kernel; the kernel does not federate. ProvekIt's data structure federates.

### "Standards bodies are slow and politicized; this won't happen."

The 20-year horizon assumes standards bodies are slow. A faster path exists if industry adoption precedes standardization (cryptographic library deployment of `.proof` files at scale would force industry-standard adoption ahead of regulatory recognition).

### "The TCB of ProvekIt + Z3 is much larger than Coq's kernel."

True for Z3. The path forward is constructive-proof backends. Configured ProvekIt-with-Coq has the same TCB as Coq; the protocol layer adds a small auditable surface (BLAKE3, Ed25519, JCS).

### "Hash collisions could break everything."

BLAKE3-512 is collision-resistant to the best of current cryptographic knowledge. A collision attack would break ProvekIt's content-addressing for the affected pair. Mitigations: cryptographic agility (the protocol catalog is itself versioned; a future version could use a different hash function; old `.proof` files would be migrated). This is the same risk every cryptographic protocol carries; it's not specific to ProvekIt.

## 17. Conclusion

A `.proof` and a chain of formally verified software from quantum physics to bytecode are 1:1 identical at the data-structure level. The data structure is `(antecedentCid, consequentCid, evidence, signature)`, composed via DAG.

Each layer of the vertical stack — quantum mechanics, semiconductor physics, transistor models, gate-level logic, microarchitecture, ISA, machine code, bytecode, compiler IR, source semantics, application contracts — is a chain of such tuples. ProvekIt provides the canonical content-addressed encoding.

The protocol does not produce verifications. It transports them. It composes them. It distributes them. It makes them auditable end to end, mathematically rather than heuristically.

The road to standardization is well-defined: per-standard engagement on multi-year cycles, with TCB minimization and tool qualification work paralleling the standards-track engagement. The 5-year horizon achieves NIST / SLSA / EU CRA recognition; the 10-year horizon achieves Common Criteria and ISO 26262; the 15-year horizon achieves DO-178C and broad regulatory acceptance.

This is the work of a generation. It is also the work that no other protocol has positioned itself to do. The structural identity between `.proof` and the vertical stack is not an accident; it is the canonical encoding of what content-addressed verification at scale requires. ProvekIt is the form, and the vertical stack is the substance.

Whoever invests in this work — in the engineering, in the standards engagement, in the industry pilots — invests in the only protocol that can compose the chain. The chain has been built piece by piece for sixty years; the substrate has been waiting. Now it is here.

## References

(For a full paper, this section would cite specific works at each layer. A representative sketch:)

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

## Read next

- [`../explanation/thesis.md`](../explanation/thesis.md) — the central claim of the protocol.
- [`../explanation/cross-domain-verification.md`](../explanation/cross-domain-verification.md) — the bridge mechanism.
- [`../explanation/boundaries.md`](../explanation/boundaries.md) — the explicit non-claims.
- [`../security/threat-model.md`](../security/threat-model.md) — what the protocol catches and what it does not.
- [`../contributing/proposing-a-spec-change.md`](../contributing/proposing-a-spec-change.md) — adding new IR primitives to capture more of the vertical stack.
