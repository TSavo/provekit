# The Universal Address Space: After Portability

> **Status.** Sustained argument. Contains six lemmas with proof sketches. Written to be cite-able.
>
> **Companion to.** [09 Lossy Boundary Compression](09-lossy-boundary-compression.md), [12 After Languages](12-after-languages-how-proofir-represents-every-language.md), [13 After Grammars](13-after-grammars-programming-languages-as-content-addressed-algebras.md), [14 After Trust](14-after-trust-the-universal-correctness-bundle.md), [15 After Civilization](15-after-civilization-why-the-author-doesnt-matter.md), [Realizer Protocol v0.2](../../protocol/specs/2026-05-10-realizer-protocol-v2.md), [C11 Language Signature Memento](../../menagerie/c11-language-signature/README.md), [Foo Algebraic Shape](../../menagerie/foo-algebraic-shape/README.md), [provekit-lift-asm-aarch64](../../implementations/rust/provekit-lift-asm-aarch64/README.md), and [provekit-lift-asm-x86-64](../../implementations/rust/provekit-lift-asm-x86-64/README.md).
>
> **Premise the earlier papers established.** Paper 12 made algorithms content-addressed mementos. Paper 13 made programming languages content-addressed algebras. Paper 14 named the deliverable as the `.proof` bundle. Paper 15 removed the author as an epistemic input. Paper 9 established ProofIR's lossy contract-boundary universality. ORP v0.2 sharpens that last claim by splitting ProofIR into a lossless term stratum and a lossy contract stratum.
>
> **What this paper argues.** The universal object is not any artifact that runs. It is the algebra that artifacts realize, and content-addressing gives that algebra an address space. Primitive operations, idioms, function shapes, and contract-equivalence classes become CIDs. Lift, canonicalize, and discharge morphisms, and the address is discovered. The universal address space is the category of colimits of lift diagrams, monoidal under conjoining, with exact quotienting where older portable layers only approximated. Portability was always lift to the algebra, then realize back down. The missing step was making the algebra first-class and addressable.

## §0: Why this paper exists

The earlier papers established the pieces. This paper names the universal object directly.

Paper 13 said programming languages are content-addressed algebras. That was the language-level reframe: a language is not its parser, compiler, VM, syntax, or brand. It is a finite signature of sorts, operations, equations, and effects, plus morphisms to other such signatures.

Paper 14 said the deliverable is the `.proof` file. That was the consumer-level reframe: the civilization-facing artifact is not a solver run, not an audit report, not a certificate authority's blessing, and not a person's standing. It is a compact correctness bundle whose ordinary verification reduces to CID recomputation, byte comparison, signatures, receipts, and policy.

Paper 15 said the author does not matter. That was the producer-level reframe: the truth-value of a bounded claim is invariant under change of author, because the verifier consumes bytes and receipts, not biography.

This paper comes after those closures. It is the capstone after the capstone because it names the thing the ladder has been circling: the universal address space is not a portable instruction set, not a universal bytecode, not a syntax tree, not a VM, not a compiler IR, not a proof file, and not a signature on any one of those. The universal object is the algebra itself, with content addresses for its elements and its quotient classes.

That sounds abstract only if addressability is absent. Without CIDs, an algebra element is a mathematical idea. With CIDs, it is an artifact. It can be referenced, cached, signed, conjoined, lifted into, realized out of, and federated. The operation `eq` is no longer merely a word in a spec. It has a CID. The operation `if` has a CID. A function shape like `foo` has a CID. A boundary contract-equivalence class has a CID. Those CIDs are not names chosen by an authority. They are deterministic consequences of canonical bytes and discharged morphisms.

The universal address space is therefore not a registry someone designs in advance. It is the object that appears when content-addressing, canonicalization, language signatures, lift morphisms, and contract discharge are all present at once.

## §1: The compiler IR history reframe

The history of portable layers is a history of looking for universality in the wrong place.

Every generation built a common concrete representation and called it portable. C became portable assembly. JVM bytecode became the portable layer for Java and then for many guest languages. .NET CIL became a managed common instruction set. LLVM IR became the common compiler middle. GIMLE and related research IRs tried to normalize low-level behavior for analysis. WebAssembly became a portable target for the web and beyond.

Each was useful. None was universal.

The reason is structural. Every concrete representation runs or lowers somewhere. JVM bytecode runs on x86-64, AArch64, ARM, RISC-V, and whatever future host implements the VM. WebAssembly runs in engines that are themselves native artifacts. LLVM IR compiles to target ISAs. C compiles to object code. CIL runs through a runtime and a JIT or AOT compiler. There is always a lower level. If "portable" means "this artifact runs unchanged everywhere," then every portable layer becomes non-portable the moment you inspect the thing that realizes it.

The mistake was searching for the universal thing among artifacts that run.

The universal thing does not run. It is what running artifacts realize.

The universal thing is the algebra: operations such as `seq`, `if`, `while`, `call`, `return`, `eq`, `deref`, `add`, `load`, `store`, `alloc`, and `trap`, together with equations such as associativity of `seq`, identity of `skip`, idempotence of same-armed conditionals, branch simplification, and effect laws. This algebra is not a target machine. It is not a host language. It is the structure common to target machines and host languages once their concrete surfaces are lifted.

`JNZ` is not portable. It is an x86 conditional branch instruction, tied to RFLAGS, encodings, instruction decoding, calling conventions, and target layout. AArch64 has `b.ne`, `cbz`, and `cbnz`, tied to NZCV flags or register tests and its own instruction set. C has `if (x)`, tied to the C language's truth conversion and syntax. These are different artifacts.

But each realizes an algebra element: branch on a boolean or branch on nonzero, depending on the precise lifted operation. The concrete encodings differ. The algebra element is one.

Portability was always the two-step path:

1. Lift a concrete artifact to the algebra.
2. Realize that algebra into another concrete artifact.

That is why all successful portable layers have the same shadow. They are not universal because their bytes are universal. They are useful because they approximate the hub. JVM bytecode is a hub for JVM languages. LLVM IR is a hub for compiler backends. WebAssembly is a hub for sandboxable modules. C was a hub for systems that had C compilers. Each hub is partial because each is still a concrete representation.

The algebra is the hub that is not another concrete representation.

Nobody made the hub first-class before because nobody could address it. A universal algebra without content-addressing is a theory. A universal algebra with CIDs is a substrate.

## §2: Content-addressing makes the algebra element an artifact

Content-addressing is the move that turns the algebra from "what we mean" into "what we can hold."

Each algebra element has three pieces.

First, it has a canonical payload: the memento that states the operation, sort, equation, effect signature, term shape, contract shape, or morphism. The payload is serialized deterministically. Byte equality is meaningful.

Second, it has a calculus: CCP composition and conjoining over shapes, LSP morphism composition over signatures, AMP algorithm identity, ORP realization through discharged morphisms, and the usual predicate implication and witness specialization machinery. The object does not sit alone. It composes.

Third, it has a discharge procedure: the prove portfolio, canonicalizer, morphism checker, or representation map that establishes when two concrete presentations are the same object in the algebraic quotient. Some discharges are solver-heavy. Some, like the `foo` renaming morphisms, are canonicalizer-only: alpha-equivalence, source-name replacement, representation folding, deterministic serialization, and CID comparison.

Once those exist, `=` is not merely a token. It is an operation CID. `branch-on-nonzero` is not merely an English phrase. It is an operation CID once minted under the signature. `foo`'s shape is not the C spelling, the x86-64 instruction sequence, or the AArch64 instruction sequence. It is the CID of the shared term or contract shape those lifts reach.

The [C11 Language Signature Memento](../../menagerie/c11-language-signature/README.md) is the empirical beginning of this address space. It mints a C11 signature CID:

```text
blake3-512:c942ba70e4b701e139a46590116f5cdc16ab41db277e80e54c01f23e4a7cf6241d4431c60473409cb3f0b61ce27f593071c1c224291f04c61aa04b4773764945
```

It also mints operation CIDs for `seq`, `if`, `while`, `call`, `return`, `deref`, `add`, `eq`, and the rest of the core slice, plus equations and effect signatures. That catalog is not a complete periodic table of computation. It is the first visible row. The important fact is not completeness today. The important fact is that the row is minted at all.

The address space's alphabet is the operation-CID set. Its words are terms over that alphabet. Its grammar is the equation mementos. Its topology is induced by discharged morphisms. Its equality is content-addressed quotienting, not prose agreement.

This is the first moment the phrase "universal address space" becomes literal. An address is not assigned by committee. It is computed from the canonical object. If a concrete artifact realizes the same object after accepted morphisms, it lands at the same address. If it does not, it lands elsewhere or refuses.

## §3: The colimit framing

The word "universal" here is not marketing. It is the universal property of a colimit.

Take a concrete function shape, such as the `foo` exhibit:

```c
int foo(int x) {
  if (x == 0) return -22;
  return x;
}
```

Now lift it from C, x86-64, and AArch64. The lifts disagree in surface names, return slots, registers, literal encodings, and representation domains. C says `x` and `result`. AArch64 says `w0` and `w0_out`. x86-64 says `edi`, `eax_post`, and in one branch a two's-complement literal that must fold to `-22`.

The [Foo Algebraic Shape](../../menagerie/foo-algebraic-shape/README.md) exhibit gives the quotient:

```text
lambda arg_0. ite(arg_0 == 0, -22, arg_0)
```

and the shape CID:

```text
blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1
```

This shape is the colimit of the diagram formed by the concrete lifts and their morphisms into the shared shape. The three renaming and representation morphisms form the cocone. The shape is the vertex through which the concrete presentations factor. The universal property says that any other object receiving compatible maps from those concrete lifts factors uniquely through the shape, up to the accepted equivalence relation.

This is exactly what the exhibit does operationally. It substitutes source-specific names into `arg_0` and `ret`, maps machine representations into `Int`, folds the x86-64 literal into `-22`, canonicalizes, and checks that each after-substitution CID equals the shape CID. No solver is needed because the morphisms are representation and renaming morphisms.

The symbols in the shape are therefore not names in the old sense. `arg_0` is not a preferred spelling. It is a colimit vertex. It is the equivalence class of the C parameter, the AArch64 argument register, and the x86-64 argument register under the accepted morphisms. `ret` is not a source variable. It is the return-value vertex shared by the diagram. The address space is unbounded across the language graph because any language can contribute another lift and another morphism into the same colimit.

The category of these objects is monoidal under conjoining. If `foo` has a shape and `bar` has a shape, then `seq(foo, bar)` is not invented from scratch. It is `seq` applied to the colimit of `foo` and the colimit of `bar`, subject to the `seq` equations. Composition is a functor. Colimits compose. CCP conjoining is the contract-layer form of the same operation.

That is the universal-language-concept space: the category of colimits of lift diagrams, monoidal under conjoining.

## §4: The self-organizing map, without loss

The CID space behaves like a self-organizing map, but with the part that matters made exact.

A Kohonen map projects high-dimensional inputs onto a lower-dimensional grid. Nearby inputs land near each other. The cost is loss. A real self-organizing map preserves neighborhood structure approximately by accepting distortion. Two inputs that are similar may land near one another, but not at the same node unless the projection makes them collide. Similarity is represented by distance.

The universal address space does something stricter. The metric is:

```text
differ only by a discharged morphism
```

If two concrete artifacts differ only by renaming, representation choice, syntax, register carrier, branch spelling, or another accepted morphism, they do not land near each other. They land at the identical CID. The topology preserved is the equivalence relation "is the same concept." It is not approximate. It is exact because the canonical bytes are deterministic and the discharge rule is explicit.

This is a self-organizing map without lossy projection at the concept layer.

The self-organization is deduplication. When the C lift, x86-64 lift, and AArch64 lift of `foo` pass through canonicalizer-discharged morphisms, the catalog does not create three nearby concepts. It creates one shape CID with three receipt-backed paths into it. When an x86 `jne`, an AArch64 `b.ne`, an AArch64 `cbz`, and a C `if` all lift to the same branch operation under their signatures and morphisms, the address space records one algebra element with multiple realizations.

The neighborhood function is the homomorphism portfolio. Renaming morphisms, representation morphisms, language morphisms, compiler morphisms, and contract implication morphisms are the ways the map knows what "near" means. But once a morphism discharges as equality in the relevant quotient, nearness collapses to identity. If the morphism does not discharge, the inputs remain distinct or refuse.

This matters because "similar" is too weak for correctness. A binary and a source function cannot be accepted because they are close. A Java guard and a TypeScript schema cannot share a receipt because they look alike. They share only when the boundary predicate or term shape is the same object under accepted morphisms.

The address space is therefore a self-organizing concept map with no probabilistic clustering step. Lift produces candidates. Canonicalization normalizes them. Discharge decides the quotient. The CID records the result.

## §5: We discover the addresses; we do not invent them

The addresses exist the moment the machinery exists.

That sentence is easy to misunderstand. It does not mean the catalog is already complete. It does not mean every operation has already been minted. It does not mean every language has a lifter. It means that for any object within the supported signatures and morphisms, the address is not a matter of taste. It is the deterministic result of lift, canonicalize, and discharge.

Discovery procedure:

```text
concrete artifact
  -> lift
  -> term over operation CIDs
  -> contract projection when needed
  -> canonicalize
  -> discharge morphisms
  -> shape CID or refusal
```

This is the same pattern at several layers. A C function lifts into a C11 term and its contract projection. An x86-64 instruction sequence lifts through the x86-64 lifter into a machine-signature term and contract shape. An AArch64 instruction sequence does the same through its lifter. If their morphisms into a shared shape discharge, the shape CID was discovered. If they do not, no amount of desire makes them the same address.

The `foo` exhibit is the small working proof. Three lifts collapse to:

```text
blake3-512:a354ab103ca4ddc5f415c0652d99651e1e0d7f42a312f52fcf02bf34e5a68daa02a13bafc7449f55798cb326232e442cac09e0ba6d94bc4b1630a80414a09af1
```

through three receipts:

```text
morphism_c_to_shape
morphism_aarch64_to_shape
morphism_x86_64_to_shape
```

The receipts are not decorative. They are the evidence that the address was reached by allowed maps rather than by wishful naming.

The asm lifters make the same point one layer lower. [provekit-lift-asm-x86-64](../../implementations/rust/provekit-lift-asm-x86-64/README.md) lists `jne`, `jnz`, and related conditional branches in its core subset and states that `jne` lowers to the same control operation as C `if` and ARM `b.ne`. [provekit-lift-asm-aarch64](../../implementations/rust/provekit-lift-asm-aarch64/README.md) lists `cbz`, `cbnz`, and flag branches such as `b.ne`, and states that `cbz` and `b.ne` recover the same operation as C's `if`, modulo carrier and primitive operations.

So the old portability claim was backwards. x86 `jne` is not portable. AArch64 `b.ne` is not portable. C `if` is not portable as syntax. The branch algebra element is portable because it is not a concrete artifact. It is the common object those artifacts realize.

The catalog discovers that object by lifting the artifacts until their differences become morphisms and their common structure becomes a CID.

## §6: The two strata, and why this does not contradict paper 9

Paper 9 said ProofIR is universal because it forgets. ORP v0.2 makes that statement precise by splitting ProofIR into two strata.

The term stratum is lossless at its abstraction level. A ProofIR term is an AST over operation CIDs. It is the implementation, expressed over abstract operations rather than concrete syntax. It is the stratum that can represent any language once that language has a signature and a lifter. It is the stratum `compile` mode consumes. It is where cross-compilation lives.

The contract stratum is lossy. It contains preconditions, postconditions, invariants, effects, resource states, signer claims, implication edges, and gaps. It is derived from terms by weakest precondition propagation, strongest postcondition propagation, effect extraction, resource-state extraction, or another accepted boundary extractor. It is where paper 9 lives.

There is no contradiction.

When this paper says ProofIR can represent any language and cross-compile, it is speaking about the term stratum. The term is a tree over operation CIDs. ORP v0.2's `compile` mode realizes that term into a target through a discharged LSP morphism:

```text
compile : Term * Target -> ConcreteCode | Refusal
```

The round-trip theorem in ORP v0.2 states that if a source lifter preserves term and contract meaning, and a target compile realizer preserves the term algebra and contract projection, then:

```text
contract(R_B(L_A(a))) = contract(a)
```

up to target representation morphisms and accepted contract-equivalence receipts. That is verified cross-compilation. It factors through the ProofIR term algebra rather than needing a point-to-point compiler proof for every source-target pair.

The factoring is the old portability dream made explicit:

```text
N source lifters + M target realizers
```

not:

```text
N sources * M targets
```

This is not a cache trick. It is LSP morphism composition over the term-algebra hub.

When paper 9 says ProofIR forgets implementation texture, it is speaking about the contract stratum. Two different terms can project to the same contract CID. That loss is deliberate. It is what lets a Spring annotation, a Zod schema, a C guard, a binary check, and a historical patch all collapse to the same boundary obligation when they truly express the same obligation.

The shape CID sits at the join. It addresses an equivalence class of contracts modulo discharged morphisms, not the encoding. The term may preserve implementation. The contract may forget implementation. The CID is stable because both strata have explicit canonicalization and explicit morphism rules.

ProofIR is universal because it has both strata and joins them:

```text
represent  : concrete artifact -> term
specify    : term -> contract
reason     : contract -> witness or implication receipt
realize    : term -> concrete artifact
```

That is the closure of represent, specify, reason, and realize. Cross-compile uses the term stratum. Federation uses the contract stratum. Both are true. They are complementary, not competing.

## §7: Lemmas L1-L6

The following lemmas state the load-bearing claims in attackable form.

### L1: Stable Primitive Address

**Statement.** Every primitive operation admitted into a language signature has a stable CID, and every language that realizes the same primitive through a discharged morphism reaches that same CID at the algebra layer.

**Proof sketch.** A primitive operation memento is serialized canonically and addressed by its CID. A language-specific construct reaches that operation only by a lifter and a discharged morphism establishing that the construct realizes the operation under the relevant signature. Canonicalization removes byte-level presentation variance. The morphism discharge removes accepted representation variance. Therefore any realization that reaches the primitive reaches the same operation CID. If it does not discharge, it does not reach that primitive.

### L2: Alphabet Plateau

**Statement.** The operation alphabet plateaus for a computational domain because there are finitely many primitive computational operations needed by that domain's signatures, even though the term space over the alphabet remains unbounded.

**Proof sketch.** A fixed language signature contains finite lists of sorts, operations, equations, and effect signatures. A fixed family of target domains adds more finite signatures. Adding languages may mint new primitives for previously unnamed semantics, but once the domain's primitive distinctions have been named, later artifacts mostly form new terms over the existing alphabet. This is the same plateau shape as paper 14: the catalog grows until the primitive axes for the snapshot flatten, then new programs are new words, not new alphabets.

### L3: Shape as Colimit

**Statement.** A shape is the colimit of its lift diagram, and the renaming, representation, and language morphisms from concrete lifts into the shape form the cocone.

**Proof sketch.** The diagram contains the concrete lifted objects and the accepted morphisms relating them. The shape object receives compatible morphisms from those lifts. Any other object that also receives compatible morphisms factors through the canonical shape because the shape is the quotient induced by the discharged morphisms and canonical bytes. The `foo` exhibit is the finite example: C, x86-64, and AArch64 lifts each map into the same shape CID after substitution and representation folding.

### L4: Monoidal Conjoining

**Statement.** The universal address space is monoidal under conjoining: the shape of `seq(foo, bar)` is `seq` applied to the shape of `foo` and the shape of `bar`, modulo the equations of the signature.

**Proof sketch.** Composition is a functor over the category of lift diagrams and morphisms. Colimits compose under the accepted composition rules. CCP conjoining and LSP operation application preserve the relevant CIDs by deterministic serialization and equation discharge. Therefore composed shapes are not separate inventions. They are compositions of prior shapes under operation CIDs such as `seq`, plus equation receipts when normalization is required.

### L5: Sound Discovery

**Statement.** A shape CID minted by lift, canonicalize, and morphism-discharge is reachable from each concrete realization in its diagram, and any two realizations that should be the same concept under accepted morphisms land on the same CID.

**Proof sketch.** Reachability is recorded by the morphism receipts from each concrete lift to the shape. Soundness follows from the discharge procedure: the canonicalizer or prove portfolio accepts only when the transformed payload equals the shape payload under the declared equivalence. Deterministic CID computation then forces equal canonical payloads to have the same CID. If two realizations "should" be the same only informally but lack a discharged morphism, the discovery procedure refuses the equality rather than guessing.

### L6: No Contradiction with Paper 9

**Statement.** Paper 16's term-stratum universality and paper 9's lossy contract-boundary universality are claims about different strata of ProofIR, and are therefore compatible.

**Proof sketch.** The term stratum is a faithful AST over operation CIDs. It is used for representation and cross-compilation. The contract stratum is a boundary projection of terms into preconditions, postconditions, effects, and related obligations. It is used for federation and implication. A shape CID at the contract layer addresses an equivalence class of contracts modulo discharged morphisms, not the full source encoding. Thus the same system can be lossless where it represents implementations and lossy where it compares boundary obligations.

## §8: What this paper does and does not claim

This paper claims four things.

First, the algebra is the portable thing. Concrete artifacts are realizations. Some are excellent hub artifacts. None is the universal object because every concrete artifact has another layer beneath it or around it.

Second, content-addressing makes algebra elements into artifacts. Once operations, equations, terms, morphisms, shapes, and contracts have canonical bytes and CIDs, the algebra can be cited and federated like any other content-addressed object.

Third, the address space self-organizes. Lifted artifacts deduplicate when accepted morphisms collapse them into the same canonical object. The map is exact at the concept layer: same concept, same CID; unsupported equality, refusal or distinct CID.

Fourth, we discover the addresses. We do not invent them by naming convention, social agreement, branding, or committee registry. The address follows from lift, canonicalization, morphism discharge, and deterministic hashing.

This paper does not claim that ProofIR-the-encoding is the algebra. ProofIR is an encoding of the algebra, plus a term stratum, a contract stratum, canonicalization rules, and protocol machinery. Confusing the encoding with the algebra would recreate the old mistake in a better costume.

It does not claim that the alphabet is complete today. The C11 mint is a start, not a final table. New operation CIDs, effect signatures, equations, and representation morphisms will be minted as the catalog reaches more languages and machines.

It does not claim that the contract stratum is lossless. Paper 9 stands. The contract stratum is powerful because it forgets implementation texture outside the boundary obligation. The term stratum carries implementation structure. The contract stratum carries the federated obligation.

It does not claim synthesis is solved. ORP v0.2 is explicit: `ProofIR contract + values -> host implementation` remains synthesis and is out of scope. `ProofIR term -> host code` is compilation and is in scope. The former asks the system to invent an implementation satisfying a boundary. The latter asks it to realize an implementation already present as a term.

It does not claim every equality is cheap. Some morphisms are canonicalizer-only. Others require solver receipts, proof assistants, compiler-correctness proofs, or future portfolio work. The claim is not that the work disappears. The claim is that the work has a stable address when done.

## §9: Closing

The ladder's lineage is now visible.

"The product is the prompts" said the value was not the static artifact users expected. The value was the editable physics that generated the artifact and could generate its successors.

"Information wants to be free" said the value was not the institution that held the bits. The value was the recomputable content once copying and verification changed the cost structure.

"The algebra is the portable thing" says the value is not the bytecode, the VM, the compiler IR, the source spelling, the instruction, the author, or even the proof bundle in isolation. The value is the realized algebra, now content-addressed.

Each move relocates the universal object away from the obvious artifact. The prompt behind the output. The bits behind the institution. The algebra behind the concrete representation.

Content-addressing is what turns that relocation into infrastructure. Without a CID, the algebra is a theory one cites in prose. With a CID, the algebra element is an artifact one can lift to, realize from, sign, cache, conjoin, discharge, and verify.

This is why the universal address space is the right name. It is the place where C, x86-64, AArch64, JVM bytecode, WebAssembly, LLVM IR, Rust, JavaScript, Python, and every future representation meet without pretending to be one another. They meet by realizing the same algebra elements and by proving the morphisms that say so.

The old dream of portability wanted one artifact to run everywhere. The new substrate says something stricter and more useful: no artifact has to be universal. The algebra is.

JNZ isn't portable. The algebra is.

T Savo
