# After Trust: The Universal Correctness Bundle

> **Status.** Sustained argument. Contains six lemmas with proof sketches. Written to be cite-able.
>
> **Companion to.** [04 Vertical Stack](04-vertical-stack-and-standardization.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md), [07 After Verification](07-after-verification-bug-classes-as-missing-edges.md), [09 Lossy Boundary Compression](09-lossy-boundary-compression.md), [12 After Languages](12-after-languages-how-proofir-represents-every-language.md), [13 After Grammars](13-after-grammars-programming-languages-as-content-addressed-algebras.md).
>
> **Premise the earlier papers established.** The substrate is a content-addressed federation of canonical predicates, signed implication edges, witness receipts, language-level mementos, and algebraic morphisms. Paper 04 established that a `.proof` and the vertical stack of formal verification are the same data structure. Paper 06 moved trust from reputation to signed claims. Paper 07 moved verification from isolated tools to a federated proof DAG. Paper 09 explained why ProofIR is universal because it forgets everything outside the contract boundary. Paper 12 made lifter algorithms content-addressed. Paper 13 made programming languages themselves content-addressed algebras.
>
> **What this paper argues.** That the point of the substrate is the `.proof` file. Everything else is the factory. The `.proof` file is the universal correctness bundle: a constant-size, locally-verifiable claim bundle whose verifier reduces to CID recomputation, byte comparison, and signature checking. Once the bedrock C and C++ runtime layer is ingested, once package registries lift as supply graphs, and once Bridge obligations collapse to a small set of runtime calling conventions, the substrate reaches a finite set of plateau axes. At that point `.proof` replaces trust in authorities with local verification for any domain that can express its claims as content-addressed predicates.

## §0: Why this paper exists

The earlier papers established the pieces. This paper names the deliverable.

The substrate is not a better static analyzer. It is not a package manager with proofs attached. It is not an LSP, an AMP, a registry crawler, a C lifter, or a replacement for one theorem prover with another theorem prover. Those are production machinery. The artifact the rest of civilization can consume is smaller and harder: a `.proof` file.

A `.proof` file says: this artifact's behavior is bounded by these receipts, computed without trusting any party other than the math, recomputable locally in constant time, valid across any transport pipe, valid across any jurisdiction, time-stable for as long as BLAKE3, Ed25519, and the accepted prove portfolio remain unbroken.

That sentence is the whole move. It is also the reason the ingestion strategy matters. A `.proof` is only useful against a populated catalog. Without the catalog, a proof for a Node application degenerates into refusal receipts: we do not know what V8 does, we do not know what libuv does, we do not know what OpenSSL does, we do not know what the native module does. With the catalog populated, the same Node application composes through V8, libuv, OpenSSL, its native bindings, and its user-level contracts into one root claim. The bundle can be shipped in a Docker label, carried through an air gap, attached to a regulatory filing, or pinned by a consumer policy. The verifier does not care where it came from.

The trust model changes shape. The old model asks who said the artifact was acceptable. The new model asks whether the local machine can recompute the CIDs and verify the signatures. Auditors, regulators, vendors, reviewers, and registries still exist as social institutions. They stop being the load-bearing source of truth. Their claims become mementos like anyone else's claims.

This is after trust in the authority sense. It is not after policy, governance, liability, or human judgment. It is after the proposition that correctness must ultimately rest on someone else's institutional standing.

## §1: The .proof file, plainly

A `.proof` file is a compact bundle of content-addressed references:

- a claim CID;
- contract memento CIDs;
- composition memento CIDs;
- leaf-discharge receipt CIDs;
- signatures over the relevant envelopes;
- refusal receipt CIDs where the substrate cannot discharge a claim.

The verifier algorithm is deliberately boring. Recompute the CIDs from canonical bytes. Compare them to the CIDs carried in the bundle. Verify the signatures. Apply the local witness policy. Accept, reject, or report the explicit refusals.

The important operation is `memcmp`. The verifier is not interpreting the moral meaning of the claim. It is not trusting the person who handed it the file. It is not reopening the whole source tree and re-running every solver by default. It is checking that the bundle's content-addressed edges are exactly the edges the bundle claims they are, that the signatures bind the claims to keys, and that the local policy accepts those keys and witness classes.

For ordinary acceptance against a locally populated catalog, the bundle reads in microseconds. The work is not theorem proving at read time. The work is hash recomputation, byte comparison, signature verification, and policy lookup.

The substrate is opaque to meaning. It sees CIDs, signatures, receipts, and composition rules. Domain-specificity lives in the lifter that produced the predicates and the prove portfolio that discharged them. Both are mementos. The same `.proof` shape can carry a C function's bounds contract, a Verilog module's timing invariant, a Catala-encoded legal clause, a Rego policy obligation, or a ledger-state invariant. The verifier does not need to become a lawyer, a hardware engineer, or a kernel maintainer. It only needs canonical bytes, cryptographic identities, and accepted receipts.

This opacity is not a weakness. It is why the data structure can be universal. A verifier that understands one domain is an expert system. A verifier that checks content-addressed receipts is infrastructure.

The objection is predictable: correctness is not just signatures and hashes. Correct. Correctness is in the predicate and the discharge. The `.proof` file does not make a false predicate true. It does not make a weak contract strong. It does not turn an unsound solver into a sound one. It gives every claim a stable address, every discharge a checkable receipt, every refusal an explicit witness, and every consumer a local acceptance function. That is the piece the old trust stack lacks.

## §2: The bedrock thesis

The runtime layer of essentially every deployed software system on Earth is C or C++ source on disk.

Not "compiles down to C-equivalent." Not "has C-like semantics." Literally C or C++ source files in version control.

The Linux kernel is C. glibc and musl are C. Node.js is C++ under the JavaScript surface the application author sees. V8, the JavaScript engine inside Node and Chrome, is C++. CPython is C. PHP's Zend engine is C. Ruby's MRI implementation is C. OpenJDK HotSpot is C++. .NET CoreCLR is C++. Erlang BEAM is C. GHC's runtime system is C. Lua, Perl, and Tcl are C. PostgreSQL, MySQL, Redis, SQLite, nginx, Apache httpd, OpenSSL, libuv, libcurl, ffmpeg, ImageMagick, git, bash, zsh, and tmux are C.

This is not a metaphor about "low-level software." It is a catalog fact. The interpreter, VM, runtime, kernel, allocator, TLS library, event loop, compression library, parser, database, shell, and media codec are not abstract trust roots. They are source trees.

Sugar's existing C lifter chain already handles this class of source: `collectors-defensive`, `kunit`, `assertions`, `kernel-doc`, `sparse`, and `walk-c`. The point is not that each project is already polished into a perfect contract set. The point is stronger and more operational: there is no new class of engineering required to extend coverage to these projects. The work is ingestion. Clone the repository. Add a target stanza. Run the pipeline. Store the mementos.

The total bedrock surface area is roughly 80 to 150 million lines across about twenty major projects. At measured throughput of 25 to 31 milliseconds per file per lifter on a 32-core machine, the entire bedrock lifts in 4 to 8 hours of wall time. One overnight ingestion run turns "we do not know what the runtime does" into a populated catalog of runtime contracts, refusals, and receipts.

That catalog dominates the trust surface because higher-level languages are surfaces over it. A Python contract that mentions `len(x)` ultimately passes through CPython's object protocol. A JavaScript array operation in Node ultimately passes through V8 and the Node runtime. A TLS operation from any language usually bottoms out in OpenSSL, BoringSSL, LibreSSL, SChannel, Secure Transport, or an equivalent native library. A database call bottoms out in a C or C++ database engine or in a C client library.

This does not demote surface-language contracts. Application intent lives there. But runtime truth lives below them. A `.proof` for a Python service without CPython in the catalog has a hole where the interpreter sits. A `.proof` for a Node service without V8 and libuv in the catalog has a hole where the execution engine sits. Bedrock ingestion closes those holes.

The counter-position is that modern systems include Rust, Go, Java, Swift, Kotlin, TypeScript, WebAssembly, eBPF, shaders, SQL, and DSLs, so C and C++ cannot be the whole story. Correct. They are not the whole story. They are the load-bearing runtime story. Rust crates still call libc, kernel syscalls, OpenSSL, SQLite, zlib, and libgit2. Go ships its own runtime, but it still crosses OS and C library boundaries. Java services sit on HotSpot. TypeScript sits on Node and V8. WebAssembly hosts embed engines written in C or C++. The bedrock thesis is not that every line of code is C. It is that the runtime layer beneath the broad deployed surface is finite, source-available, and already in the lifter's reach.

## §3: The supply-graph thesis

The npm lifter is not an npm trick. It is the registry pattern showing itself.

Every package registry is a content-addressed federated graph in waiting. npm, PyPI, crates.io, RubyGems, Maven Central, NuGet, Hex, and their mirrors all expose the same substrate shape: package identities, version identities, dependency edges, integrity metadata, author or publisher metadata, release times, tarball bytes, install scripts, and links to source repositories. A registry lifter extracts mementos for packages, versions, dependency edges, source artifacts, scripts, and policy-relevant metadata. It also cross-links packages to the native C and C++ source they wrap.

That last cross-link is the part security cares about. Most npm packages with real security weight wrap C or C++:

- `sharp` wraps libvips;
- `bcrypt` wraps native crypto code;
- `node-sass` wraps libsass;
- `better-sqlite3` wraps SQLite;
- `serialport` wraps libserialport;
- `canvas` wraps Cairo;
- brotli, zlib, snappy, and lz4 packages wrap native compression libraries.

Once the bedrock catalog has lifted libvips, libsass, SQLite, libserialport, Cairo, brotli, zlib, snappy, lz4, and their peers, the native trust surface of these package wrappers is already verifiable. The registry lifter does not need to prove libvips again. It needs to bind the package version to the native source CID, the binding code, and the install path. The hard runtime semantics have already landed in the catalog.

Supply-chain sabotage also becomes catalog-shaped. Postinstall scripts are not vibes. They are mementos with executable content, declared triggers, network behavior, filesystem writes, and refusal patterns. Prebuilt native binaries are not mysteries. They are artifacts with hashes, download URLs, platform selectors, and missing-source conditions. Native bindings without checksums are not merely suspicious. They are policy failures that mint refusal receipts.

The public incidents fit this structure. `eslint-scope` credential compromise, `ua-parser-js` malware releases, the `colors.js` self-sabotage, and left-pad descendant attacks all depend on the old substrate treating registry state as authority. In the Sugar substrate, those incidents become detectable graph patterns: maintainer credential discontinuity, version behavior discontinuity, install-time execution expansion, dependency replacement, unreachable or unpinned source, native binary without a source correspondence receipt. A consumer policy can refuse them before execution and carry the refusal as a receipt.

This is the supply-graph thesis: registries are already graphs, but they are not yet proof graphs. The lifter turns them into proof graphs by replacing names, versions, and install rituals with CIDs, dependency-edge mementos, native-source bindings, and policy receipts.

## §4: The Bridge collapse

Paper 13 introduced `LanguageMorphismMemento`: a content-addressed homomorphism between language signatures. That is the right general structure for cross-language semantics. But most deployed "cross-language" behavior is not a deep semantic bridge. It is a surface call into the C or C++ runtime that implements the surface language.

For built-ins, the morphism is often identity at the contract layer. Python `len()` is `PyObject_Length` reached through CPython's object protocol. Java `System.arraycopy` is a runtime intrinsic with native implementation and well-specified contract. JavaScript `Array.prototype.push` in V8 dispatches into the engine implementation. The surface-language built-in is the runtime function as exposed through the language's calling convention.

There is no semantic gap to discharge in these cases. There is a calling-convention shim to record.

Bridge obligations therefore split into two classes.

**Class A, trivial runtime shims.** A surface call maps to a C or C++ runtime function through a known calling convention: CPython `PyCFunction`, JVM native or intrinsic binding, V8 callback and builtin machinery, Node native addon ABI, Zend internal function binding, Ruby C API entry, BEAM NIF boundary, Lua C function binding, Perl XS, Tcl command registration, and comparable runtime surfaces. The contract-layer object is the runtime function's contract. The bridge memento records the dispatch convention, argument conversion, error convention, and return convention.

There are on the order of a dozen such shim families in the substrate. They are finite by enumeration because runtimes expose a finite set of native calling conventions.

**Class B, nontrivial cross-runtime FFI.** Java calling Rust through JNI, Python calling a user C extension through Cython, Node calling a native addon, Ruby calling a C extension, or a WASM guest calling a host function are real morphisms when the two sides carry separate surface semantics. These bridges have genuine homomorphism obligations. They need receipts.

Class B matters. It is also a tiny fraction of cross-language calls compared to the ocean of built-ins implemented by runtimes. The practical result is the Bridge collapse: the scary cross-language surface collapses, for the runtime substrate, into a small finite list of calling-convention mementos plus a much smaller set of real FFI morphisms.

CICP composes through both classes. The composite memento spans languages with one CID. Cross-language verification becomes a side effect of deduplication and composition, not a separate trust regime.

## §5: The five plateau axes

The substrate grows along five axes. All five plateau for a fixed ecosystem snapshot.

First, atomic contracts: per-function lifted contracts from source artifacts. This grows as source trees are ingested, then flattens when the source snapshot has been lifted.

Second, composite contracts: call-edge and dependency-edge contracts composed by CICP. This grows as the call graph and dependency graph are traversed, then flattens when reachable edges have receipts or refusals.

Third, runtime-shim Bridges: the Class A calling conventions named above. This set is finite by enumeration. A new runtime can add a shim family, but the major deployed substrate has a small count and changes slowly.

Fourth, registry packages: npm, PyPI, crates.io, RubyGems, Maven Central, NuGet, Hex, and related ecosystems. Each registry snapshot is finite. New releases become deltas, not reinvention.

Fifth, supply-chain pattern mementos: postinstall execution, prebuilt binary download, native binding without checksum, source repository mismatch, maintainer key discontinuity, dependency-edge surprise, install-time network access, and comparable refusal-receipt patterns. This set grows when new attack shapes are named, then stabilizes into policy vocabulary.

These axes are empirically convergent. They do not converge because software stops changing. They converge because the substrate is content-addressed. A changed file gets a new CID and a delta ingestion. An unchanged file hits the catalog. A new package version adds a version memento and dependency edges, not a new theory of packages. A new runtime shim adds one calling-convention memento, not a new language-verification regime.

The substrate exhausts when all five axes flatten for the snapshot in question. After that, verification is lookup, composition, and policy. Supra omnia, rectum binds the corpus.

## §6: Domain extension: same data structure, every domain

The `.proof` file is domain-agnostic because the substrate is domain-opaque.

Software uses lifters that produce C, Rust, Python, JavaScript, Java, or ProofIR contracts, then proves them with z3, cvc5, Vampire, Coq, Lean, Isabelle, or accepted portfolio members.

Hardware uses lifters that translate RTL, Verilog, VHDL, netlists, timing constraints, and equivalence claims into SMT predicates or proof-assistant goals. The receipts come from the same SAT, SMT, model-checking, and theorem-proving families.

Physics simulation uses lifters that bind numerical code to Lean, Coq, Isabelle, HOL, or domain-formalized equations. The proof portfolio checks discretization obligations, invariant preservation, conservation laws, or correspondence to accepted model mementos.

Legal contracts use lifters that encode clauses through Catala, RuleML, controlled natural language, or jurisdiction-specific predicate vocabularies. The receipts establish consistency, entailment, obligation triggering, and refusal where text cannot be formalized.

Financial transactions use lifters that produce invariant predicates over ledger state: conservation of balances, authorization, settlement constraints, collateral requirements, and temporal obligations.

Regulatory compliance uses policy-as-code surfaces such as Rego and Cedar, plus domain-specific mementos for control frameworks. The proof obligation is policy satisfaction under the artifact's lifted facts.

Medical device safety uses device-spec predicates, firmware contracts, RTL claims, temporal logic, and hazard constraints. The receipts bind behavior to the safety case.

Supply chain uses registry lifters for package closures and catalog membership. The proof obligation is often a lookup: every dependency edge, source binding, binary hash, and refusal pattern is or is not in the accepted catalog.

Same data structure. Same verifier. Same `memcmp`.

The domain does not disappear. The lifter and the portfolio have to be domain-correct. A bad legal lifter produces bad legal predicates. A bad RTL model produces bad hardware claims. A weak policy vocabulary leaves real obligations unnamed. The `.proof` file is not magic. It is the common envelope that makes those domain claims portable, comparable, signed, content-addressed, and locally checkable.

## §7: What .proof replaces

Human civilization built trust-in-authority mechanisms because direct verification was unavailable.

Auditors inspect. Regulators certify. Certification bodies grant DO-178C, Common Criteria EAL, ISO 26262, FDA SaMD, IEC 62304, FedRAMP, SLSA, and related standings. Peer reviewers recommend acceptance. Vendors warrant. Code-signing PKI says a key associated with a party signed the bytes. Third-party security firms assess. Package registries host and moderate. Institutions stand in the gap between artifacts and the people who need to rely on them.

A `.proof` file replaces the epistemic core of that arrangement. Not the legal power. Not the economic role. Not the human responsibility. The epistemic core.

The old statement is: someone you trust said it is fine.

The new statement is: the math says this bounded claim checks under this policy, here is how to verify it yourself.

That difference is not cosmetic. It changes who can participate. A small manufacturer can ship the same kind of `.proof` as a large vendor. A regulator can verify locally rather than trust a lab report's prose. A downstream consumer can pin a policy and reject artifacts without asking a central authority. An air-gapped operator can carry a proof through a controlled transfer and verify it without network access. A future maintainer can recheck a claim decades later as long as the cryptographic primitives and witness portfolio remain accepted.

Institutions still matter. They set policy. They decide which portfolios are acceptable. They decide liability when a claim is false or too weak. They decide whether a refusal is tolerable. What they stop doing is serving as the only practical proxy for checking the artifact's bounded truth.

## §8: The factory and the deliverable

The substrate exists to produce `.proof` files.

Lifters are factory machinery. AMP is factory machinery. LSP is factory machinery. CICP is factory machinery. Bridges are factory machinery. The npm lifter and every future registry lifter are factory machinery. Catalog storage, witness pluralism, refusal receipts, droppers, policy engines, and source-ingestion jobs are factory machinery.

The deliverable is the correctness bundle.

This distinction matters because it disciplines roadmap choices. A feature that does not improve the ability to mint, transport, verify, explain, or compose `.proof` files is not central. It can still be useful. It is not load-bearing. A source-ingestion feature that adds ten million lines of runtime coverage is central because it turns future `.proof` files from refusal-heavy bundles into composed bundles. A registry cross-link that connects `sharp` to libvips and a package version CID is central because it turns a JavaScript dependency into a verifiable native-source edge. A Class A Bridge memento for a runtime calling convention is central because it collapses thousands of built-in calls into one reusable shim.

Source ingestion is therefore the load-bearing piece. Without it, a `.proof` for an arbitrary deployed application is a map of holes. With it, the proof composes downward through the runtime and outward through the supply graph. Node app to V8 contracts. Node app to libuv contracts. Native module to C library contracts. User code to its lifted contracts. Registry package to source CID. Source CID to bedrock catalog. The final bundle is small enough to ship with the artifact.

That is the cypherpunk dream's final form. Do not trust institutions when local verification is possible. Trust cryptographic primitives, content-addressed claims, and accepted proof receipts. Verify locally.

## §9: Lemmas L1-L6

The following lemmas state the load-bearing claims in a form a careful reader can attack. They are not machine-checkable proofs. They are the proof obligations this paper relies on.

### L1: Bedrock Dominance

**Statement.** Let `A` be a deployed software artifact whose execution depends on a runtime stack `R`. If the operational semantics of `R` bottom out in a finite set `B` of C or C++ source artifacts, and the substrate has lifted `B` into contract mementos, then every runtime-layer trust obligation of `A` is reducible to catalog lookup, composition, or refusal over `B`.

**Proof sketch.** By hypothesis, runtime behavior reaches the machine through `R`, and `R`'s source-level semantics bottom out in `B`. Lifting `B` produces contract mementos and receipts for the functions, boundaries, and refusals the lifters can express. Any runtime-layer claim about `A` must either reference one of those lifted contracts, compose through them, or hit an unlifted or undischargeable boundary. The first two cases are catalog operations. The third case is an explicit refusal receipt. Therefore bedrock ingestion dominates the runtime trust surface for `A`.

The lemma does not say application logic is covered by bedrock. It says the runtime layer beneath the application is no longer an unexamined trust root.

### L2: Bridge Minimality

**Statement.** For surface-language built-ins whose implementation is a dispatch to a native runtime function, the `LanguageMorphismMemento` obligation reduces to a calling-convention memento plus the native function's contract. Nontrivial cross-language Bridge obligations are required only where two independently-specified surface semantics meet through FFI or equivalent runtime boundaries.

**Proof sketch.** A homomorphism obligation exists to prove semantic preservation across signatures. For a built-in implemented by the runtime, the surface operation and native function are not two independently-evolving semantic objects at the contract layer. The surface operation is the exposed entry point for the native implementation. The remaining obligation is that arguments, errors, return values, ownership, and effects are transported correctly through the runtime calling convention. That is a shim memento. In FFI cases, the caller and callee sit under different signatures with separate semantic commitments, so a real morphism remains necessary.

Thus Bridge obligations are minimal: finite shim families for runtime built-ins, real morphisms for genuine FFI.

### L3: Plateau Finiteness

**Statement.** For any fixed ecosystem snapshot `S`, the substrate's growth axes are finite: atomic contracts, composite contracts, runtime-shim Bridges, registry packages, and supply-chain pattern mementos. Therefore the catalog reaches a plateau for `S` after complete ingestion and discharge or refusal of reachable obligations.

**Proof sketch.** A fixed source snapshot contains finitely many files, functions, and lifted boundary obligations. A fixed call graph and dependency graph contain finitely many reachable edges. The set of runtime calling conventions in the deployed runtimes under `S` is finite by enumeration. A registry snapshot contains finitely many package versions and dependency edges. The policy vocabulary for supply-chain patterns accepted at `S` is finite. Each axis is therefore finite. Complete ingestion either emits receipts or refusals for each reachable obligation. When no new mementos are produced, the axis plateaus.

Ecosystems keep changing. That does not refute the lemma. It means plateau is a snapshot property and maintenance is delta ingestion.

### L4: Domain Agnosticism

**Statement.** If a domain claim can be encoded as a canonical content-addressed predicate and its discharge can be represented as an accepted receipt memento, then the `.proof` verifier handles that domain claim without domain-specific verifier changes.

**Proof sketch.** The verifier operates over canonical bytes, CIDs, signatures, composition references, witness receipts, and local policy. It does not inspect whether a predicate came from C source, RTL, legal text, financial ledgers, medical device specifications, or policy-as-code. Domain semantics are confined to the lifter and the accepted prove portfolio. Once those produce the same substrate objects, the verifier's algorithm is unchanged. Therefore domain extension does not require a new verifier, only new lifters, vocabularies, and portfolio receipts.

This is exactly paper 09's lossy boundary compression applied outside software: keep the obligation, discard source texture irrelevant to the boundary question.

### L5: Constant-Size Verification Preserved Under Composition

**Statement.** Suppose every composition step in a proof DAG mints a composition memento whose CID commits to its constituent CIDs and accepted receipts. If a `.proof` bundle carries the composite root CID, the policy pins, and the required signatures, then accept/reject verification of the bundle is constant-size with respect to the source artifact and dependency tree, assuming the referenced catalog is locally available and closed under the cited CIDs.

**Proof sketch.** Each composition replaces a subgraph with a content-addressed root that commits to the subgraph's endpoints and receipts. The outer `.proof` verifier checks the fixed acceptance surface: root CID equality, signature validity, policy acceptance, and the presence of required catalog entries. It does not re-run every solver or inline every proof term. Composition preserves the property because a composed memento is itself a memento with a CID. Repeated composition yields another CID. The verifier's accept/reject path remains CID recomputation and signature checking over the bundle's fixed fields.

Full audit expansion is different. A consumer can traverse the referenced DAG and recheck every receipt. That audit is linear in the expanded graph. The constant-size claim is about ordinary bundle verification against a locally closed catalog, not about optional forensic expansion.

### L6: CVE Blast-Radius is SELECT

**Statement.** In a catalog where artifacts, package versions, native-source bindings, dependency edges, contracts, and refusal patterns are content-addressed mementos, the blast radius of a CVE is a relational selection over the catalog plus graph reachability over dependency and call edges.

**Proof sketch.** A CVE names a vulnerable artifact, version range, function, predicate, behavior, or dependency edge. The substrate represents each of those as CIDs and memento relations. Affected artifacts are those whose dependency closure, call closure, native-source binding, or package-version relation reaches the vulnerable CID or satisfies the vulnerable predicate. That is a database query:

```sql
SELECT artifact_cid
FROM catalog_closure
WHERE reaches(vulnerable_cid)
   OR satisfies(vulnerable_predicate_cid);
```

The SQL is illustrative, not normative. The point is structural. Blast-radius analysis is no longer a prose exercise over names and guessed version ranges. It is selection and reachability over content-addressed facts. False positives become explicit over-approximations. Unknowns become refusal receipts. Native wrappers are included because registry lifters cross-link packages to the C and C++ sources they wrap.

## §10: What is and is not in this paper

This paper makes six claims.

First, `.proof` is the substrate's deliverable. Second, the runtime bedrock is mostly C and C++ source that the existing lifter chain can ingest. Third, package registries lift as supply graphs and cross-link to that bedrock. Fourth, most Bridge obligations collapse to finite runtime calling conventions. Fifth, the catalog grows along finite plateau axes for each snapshot. Sixth, the same bundle shape works across domains because the verifier is domain-opaque.

This paper does not claim that every contract is already authored, that every lifted predicate is strong, that every solver is sound, or that every future language boundary is trivial. It does not claim that institutions vanish. It does not claim that source availability alone proves correctness. It does not claim that C and C++ are good languages. It claims that the deployed runtime substrate is source-shaped, finite enough to ingest, and strategically decisive.

It also does not revise the whitepaper. That revision is a separate pass. This paper supplies the closing argument the whitepaper can later point to: the factory exists to produce the universal correctness bundle.

## §11: Out of scope

The concrete `.proof` wire schema is out of scope here. The protocol specs carry that.

Benchmark reproduction is out of scope here. The measured 25 to 31 milliseconds per file per lifter and the 4 to 8 hour overnight bedrock ingestion claim are operational planning facts for this argument, not a benchmark appendix.

The exact first ingestion target list is out of scope here. The bedrock examples name the obvious projects because the thesis depends on their shape, not on a particular overnight job order.

Legal acceptance by a regulator is out of scope here. Paper 04 traces the standards path. This paper argues what the artifact is once those regimes accept hash-bounded receipts as evidence.

Domain lifter correctness is out of scope here. A legal lifter, RTL lifter, medical-device lifter, or financial lifter needs its own proof story. The `.proof` file gives those stories a common carrier.

Social trust is out of scope except where it is displaced from epistemic authority into policy. Humans still decide what claims matter. The substrate decides whether the bounded claims check.

## §12: Closing principle

The substrate exists to produce `.proof` files.

Every other artifact is factory machinery: lifters, droppers, AMP, LSP, CICP, Bridges, registry lifters, C lifters, refusal receipts, catalogs, witness portfolios, policy engines, and ingestion runs.

The `.proof` file is the deliverable to the rest of civilization. It is small enough to travel through any pipe. It is stable enough to be rechecked decades later. It is local enough to verify without asking permission. It is strict enough to reject authority where receipts are missing. It is general enough to carry any domain whose obligations can be expressed as content-addressed predicates.

For software, it binds user code to runtimes and dependencies. For hardware, it binds RTL to claims. For law, it binds clauses to predicate obligations. For finance, it binds transactions to invariants. For medical devices, it binds safety cases to firmware and hardware facts. For supply chains, it binds packages to source, dependencies, refusals, and receipts.

After trust, the question is not "who says this is fine." The question is "what exactly is claimed, what receipts bound it, what policy accepts it, and can I verify it here."

The answer is the `.proof` file.

T Savo
