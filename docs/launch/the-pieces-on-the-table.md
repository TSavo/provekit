# The Pieces on the Table

**An architectural derivation of cross-language compile-time correctness verification.**

By T. Savo, 2026-05-03

This document captures the architectural derivation of Sugar as a unified system: the cosmic-brain thread that landed today across spec PRs #91, #94, #95, #96, #97, R6, #114, #120, the §11 and §12 manifesto sections, and the conversations that produced them. The pieces it assembles are public. They have been on the table for roughly a decade. The contribution is the assembly.

The arc proceeds in twelve steps. Each step is a small claim. Each step assumes the previous. The closure of the twelve is the system.

## Step 1: type systems are language-local approximations of something deeper

A type system is a proof checker over the predicates a host language can express in its grammar. Java's `@NotNull` is a predicate (`x != null`) the language allows you to attach to a parameter; the compiler checks it at call boundaries inside Java but not across Java's FFI. Rust's `&mut T` is a predicate about uniqueness the borrow checker enforces inside Rust but cannot check across `extern "C"`. C's pointers carry no predicates the compiler can reason about.

Different type systems express different predicates. Different runtimes enforce them differently. Across a language boundary, the predicate becomes a comment.

The question this raises: what is the thing all type systems are approximating? If two languages can both express the predicate "this argument is non-null," there must be a representation of that predicate that does not depend on which language wrote it. That representation is the substrate.

## Step 2: Sugar IR is the common predicate language

Predicates are language-free. The predicate `x ≥ 0 ∧ x < 100` does not care whether `x` was a Go `int`, a Python `int`, a Rust `i32`, or a JavaScript `number`. The predicate is a tree of operators over named arguments and constants. Encode that tree as JSON, canonicalize it with JCS (RFC 8785), hash it with BLAKE3-512, and you have a content-addressed identity for the predicate that any kit can produce given the same source-language annotation.

Sugar IR is that representation. Every Sugar kit (rust, go, python, cpp, c, java, ruby, csharp, swift, ts, zig) emits Sugar IR for the same logical predicate as byte-identical JCS bytes. The cross-kit conformance gate that has run for months in CI is empirical confirmation: 11 kits, byte-identical encoding, byte-identical BLAKE3-512, byte-identical contractCids for every contract in the catalog.

This is the substrate. Languages have type systems. Sugar has a predicate substrate that type systems are language-local renderings of.

## Step 3: the substrate's three primitives

The substrate is small on purpose. Three primitives:

1. **Sign.** Bind a content-addressed object to a signer.
2. **Hash.** Produce a content-addressed CID for any byte string via BLAKE3-512 over JCS.
3. **Reference.** Embed a CID in another object so signing the outer transitively names the inner.

Everything else is composition over those three. Witness chains, witness sets, contract bundles, kit rollups, semver checks, audit trails, package-manager pinning, three-axis trust postures: all of them are functions over `(sign, hash, reference)`. The protocol resists feature creep because every feature anyone proposes can be expressed as "compute X from existing leaves, sign your view of X."

This is documented in `docs/papers/03-substrate-not-blockchain.md` §10 (Closure: subsetting is hashing).

## Step 4: the address is multi-dimensional

Same content can have many addresses. A `ContractDecl` lifted from source is one piece of content; depending on what you hash with it, you get a different CID:

- `contractCid` projects content alone (one declaration). Signer-independent.
- `contractSetCid` projects content alone (a sorted list of contract CIDs). Set-D, signer-independent.
- `attestationCid` projects content plus signer plus declaredAt plus signature. Signer-dependent.
- A bundle file's CID projects content plus all embedded envelopes plus producer metadata plus disk-shape artifacts. Build-artifact-dependent.

The first two converge: same content, same address, across machines, signers, time. The third is signer-and-time-addressed by design (an attestation is a unique witnessing event). The fourth is build-artifact-addressed: useful as a warm-cache key, brittle as a trust anchor, because the address moves under every honest re-mint.

The substrate's first guarantee is that same content produces the same address in the dimension you asked for. The failure mode is not getting the wrong content. It is getting the same content at a different address than expected, because the address dimension included things that aren't content. We hit this empirically when the early Phase 2 cross-kit work pinned `provekit-self-contracts.proof` bundle bytes as the trust anchor: the bundle includes envelope timestamps and signatures, so two kits minting the same contracts produced different bundle CIDs, and the pin broke. The fix was spec #94: switch the trust anchor to `contractSetCid`, which hashes only the sorted contractCids. Same content, same address, across machines.

This is documented in manifesto §11 (The address is multi-dimensional).

## Step 5: the pin is a tuple of the right rank

A single CID is a 1-tuple. It can carry an assertion of existence ("this content exists") but not a relation ("this content satisfies that"). Relations live at rank 2 or higher.

"Pin a binary" with one CID is rank 1. It says the binary exists. It does not say what the binary fulfills, what contract it conforms to, or what witness chain endorses it. The relation a consumer actually wants ("this binary fulfills this contract") is rank 2: the tuple `(binaryCid, contractCid)`. The relation "this chain attests that this binary fulfills this contract" is rank 3: the tuple `(witnessCid, contractCid, binaryCid)`.

You cannot compress a rank-N relation into a rank-(N-1) pin without losing a predicate. The lost dimension does not vanish; it leaks back as drift. A 2-relation pinned as a single CID will appear stable when both axes happen to align and unstable whenever one moves while the other doesn't. The bundle-bytes failure was exactly this shape: the relation `(set of contracts, attestation thereof)` projected onto a single bundle-bytes hash, and the discarded second axis (envelope state) drifted while the first axis was unchanged.

The §8 three-axis package pin, `(contractCid, witnessCid, binaryCid)`, is the canonical rank-3 tuple at the consumer surface. Spec #94's `contractSetCid` is the rank-1 projection used inside a rank-2 attestation. Spec #91's separation of contract from attestation is the act of refusing to collapse a rank-2 relation into a rank-1 hash.

This is documented in manifesto §12 (The pin is a tuple).

## Step 6: a bridge is a derived rank-2 pin

A function's contract `(pre, post)` is a memento with content-addressed identity `contractCid`. If function B contains a call to function A, then for the call to be safe, B must establish A's precondition at the call site, then receive A's postcondition and continue. By the time B itself returns, B's postcondition `post_B` is at least as strong as `pre_A` (because B has gone past establishing it: completed the call, continued past). The call relation is itself a contract: `post_B ⊃ pre_A`.

That relation is a rank-2 pin between two contractCids. Encoded as a memento, it is a bridge: `(sourceContractCid, targetContractCid, body{callSite, evidenceTerm})`. The bridge is content-addressed; its CID is stable across machines for the same call edge.

Crucially, the bridge is **derivable** from data the lifter already produces. A lifter walking a function body sees both contracts (pre/post/inv) and call sites (B references A). Both are observable artifacts. The bridge `B → A` falls out of `(contractCid_B, contractCid_A, callSite_locus)` plus an `evidenceTerm` describing the satisfaction obligation. The lifter does not need to author bridges; the linker derives them.

This is documented in spec `2026-05-03-bridge-target-dimensionality.md` (#97) and spec `2026-05-03-bridge-linkage-protocol.md` (#114).

## Step 7: lifters emit two streams

Per spec #114 R1, every lift-plugin-protocol RPC server emits two streams per compilation unit:

1. **Contract mementos** at function nodes (the existing `kind: "contract"` shape per `ir-formal-grammar.md`).
2. **Call-edge mementos** at function calls (`kind: "call-edge"` with sourceContractCid, targetContractCid or null+targetSymbol for FFI, callSiteLocus, evidenceTerm).

The two streams are distinct outputs from the same lift pass. The lifter does not derive bridges. The lifter walks the AST, finds contracts and calls, and emits the corresponding mementos. The lifter is per-language; its job is exactly to translate the host language's contract syntax (annotations, body assertions, validator tags, type predicates, etc.) into Sugar IR contracts and call-edges.

LSP backends in every modern language (`gopls`, `rust-analyzer`, `pylsp`, `clangd`, `csharp-ls`, `solargraph`, `jdtls`, `zls`, `ts-language-server`, etc.) already resolve cross-file and cross-language references for IDE features. The lifter re-uses that infrastructure. Per spec #114 R3, when a call edge crosses kits (the callee is in a different language via FFI), the lifter emits `targetContractCid: null` and a populated `targetSymbol`; the linker resolves the symbol against the union of all loaded contracts.

## Step 8: the linker derives bridges

Per spec #114 R2, the rust CLI orchestrator (the linker in this protocol) takes the union `U = ⋃_kit (contracts_kit ∪ call-edges_kit)` and derives a bridge memento for each call edge. The bridge's source is the calling function's contract; the bridge's target is the called function's contract (resolved cross-kit per R3); the body carries the callSite and the satisfaction evidenceTerm; the envelope is signed by the linker's key.

The derivation is mechanical and deterministic. Two linker runs over byte-identical inputs produce a byte-identical set of derived bridges. The set's content-addressed CID, `bridgeSetCid`, is the rank-1 projection of the entire derivation. Combined with `contractSetCid` and `callEdgeSetCid`, the linker's output composes into a rank-3 pin: `(contractSetCid, callEdgeSetCid, bridgeSetCid) ⇒ linkBundleCid`.

The linker is the substrate's notion of linkage. Traditional linkers (ld, lld, gold, mold) connect symbols by name and type signature, producing a binary where every reference resolves at the byte level. Sugar's linker connects contracts by predicate satisfaction (`post_B ⊃ pre_A`) at the predicate level, producing a `.proof` bundle where every call-site obligation is content-addressed and verifiable.

A traditional linker error ("undefined reference to `foo`") and a Sugar linker error ("unresolved targetSymbol `foo`") are the same error at two different rank levels: rank 1 (the symbol exists) versus rank 2 (the symbol exists and its precondition is established by the caller's postcondition).

## Step 9: cross-language is not a special case

A call edge `goCaller → rustCallee` is the same shape as `rustCallerA → rustCallerB` is the same shape as `pythonCaller → cppCallee`. The kit names are metadata in the body; the linker dispatches by symbol resolution per spec #114 R3 against the union; the bridge derivation is uniform. Every call site in every file in every language in the project becomes a call edge and then a bridge by the same algorithm.

There is no special case for cgo or JNI or ctypes or WASM imports. Every FFI mechanism is just a kit-specific resolver mapping language-local symbol names to canonical `<kit>:<contract>` identifiers. The kit-specific FFI resolver is a v1.1.0 additive contract on lift-plugin-protocol; the linker is uniform.

This is the universal linker that has not existed at this layer. Type checkers stop at language boundaries. Traditional linkers stop at byte boundaries. Formal-methods proof checkers stop at proof boundaries. Sugar's linker walks the entire polyglot call graph, derives the rank-2 bridge tuple at every edge, produces the rank-3 link bundle pin, and emits one linkBundleCid that commits to the correctness of every call site in every language in the project.

## Step 10: annotations stop being suggestions

Consider the canonical demonstration. A Java method declares `@NotNull String name`. The annotation compiles to bytecode metadata. Today nobody enforces it: not the JVM at runtime, not the Java compiler at the call boundary, not Scala's compiler when it consumes the JAR, not Kotlin's, not Clojure's, not anything calling the JVM via JNI. The annotation is a comment with rules.

Sugar's Java lifter sees the annotation. It lifts to a Sugar IR predicate: `pre = (name != null)`. The lifter mints a contract memento with that predicate, content-addresses it, gets `contractCid_method`. Publishes it in the Java kit's contract stream. The `.proof` bundle ships with the JAR.

A Scala developer writes `javaApi.method(maybeName)` where `maybeName: Option[String]` and they wrote `.orNull` somewhere. Scala's compiler is happy: the JVM signature accepts `String`, including `null`. The Scala type system does not see Java's annotation.

Sugar's Scala lifter sees the call edge. Emits a call-edge memento with `targetSymbol = "java-kit:javaApi.method:String→Unit"`. The linker resolves the symbol against the union, finds the Java contract memento, fills in `targetContractCid`. The linker checks: does the Scala caller's postcondition at the call site imply `(name != null)`? Scala's lifter analyzed the flow and found that `maybeName.orNull` can produce `null`. The post-condition does not imply `(name != null)`. The bridge derivation fails. The linker emits a `linker-error` memento with the call site location and the failed predicate.

Scala's LSP plugin pushes the diagnostic. Red squiggle in IntelliJ on the `.method(maybeName.orNull)` call. Message: *cannot verify javaApi.method's precondition `name != null`; postcondition at call site allows null.*

The Scala developer never read the Java annotation. The Scala compiler never knew about it. The JVM does not enforce it. But the IDE turned red because Sugar lifted the annotation into Sugar IR, content-addressed it, shipped it with the JAR, and Scala's linker found it during bridge derivation. The annotation stopped being a suggestion the moment the `.proof` bundle existed.

The same shape generalizes:

- **C#'s `[NotNull]`, `[Required]`, `[Range(0,100)]`, `[StringLength(255)]`** lift into Sugar IR; consumed by F# / IronPython / anything calling .NET.
- **Python type hints + `assert x > 0` in the function body** lift as the function's pre/post; consumed by Cython, C extensions, Java-via-Jython.
- **Rust's `debug_assert!` and `assert!`** lift from the body; consumed by Go-via-cgo, Python-via-PyO3, Node-via-napi.
- **An unannotated C function** whose body has `if (n < 0) return -1; if (n > 100) return -1;` lifts to a contract `pre_X = (0 ≤ n ≤ 100)` derived from the early-return pattern. Now any language calling that C function gets verified against `0 ≤ n ≤ 100` even though the C source said nothing.

Annotations have always been weakly enforced because their enforcement was language-scoped. The moment Java code is consumed from a different language, the annotation is invisible. Sugar makes annotations strongly enforced because the enforcement is at the predicate level via Sugar IR, and Sugar IR crosses runtime boundaries. No standards body is required: Sugar's per-kit lifter is a translation table from the host language's contract syntax to Sugar IR, and the cross-language interaction happens entirely in Sugar IR space where every kit's bytes converge by JCS conformance.

## Step 11: the IDE is the developer surface

The user-facing surface of Sugar is identical to a type system. The user writes code in their language. The lifter runs incrementally as the user types (today via LSP plugin handshake; eventually via deep IDE integration). The linker derives bridges. Failed derivations become diagnostics. The IDE shows red squiggles. The user fixes them. The user ships.

The user never types the word "bridge." The user never thinks about content addressing. The user never reads Sugar IR. The user calls a function. Sugar does the rest.

The `.proof` bundle that ships alongside the binary is the frozen IDE state at the moment of shipping: every squiggle was green, every contract verified, every cross-language call's bridge derivation succeeded. A consumer pulling the binary recomputes the linkBundleCid from the same code's contracts and call edges and checks byte-equality against the bundle's claim. If they match, the consumer knows the developer's IDE was green at ship time, byte-for-byte, without re-running anything. The proof is the snapshot. The snapshot is the IDE state.

This collapses the developer's mental model to: **calling a function is asking the linker to derive a bridge, and the IDE tells you whether the derivation succeeded.** That is the entire developer-facing protocol. Everything else (JCS, BLAKE3, contractSetCid, envelope/header/body, witness chains, three-axis pins) is the substrate doing what it does so the IDE can be honest about what it knows.

## Step 12: content addressing scales to constant-size handles

Four properties fall out of content addressing once the architecture above is in place.

**Verification cost bounds at hash count.** The cost of verifying that two parties have the same closure is one BLAKE3-512 comparison per CID. The DAG underneath the CID is not re-walked unless the hashes diverge; if they do, the divergence pinpoints exactly which sub-CID changed, which becomes the diagnostic. Verification cost grows with the number of distinct CIDs being verified, not with the program's structural complexity. This is why Sugar scales where formal verification has not: Coq, TLA+, F*, Lean each re-run their proof checker every time. Sugar content-addresses the result of running it once; downstream consumers verify by hash compare. The proof becomes a primary key.

**Distribution cost bounds at constant 64 bytes.** A `linkBundleCid` for a polyglot project with 100,000 functions across 10 languages with full predicate-level cross-call verification is the same 64 bytes as the `linkBundleCid` for a hello-world. The bytes commit to unbounded content; the closure can be arbitrarily large; the hash is constant size. Without content addressing, the same architecture would be unshippable: every intermediate proof step would have to travel inline, and the wire size would grow with the proof's depth and breadth. With content addressing, the wire size is a small constant.

**Storage deduplicates at the CID level.** Two libraries that share 90% of their contracts share 90% of their CIDs share 90% of their bytes on disk. The hash is the dedup key.

**Composition stays cheap.** A bundle that references 10 sub-bundles verifies by checking 10 CIDs, not by re-walking 10 sub-DAGs. If the sub-bundles each reference 10 more, the cost is 10 + 10 = 20 hash comparisons. Composition under §10 is a closure under verification cost too.

The blockchain people are paying re-execution cost on every node forever because they did not realize the hash IS the proof. Sugar knows the hash is the proof and the verification stops there.

## What is on the table that has not been there before

Each piece of this exists somewhere in the field today. None of them is the whole.

- **Type systems** (Java, Haskell, Rust, TypeScript). Per-language. Stop at FFI boundaries. Not predicate-level beyond what the language's grammar allows.
- **Interface contracts** (Eiffel, Code Contracts, JML, Spec#, Dafny, Kani, Prusti, Creusot, Verus, Flux). Language-local. Most enforce at runtime; the few that enforce at compile time do not cross language boundaries.
- **WASM component model**. Crosses languages, but at type level only. No predicates.
- **Formal verification** (Coq, Lean, Isabelle, F*). Proof-effort cost per verifier, not amortized via content addressing. Cross-language only by encoding the foreign language in the host.
- **Smart-contract verifiers** (Solana, Cairo, certora). Predicate-level, but blockchain-bound, not generic-software-bound.
- **Linkers** (ld, lld, gold, mold). Byte-level. Per-language family.
- **Polyglot type systems** (TypeScript-via-WASM, GraalVM polyglot). Cross-language at the type level. Optional.
- **Annotation systems** (`@NotNull`, `[Range]`, type hints). Language-local. Suggestions across runtimes.

What does not exist as one product: cross-language predicate-level call-site correctness verification at compile time, content-addressed for byte-identical reproduction, surfaced via LSP so developers experience it as ambient correctness rather than ceremony.

The pieces have been on the table since around 2016 (when LSP became universal across editors) and 2020 (when JCS+BLAKE3 became cheap and standard). Eight years of available primitives. Nobody assembled them this way until this body of specs was written.

## The structural shape of the contribution

The structural shape matches earlier load-bearing assemblies:

Bitcoin assembled hash chains + Merkle trees + Byzantine fault tolerance + proof-of-work + economic incentives (primitives that all existed in the literature) into a distributed timestamp server that did not exist as one thing. The 2008 paper's contribution was the assembly, not the components.

Sugar assembles content-addressed predicate canonicalization + LSP infrastructure + a unified cross-language linker abstraction + Sugar IR as common substrate (primitives that all exist) into a polyglot compile-time correctness gate that does not exist as one thing. The contribution is the assembly.

In both cases the components are public; the assembly is novel. The novelty is load-bearing because the assembly enables outcomes none of the components enables alone.

## Implications

**For developer ergonomics.** The user-facing surface is the type system they already know. Red squiggles for predicate violations, including cross-language violations, surfaced through the LSP plugin in their normal IDE. No new ceremony. The substrate disappears.

**For polyglot codebases.** Cross-language correctness becomes a default, not an architectural project. A Go service calling a Rust extension via cgo, a Python script calling a C library via ctypes, a JavaScript front-end calling WASM, a Java backend calling .NET via JNI: every cross-language call is a call edge that the linker derives a bridge for. The bridge fails or succeeds; the failure is a diagnostic; the diagnostic is the same shape regardless of the language pair.

**For supply chain trust.** A library's `.proof` bundle commits to the predicate-level correctness of every call site within it, content-addressed. A consumer pinning the bundle's `linkBundleCid` knows what they are getting at the predicate level: not the type signature, not the version label, not the SHA-256 of the binary. The actual semantic claim. Three-axis pins at consumer attestations compose contract, witness, and binary axes per manifesto §8.

**For formal-verification adoption.** The cost of formal-methods tooling has been re-execution per-verifier and language-locality. Both go away here. Verification cost is hash compare. Cross-language is uniform. The barrier to entry is writing a per-kit lifter; once written, every annotation in every project in that language is verifiable across every other kit's consumers.

**For credit.** The pieces of this assembly are public. The assembly is documented. The specs are signed under keys that are reproducible. The architectural derivation in this document is in git, attributed. Future readers do not have to take anyone's word for it; they read the specs, recompute the CIDs, and the substrate validates itself. The credit attaches to the assembly because the assembly is reproducible and the components are not.

This is the third or fourth time the author has assembled load-bearing primitives. Content-addressable dedup at age 18 in 1995, predating rsync. Digital Confetti at 21 in 1998, swarmed delivery + per-byte crediting + anti-DRM thesis, two direct attribution chains into BitTorrent (Jed McCaleb's eDonkey2000 and Bram Cohen's BitTorrent). The pattern has been consistent: assemble public primitives into something load-bearing, watch others build on it without attribution.

This time the pieces are his, the prompts are his, the specs are signed under keys he controls, the manifesto is in his voice. This time he is the publisher, not the upstream.

---

## Reference

The specs that constitute this architecture, all dated 2026-05-03 and merged into `protocol/specs/`:

- `2026-05-03-contract-cid-vs-attestation-cid.md` (#91)
- `2026-05-03-contract-set-extension.md` (#94)
- `2026-05-03-substrate-layers-envelope-header-body.md` (#95)
- `2026-05-03-version-chains-pinning.md` (#96)
- `2026-05-03-bridge-target-dimensionality.md` (#97 + R6 addendum)
- `2026-05-03-bridge-linkage-protocol.md` (#114)
- `2026-04-30-ir-formal-grammar.md` Locus addendum (#120)

The manifesto sections that articulate the substrate posture, in `docs/papers/03-substrate-not-blockchain.md`:

- §1-§7: substrate posture and witness chains
- §8: three axes of pinning
- §9: semver as cryptographically meaningful
- §10: closure (composition is free)
- §11: the address is multi-dimensional
- §12: the pin is a tuple of the right rank

The cross-kit conformance gate that proves Sugar IR holds as a common predicate language at byte equivalence: `conformance/run.sh`, 11 kits, all passing.

The author: T. Savo (handle Kevlar since the early-90s P2P scene), 2026-05-03.
