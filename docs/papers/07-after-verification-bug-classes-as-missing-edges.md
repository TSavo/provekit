# After Verification: Bug Classes as Missing Edges in the Federated Proof Substrate

> **Status.** Sustained argument. Contains a theorem with constructive proof. Engages counterarguments. Written to be cite-able.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [03 Substrate, not Blockchain](03-substrate-not-blockchain.md), [04 Vertical Stack and Standardization](04-vertical-stack-and-standardization.md), [05 Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md).
>
> **Premise the earlier papers established.** A protocol for content-addressable, cryptographically-signed, byte-deterministic claims about software behavior, federated across signers, composable end-to-end, jurisdiction-neutral, and machine-checkable. After Reputation argued that the consequence of shipping that protocol is the substrate replacing reputation as the load-bearing trust mechanism in software.
>
> **What this paper argues.** That the same substrate, once you also ship inverse-direction droppers and let the lifter and dropper close their loop over weakest-precondition propagation, makes leaf-discharge bug classes structurally impossible. Not caught at runtime. Not caught at compile time as a heuristic. Structurally impossible, in the sense that no path through the program's data-flow DAG can reach a sink with an undischarged precondition without compile failure. We prove this. The proof is constructive and short. The substrate's accumulated lemma cache turns the proof obligation tractable by amortizing verification work across the entire ecosystem.

## §0: The claim

Today, bug-class elimination is heuristic. Static analyzers find some bugs and miss others. Type systems catch a narrow band. Test suites cover what they cover. Whole-program verification efforts (seL4, CompCert) take 200 person-years and produce a single artifact whose scope is a microkernel's worth of code. The substrate of correctness assurance is fragmented across tools whose results are ephemeral, unsigned, and incompatible with each other.

This paper argues that the substrate Sugar ships, once droppers close the loop over weakest-precondition propagation, retires that fragmentation by making bug-class elimination a theorem rather than a heuristic. Specifically: every leaf-discharge bug class (input validation, use-after-free, null deref, data races on lock-holding properties, and most of the OWASP Top 10) is structurally eliminated under a sound substrate, and the proof is by induction on data-flow path length over a content-addressed federated DAG of cached one-step implications.

The substrate's accumulated DAG IS the proof. Verification of any program is graph reachability over the cached edges. New verification work mints new edges. Edges are content-addressed, signed, and shared across the entire ecosystem. The cost of verifying any new program asymptotically approaches the cost of cache lookups, because most proof obligations have already been discharged by someone, somewhere, for some other program.

We are after verification as it has been understood for fifty years. The substrate makes verification compose. What follows is what falls out.

## §1: Verification today

Verification in 2026 is research, not infrastructure.

A correctness-assurance team at any non-trivial software company runs five or six tools, each producing a separate model of what is true. The static analyzer flags some patterns. The fuzzer finds some inputs. The property-based test generator covers a sample of input space. The type checker enforces a thin band of structural shapes. The SMT-backed verifier (if there is one) handles annotated subsets of the code. The audit team manually reviews the rest. Each tool's output is ephemeral: it lives for the duration of one CI run, one analysis pass, one engineer's session. Nothing aggregates.

When a CVE is reported, the team starts over. The fix involves new annotations, new test cases, new analyzer configurations, possibly a new tool. The process for the next CVE is identical, with no benefit accrued from the last one. Static analyzers cache nothing across builds. SMT solvers cache nothing across sessions. Proof assistants cache nothing across organizations. Coverity is a closed silo; CodeQL is a closed silo; the verifier on the seL4 project is a closed silo. The work that has historically been the most cognitively expensive in the entire industry, finding and proving the absence of bugs, is thrown away every build.

The whole-program verification efforts that DO produce durable artifacts are the exception that prove the rule. seL4's proof is real, durable, audited, cite-able. It cost about 200 person-years and the proof artifact applies to a microkernel of about 8,700 lines. The proof is monolithic: changing the kernel requires re-proof. The proof is closed: applying its insights to FreeBSD's kernel or Linux's KVM subsystem is not a transfer operation; it is restarting the project. CompCert is the same shape, applied to a C compiler. Both projects are research achievements. Neither scales.

The substrate of correctness assurance, viewed honestly, is a dozen non-interoperating tools whose outputs evaporate after each invocation, plus a handful of heroic research efforts whose outputs do not transfer.

Static analysis as a discipline knows about this problem. Sparse, Smatch, Coverity, Infer, and CodeQL exist precisely because the rest of the verification stack is too expensive for daily use. They make trade-offs explicitly: they sacrifice soundness or completeness or both, in exchange for being fast enough to run on every commit. They are extraordinarily useful, and they are also fundamentally heuristic. They produce findings, not proofs. Their findings are not signed, not content-addressed, not composable across tools, and not durable beyond a single build.

This is the substrate we are leaving behind.

## §2: The shift

Replace verification-as-discipline with substrate-as-DAG.

Three pieces have to be in place. None of them are individually new; what is new is that all three are simultaneously cheap enough to run on every save and compose into a federated cache.

**Lifters and droppers close a loop.** A lifter reads native code (Spring annotations, Verus invariants, Pydantic validators, Linux's BUG_ON macros, every if-statement a developer ever wrote) and emits content-addressed contract mementos. A dropper reads contract mementos and emits native code in the host language (parameterized queries, lock acquisitions, bounds checks, null guards, sanitization wrappers). The lifter sees what is there. The dropper writes what is missing. Together they make the substrate active, not just observational.

**Weakest-precondition propagation is the algorithm.** Dijkstra published `wp` in 1975. For any program statement S and postcondition Q, `wp(S, Q)` is the weakest precondition that, combined with executing S, guarantees Q. WP at a function call is mechanical: substitute the actual argument expressions for the callee's formal parameters in the callee's precondition. The walk is deterministic, finite (rooted at allocations), and decidable structurally with implication-checking deferred to solvers. WP propagation through the AST yields, at every arrival, the conjunction of facts known at that point. Each arrival's local proof obligation is a one-step implication: the WP at this arrival implies the precondition demanded here.

**Content-addressed predicate identity makes the cache federated.** Every predicate canonicalizes to JCS bytes and content-addresses to a BLAKE3-512 CID. Two predicates that mean the same thing in different languages, written by different authors, in different decades, hash to the same CID once lifted to the canonical IR. Their identity is the hash. Implications between predicates are also content-addressed: an edge `p → q` has CID `hash(p-CID, q-CID)`: the endpoints alone determine the edge's identity. Witnesses are mechanically-verifiable proof artifacts (Z3 unsat core, Coq term, Vampire saturation, CVC5 proof, hand-derived implication chain) carried *inside* the edge memento as one or more `proofData` entries, each independently signed. Multiple witnesses for the same logical implication coexist on one edge as alternate `proofData` slots; consumer policy decides which to accept. Every witness must be checkable from its bytes alone, independent of who or what produced it. Every edge has a stable endpoint-determined address. Every witness is signed. Every edge is shareable.

The shift is from substrate-of-tools to substrate-of-cached-edges. Verification is no longer "run the analyzer." It is "look up the edge in the substrate; if missing, mint it locally; sign the result; add it to the substrate." Every developer's verification work, at every save, contributes to a global lemma cache that grows monotonically. Every other developer's verification work, on every other program, becomes available the instant their lemmas land.

Reputation moved from the substrate to the policy layer in After Reputation. Verification moves from the discipline to the substrate here. The substrate's job becomes mechanical: cache edges, look them up, mint missing ones, compose. Discipline becomes the consumer's call: which signers' edges do you accept, which solvers' witnesses do you trust, which patches do you apply.

## §3: The substrate's algebraic shape

Stating the algebraic frame matters because the substrate's properties are not pitched, they are derivable.

The substrate is a thin Heyting category. Its objects are content-addressed canonical IR predicates. Its morphisms are content-addressed one-step implications: each morphism's identity is the CID `hash(source-CID, target-CID)` derived from its endpoints alone. The morphism carries one or more witnesses inside its memento (alternate `proofData` entries from different verifiers), but the morphism's identity is endpoint-determined, not witness-determined. Composition is endpoint composition: given an edge `p → q` and an edge `q → r`, the composed edge `p → r` has CID `hash(p-CID, r-CID)`. Composition is trivially associative because endpoint composition is: `(p→q ∘ q→r) ∘ r→s = p→s = p→q ∘ (q→r ∘ r→s)`, and both sides hash the same `(p-CID, s-CID)` pair. Identity is the trivial implication `p → p`, content-addressed as `hash(p-CID, p-CID)`. The category is "thin" because between any two predicates `p` and `r` there is at most one morphism, valid or absent: implication either holds or does not, with no morphism multiplicity. Multiple witnesses on a single edge are not multiple morphisms: they are alternate proofs of the same morphism's existence.

Thin categories are equivalent to preorders. The substrate is a content-addressed federated preorder over predicates, equipped with the logical operations `∧`, `∨`, `¬`, `→` that make a Heyting algebra. The substrate is the Heyting algebra of all reachable predicates, with cached implication witnesses as the morphisms.

This is not decoration. The categorical frame is what makes the substrate's properties algebraically derivable rather than empirically hoped for.

**Sharing is automatic.** Two arrivals with the same content-addressed DAG share the proof byte for byte. Two programs with overlapping data-flow shapes share their overlapping sub-proofs. The substrate amortizes verification across the entire ecosystem because morphism equality is hash equality.

**Composition is free.** Edge composition is O(1) hash combination. Verifying a composed proof is verifying its constituent hashes plus the composition step. No re-derivation. The substrate's "verification" of a chain of cached edges is graph reachability, not theorem-proving.

**Federation is the absence of global structure.** A category has no central authority beyond its objects, morphisms, and composition law. Two parties independently minting the same edge produce byte-identical CIDs. The substrate has no central registry; the cache is a distributed hash table of edges, each verifiable from its bytes alone.

**Universal quantification gives leverage per byte.** Lifted implications are not between fixed predicates; they are between families parameterized by free variables. `if x < 10 throw` lifts to `∀x. (x_unconstrained_at_callsite → x ≥ 10_required_for_body_to_proceed)`. One cached edge applies to every caller of this function, every value of x, forever. The cache's effective coverage is asymptotically infinite per byte stored.

**Closure under operations.** Get-or-mint is the substrate's primitive: given an edge address `e = hash(p, q)`, look it up; on miss, run a verifier and mint it; insert into the substrate; return the witness. The substrate is closed under its own operations. Every miss feeds the cache. Every hit costs nothing.

The substrate is mathematically clean. It is not a clever data structure. It is the right algebraic shape for the problem of correctness as composition.

## §4: Arrivals, not functions

The unit of verification in every prior approach has been the function. seL4 verifies function by function. CompCert verifies function by function. Verus, F*, Dafny, Frama-C, KeY, Spec#, Cogent, Boogie: function by function. Sparse and Smatch operate per-function with limited inter-procedural reach. Coverity and CodeQL inflate to whole-program but still bottom out at function-summary primitives.

The substrate operates below the function abstraction.

A function has one body, but does not have one contract. The body discharges a single most-general (pre, post) pair, which is a lossy summary. At every callsite, the function's effective contract is specialized to the actual arguments and to what the calling context already knows. Different callsite, different specialization, different proof obligations. Worse: the consequences at a callsite are not determined by the function alone; they are determined by what the caller needs next. The function provides a postcondition. What gets proved at the callsite is the implication `(context-in ∧ post[args]) → context-out`, where `context-out` is dictated by the caller's downstream code. The function does not have "the contract." The callsite does, and every callsite has its own.

The substrate's primitive is the arrival. An arrival is a pair `(AST location, accumulated WP at this location)`. Every callsite is an arrival. Every branch is an arrival. Every merge point is an arrival. Every loop iteration is an arrival. Every assignment, every assertion, every conditional guard, every operator: each is an arrival. At each arrival, the accumulated WP implies a context-dependent bundle of consequences that hold at this specific point. The bundle is determined by what comes next, not by what the function "always implies."

Functions do not appear in the substrate. They never did. They are an editing abstraction, a naming convenience, a unit of code organization above the substrate. The DAG sees through them to arrivals. Two callsites of the same function with identical surrounding contexts produce identical arrivals; the cache hits. Two callsites of the same function with different contexts produce different arrivals; the cache stores both. The function never appears in the lookup.

This dissolves a class of problems prior verification approaches have spent decades working around. Higher-order functions, closures, currying, dependent types, effects, generators, coroutines, async/await, monadic bind, callcc, lazy evaluation, exception handlers, continuations, algebraic effects: the substrate sees through all of them. They are language-level constructs that produce AST arrivals; the substrate verifies arrivals. Whatever exotic flow-control a language has, it bottoms out into arrivals with accumulated WPs. The substrate verifies at that depth, below any language's function abstraction.

This is what makes the substrate universal across the entire computing surface. Not "twelve language kits." Not "every language supported." The substrate is below the level at which languages differ. Rust's `unsafe`, Java's `volatile`, C's pointer arithmetic, Python's `__getattr__`, Haskell's monadic bind, Verilog's `always` block, an eBPF program, a SQL query, a shader: all decompose into arrivals. The substrate verifies at that depth, for all of them, with shared cache entries wherever the data-flow shapes coincide.

### §4.1: Arrivals are enumerable: the completeness lemma

The set of arrivals is constructive. For any program in any language, the set of interesting arrivals is precisely the Cartesian product `Allocations × Reads`. Every fact about every value originates at exactly one allocation site, the SSA definition that bound the variable. Every use of that value happens at one or more read sites. The pair `(allocation, read)` is one arrival. The set of all such pairs is the set of all interesting arrivals.

"Interesting" scopes to facts that surface in the lifted canonical IR. Three families that look like exceptions reduce cleanly to the same enumeration. Control-flow facts surface as φ-merge values: a path-conditional fact "X holds on this branch" is the SSA-level fact about the φ-value carrying the path condition, with allocation site at the φ and reads at the branch's downstream uses. Boundary-axiom facts (network reads, syscall returns, attacker-controlled input) surface as signed edges from boundary nodes whose allocation site is the lifter's signed boundary mint, not user code. Chained facts (a fact derived by composing two cached implications) surface via composition rather than enumeration: composition adds no new allocation, only a new edge between two arrivals already in the product. Compiler-internal intermediate values that never surface in the lifted IR are by construction not interesting; the IR canonicalizer is the cut.

This set is finite, bounded by the SSA size of the program. It is enumerable: walk every definition, enumerate every use. It is exhaustive: every fact has exactly one origin and finitely many destinations. The substrate's verification coverage at the arrival level is therefore total, not heuristic. Every `(allocation, read)` pair gets a verified DAG path or has its proof obligation flagged as a missing edge. There is no "we sampled the inputs" and no "we covered the common paths." The enumeration is by construction.

Across platforms, across domains, this holds without modification. The IR canonicalizes language differences, instruction-set differences, execution-context differences. The Cartesian product is computed in the lifted canonical IR, where Rust's `unsafe`, Java's `volatile`, C's pointer arithmetic, x86 and ARM and RISC-V and WASM, kernel space and userspace and embedded firmware, are all the same IR. The set of interesting arrivals is the same set, regardless of platform, regardless of domain.

This is the completeness lemma that makes the theorem of §5 total. The theorem claims structural elimination for every reachable `(source, sink)` pair. The set of `(source, sink)` pairs IS the set of `(allocation, read)` pairs filtered to those whose source is a boundary axiom and whose sink has a leaf precondition. The set is finite. The enumeration is exhaustive. The theorem's coverage is everything.

This is what separates the substrate from every prior approach to bug-class elimination. Static analyzers find some `(allocation, read)` pairs and miss others. Fuzzers sample input space and miss what they do not happen to hit. Tests cover what they cover. The substrate enumerates `(allocation, read)` pairs constructively, verifies each, and reports every missing edge. Coverage is not a measurement after the fact; coverage is the size of the Cartesian product, computed at lift time, exhaustive by construction.

## §5: The theorem

The structural-elimination claim is a theorem, provable by induction on path length over the cached DAG.

**Theorem (Structural Elimination of Leaf-Discharge Bug Classes).**

Let `G = (V, E)` be the cached data-flow DAG of a program where:

- `V` is the set of arrivals.
- `E` is the set of one-step WP implications, each independently verified and signed.
- `B ⊂ V` is the set of external-boundary axiom nodes (network reads, file reads, syscall returns, user input), each signed with a provenance predicate (e.g., `untrusted`).
- `S ⊂ V` is the set of sink nodes (security-sensitive operations, dereference points, lock acquisitions), each with a signed leaf precondition (e.g., `requires sanitization`, `requires alive`, `requires lock_held`).

Suppose every edge in `E` is sound under the IR's semantics: for every cached edge `p → q`, the implication holds in the substrate's logic.

**Claim.** For any reachable pair `(source ∈ B labeled untrusted, sink ∈ S requiring safe)`, the program either contains a path `source → ... → sink` in which some edge strengthens the WP from `untrusted` to `safe` (a sanitization edge), or the program fails to compile.

**Proof.** By induction on path length `n`.

*Base case (`n = 1`).* A direct edge `source → sink`. The sink requires `safe`. The source provides `untrusted`. Edge soundness requires `untrusted → safe` to be a valid implication for the edge to exist. The base case rests on a curatorial obligation that is part of the foundation baseline catalog: **the canonical predicate vocabulary forbids any signed witness for the implication `untrusted → safe` directly. Sanitization edges must explicitly strengthen the WP via a named sanitization predicate** (e.g., `sanitize_for_sql`, `escape_for_html`, `validated_against_schema`). The catalog therefore contains witnesses for `untrusted → sanitized_for_X` and for `sanitized_for_X → safe_for_X` (both auditable as discrete signed lemmas), but never a direct `untrusted → safe` collapse. Predicate distinctness (content-addressing makes `untrusted`, `sanitized_for_X`, and `safe` different CIDs in a sound canonical IR where the predicate vocabulary has no spurious equivalences) ensures these are three distinct objects. The catalog's refusal to sign `untrusted → safe` directly ensures no such edge is reachable. Therefore the direct `source → sink` edge does not exist in a sound substrate. The precondition is undischarged. The compiler's contract gate refuses. Compile fails.

*Inductive case (`n + 1`).* Assume the property holds for all paths of length `≤ n`. Consider a path of length `n + 1` ending in `v_n → sink`. The WP at `v_n` must imply `safe` for the sink's precondition to discharge. By the inductive hypothesis applied to the path from `source` to `v_n`, either that path contains an edge that strengthens the WP from `untrusted` to `safe` (in which case the full path of length `n + 1` contains it transitively, satisfying the conclusion), or `v_n` still carries `untrusted` as its accumulated WP. In the latter case, the final edge requires `untrusted → safe`, which by the base-case argument does not exist in a sound substrate. The precondition is undischarged at the sink. Compile fails.

QED.

The theorem generalizes. Replace `untrusted → safe` with `freed → alive`: use-after-free is structurally eliminated. Replace it with `unlocked → lock_held`: data races on lock-holding properties are structurally eliminated. Replace it with `null_possible → null_checked`: null deref is structurally eliminated. Replace it with `unbounded → bounded`: integer overflow at security-sensitive operations is structurally eliminated. The proof is identical; only the predicate names change.

The theorem covers, by direct application, the leaf-discharge subset of the bug landscape. SQL injection, XSS, command injection, path traversal, SSRF, deserialization-of-untrusted, JNDI-lookup-on-attacker-string, prototype pollution, log injection, header injection, use-after-free, double-free, refcount mismatch, type confusion, missing capability check, missing authentication, missing authorization for sensitive sinks, lock-ordering violations whose sinks have known lock-holding preconditions, unsafe-cast-to-trusted-type. This is most of the OWASP Top 10. It is most of the kernel CVE corpus. It is most of what attackers exploit.

The theorem does not cover bug classes that are not leaf-discharge in shape: logic errors (computation wrong, no precondition violated), performance bugs (correctness met, time or space wrong), specification bugs (the precondition itself is wrong), concurrency bugs not reducible to lock-holding (ABA, missed wakeups, incorrect lock granularity, livelock). These require richer specifications than the leaf-discharge frame admits. The theorem is honest about its scope.

The theorem's proof obligations divide cleanly:

**Mechanical obligations.** (1) Sound WP at the IR level, discharged by the substrate's CDDL grammar plus the multi-solver portfolio's coverage. (2) Sound lifters, discharged per kit by byte-equivalence golden fixtures plus the kit author's signed claim. (3) Predicate distinctness from content-addressing, discharged by JCS canonicalization plus the predicate vocabulary having no spurious equivalences.

**Curatorial obligations.** (4) External boundaries are correctly tagged at the source. (5) Security-sensitive sinks have correctly-specified leaf preconditions. (6) The foundation baseline catalog signs only sanitization edges that go through named sanitization predicates (e.g., `untrusted → sanitized_for_sql → safe_for_sql`), never collapsed direct edges (`untrusted → safe`). This is the explicit no-shortcut rule that makes the base case sound.

The mechanical obligations are properties of the substrate. Curatorial obligations are signed claims by humans. Once curation is correct, structural elimination is automatic.

## §6: Composition is free

The substrate's growth profile is the heart of the practical claim.

For any new proof obligation `p → q` where `q` decomposes as `q = a ∧ b ∧ c ∧ ...`, the verification reduces to:

1. Decompose `q` into conjuncts.
2. For each conjunct `c`, `ensure_edge(p, c)`: lookup at CID `hash(p, c)`; on miss, mint locally and insert.
3. Verify `p` is consistent: `p → ⊥` is not in the substrate. (If it is, the program has a contradiction; flag it.)
4. Verify no contradictions: for each conjunct `c`, `p → ¬c` is not in the substrate. (Otherwise `p` contradicts `q`'s demand.)
5. The composed proof of `p → q` is the conjunction of the per-conjunct edges.

Each cached `p → c` is a free contribution. Only missing conjuncts cost mint work. The substrate's effective coverage of any new proof obligation is `(cached_conjuncts / total_conjuncts) × 100%`. Year 1: near 0%. Year 20: near 100%. The cost of verification asymptotically approaches the cost of lookup.

Combined with universal quantification, the leverage compounds. Each cached edge is universally quantified over its free variables; one edge covers all instantiations. The cache grows polynomially with distinct universal patterns. The program space grows exponentially with concrete instantiations. The ratio is the substrate's compounding return.

The substrate is the global lemma cache of all theorem provers, ever. Z3 has an internal lemma cache, scoped to a single solving session. Coq has one, scoped to a session. Vampire has one. CVC5 has one. Every static analyzer has its own. They are ephemeral. Sugar makes them public, federated, monotonic. Every cached edge is a lemma some solver minted, that no other solver ever needs to mint again. Z3's work survives Z3's session. Coq's work survives Coq's session. The work that has historically been thrown away every build, every CI run, every research paper, is preserved, content-addressed, and shared.

Software stops aging because the substrate accumulates faster than novel proof shapes appear. Every if-statement, every assertion, every type-narrowing, every match arm, every lock acquisition, every bounds check in every program ever lifted contributes one or more cached edges. The cache grows monotonically. Coverage grows monotonically. The cost of verifying new code asymptotically approaches the cost of computing CIDs, which is the cost of one BLAKE3 hash, which is what compilers pay anyway.

### Every step is mechanical

Every operation on the substrate is a deterministic algorithm with no model, heuristic, or learned component in the soundness chain. Walking MIR backward from a callsite is iteration. WP transformation at each statement is substitution. Canonicalization is JCS. Content-addressing is BLAKE3. Cache lookup is hash equality. Minting on cache miss dispatches to a solver (Z3, Coq, Vampire, CVC5) which produces a witness verifiable from its bytes alone. Signing is Ed25519 over canonical bytes. Verification of a composed proof is graph reachability over signed edges. Each step is finite, decidable, and checkable independently of who ran it.

This matters because the substrate's correctness must not depend on the trustworthiness of any model. Models are useful at the curation and authoring layer (suggesting boundary labels, drafting baseline-catalog predicates, proposing which cached witness fits a particular gap when several do, generating candidate code to close DAG gaps). Their suggestions then enter the substrate through the mechanical path: the candidate is lifted, the WP is computed, the gap is identified, the cached witness is checked, the edge is signed. Models accelerate authoring; they never participate in verification. The substrate's claim that "bug classes vanish structurally" reduces to a chain of finite mechanical operations, all inspectable, all signable, none requiring trust in any non-deterministic component.

## §7: Generative completion

The substrate is not just observational, and not just active. It is generative. It computes what is missing and writes the code that supplies it.

The mechanism is direct. Given a lifted program with an undischarged precondition at sink `S`, the substrate computes the gap: `(accumulated_WP, required_precondition)`. The gap has a CID: `hash(accumulated_WP_CID, required_precondition_CID)`. Look it up. On hit, the cached witness includes per-language drop shapes: how to express this strengthening edge in each kit's native language. The dropper picks the host's shape and inserts at the gap's location. On miss, the verifier mints locally; the new edge enters the substrate; future programs reuse for free.

For your function `f(x)` whose body is `if x < 10 throw; db_query(x)`:

The lifter produces an edge from `f`'s parameter binding (allocation site for `x`) through the branch (which introduces `¬(x < 10) = x ≥ 10` in the non-throw path) to the callsite at `db_query(x)`. The function's effective precondition, lifted from its body, is `x ≥ 10`. A caller doing `f(y)` where `y` is unconstrained produces a missing edge: `unconstrained(y) → y ≥ 10`. The substrate looks up that CID. Common cached witnesses:

- `if y >= 10 { f(y) }` (guard)
- `assert(y >= 10); f(y)` (assertion)
- `f(max(y, 10))` (saturating clamp)
- `f(if y < 10 { return } else { y })` (early return)

The dropper picks one based on codebase convention, language, and developer preference. It inserts the native code at the gap's location. Re-lift confirms the DAG closes. The bug, if there was one, is mechanically fixed.

This generalizes to every leaf-discharge bug class. SQL injection: gap is `untrusted → sanitized` at a `db.query` callsite; cached witnesses include parameterized queries, prepared statements, and named-parameter binding; the dropper writes the host language's idiom. Use-after-free: gap is `freed → alive` at a deref; cached witnesses include pre-deref liveness checks, RAII guards, refcount bumps; the dropper writes the appropriate one. Lock ordering: gap is `unlocked(L) → lock_held(L)` at a sink; cached witnesses include lock acquisitions, RAII lock guards, atomic compare-exchange paths; the dropper writes them.

The compiler becomes a fix generator. Compile failure no longer says "error: cannot prove `p → q` at line 47." It says: "cannot prove `p → q` at line 47; substrate has three cached witnesses that close this gap: `escape_sql(arg)`, `parameterize(arg)`, `prepared_statement(arg)`. Pick one." The developer chooses; the compiler inserts; the build succeeds.

CVE remediation becomes mechanical. A new leaf precondition published by Linux upstream is a new edge requirement at every reachable sink. The substrate computes which existing functions now have missing edges; it looks up cached witnesses for each gap; it generates patches; it signs them; it distributes them via the substrate's content-addressed channel. Every running instance of the affected version verifies the patch's witness chain locally and applies if accepted.

Code review becomes proof review. Instead of "did the developer write good code," the question is "did the developer's edges close the DAG, and are the signed contracts the right ones for what this code is supposed to do?" The substrate reports DAG closure mechanically; reviewers spend their attention on business judgment, not on chasing null checks the substrate already inserted.

AI-generated code becomes verifiable in the strong sense. The AI generates a candidate. The lifter reads the candidate. The substrate checks mechanically: does this close the DAG? On yes, the AI's output is provably correct (modulo curation), where "provably" means a chain of signed edges with verifiable witnesses, not the model's confidence in its own work. On no, the substrate names every missing edge; the AI tries again with the gaps as feedback. AI as contract-implementation generator stops being aspirational rhetoric and becomes operationally meaningful: it is the consequence of the substrate plus a code-generating model trained to close DAG gaps. The model's role is candidate generation; the substrate's role is mechanical verification; the two roles do not blur.

## §8: The Linux empirical case

The Linux kernel is the most-deployed software in human history. It runs on every Android phone, every cloud server, every embedded device, every autonomous vehicle running Linux. It is approximately 30 million lines of code and grows by hundreds of thousands of lines per year. Whole-kernel verification by any prior approach is not on any plausible roadmap.

The kernel also contains forty years of accumulated leaf assertions, written by tens of thousands of contributors, in nearly every commit of its history. `BUG_ON`, `WARN_ON`, `WARN_ON_ONCE`, every sparse annotation (`__user`, `__rcu`, `__bitwise`, `__must_check`, `__must_hold`, `__acquires`, `__releases`, `__force`, `__iomem`, `__percpu`), every lockdep `assert_held`, every refcount `BUG_ON`, every capability check (`if (!cap_capable(...)) return -EPERM`), every bounds check (`if (size > limit) return -EINVAL`), every input validation at the syscall boundary. Hundreds of thousands of signed-shaped predicates, written for forty years by people who thought they were just writing safe C and who, on the substrate's reading, were writing leaf preconditions for a global proof.

Lift them. Propagate them. Drop them.

Every syscall entry point, under leaf-propagation, gains a computed contract. Mechanically, from the conjunction of every leaf precondition reachable through that syscall's call graph. No human writes the syscall contract. The substrate composes it from the leaves. The kernel ABI becomes a contract surface. Userspace verifies its calls before invocation. Distributions sign per-version contracts. kABI breakage detection becomes algebraic.

Lock ordering becomes compile-time. lockdep currently catches violations at runtime, after the fact, often months later in syzkaller fuzzing. With `__must_hold` propagation, every function gains a computed lock-state precondition. Lock-order bugs become "won't compile."

Use-after-free vanishes structurally. `requires alive(p)` propagates through the call graph from every dereference. Every path that reaches the dereference must guarantee aliveness or fail to build. Same shape for double-free, refcount mismatch, type confusion. The entire memory-safety bug class becomes structurally unreachable, not by a Rust rewrite of the kernel, but by leaf-propagation over the existing C.

eBPF verification becomes thin. The eBPF verifier today does heroic abstract interpretation because it has no contracts to match against. With droppers, kernel helpers ship signed contracts; eBPF programs ship signed contracts; verification is contract discharge, not whole-program AI. The same shape applies to kprobes, tracepoints, and BPF LSM.

Driver authors write at the leaf, get sealed automatically. A new PCIe driver's authors add the safety assertions appropriate to their device. Propagation extends to every interrupt handler, every syscall path that eventually touches the device. The "did we miss a path" question dissolves: propagation finds them all.

Whole-kernel verification, which has been impossible by every prior approach, becomes incremental and compositional. seL4 took 200 person-years to verify a microkernel's worth of code. Sugar-on-Linux does not ask anyone to do whole-kernel verification. It asks the substrate to read what kernel devs have already written, propagate it mechanically, and accumulate the resulting edges in the federated DAG. Each commit's contribution is small; the cumulative coverage is monotonically growing; the verified region becomes the union of every successfully-discharged proof obligation. After ten years of community lifting, the verified surface is most of the kernel. After one year, it is the high-traffic syscall paths. Coverage is strictly monotone in either case.

If droppers work on Linux, the foundation of computation becomes structurally provable, retroactively, from work humans already did. Then FreeBSD. Then XNU. Then NT, if Microsoft signs. Then every embedded RTOS whose vendor cares about the SBOM. The kernel ecosystem under every running application becomes contract-verified. Application contracts then mean something because the kernel they run on stops being the unverified weak link.

This is what makes the substrate's claim civilizational rather than tooling-shaped. The substrate's accumulating DAG eats fragmented verification across the entire computing surface. The fragmentation does not survive content-addressing.

## §9: Why now

The pieces have been around a long time. None of them are new.

- Hoare logic dates to 1969.
- Dijkstra's weakest preconditions were published in 1975.
- Cousot & Cousot's abstract interpretation was published in 1977.
- Content-addressable storage emerged from research in the 1980s, was operationalized in the 1990s, and became cultural default with Bitcoin (2009), Git (2005), and IPFS (2015).
- Cryptographic signatures became practical in the 1990s; Ed25519 made signature checks effectively free in 2011.
- BLAKE3 (2020) made hashing on every keystroke invisible.
- SMT solvers (Z3 in 2007, CVC5 in 2022) made implication-checking accessible to non-specialists.
- Multi-solver portfolios (using whichever solver is best for the shape) became standard practice in the 2010s.

Every piece has been shippable, separately, for years or decades. The IDEA of "every program ships with a federated proof DAG that grows monotonically as new edges are minted" is not new. So why has nothing like it shipped before?

Three constraints relaxed roughly simultaneously in the past five years.

**Hashing became cheap enough to run on every save.** BLAKE3 changed the cost-of-substrate floor. Hashing every AST node, on every keystroke, in real time, in a code editor, is invisible. Verification on every install is invisible. The substrate's marginal cost dropped below the threshold at which it becomes friction.

**AI became capable enough to author baseline lemmas at scale.** Hand-authoring a baseline catalog for one language's standard library is a multi-decade project. Hand-authoring leaf preconditions for the OWASP Top 10 across twelve languages is decades. AI authoring of the same material, supervised and signed by humans, is days. The bottleneck on substrate adoption was always "who writes the lemmas." That bottleneck dissolved when AI got good enough to do the writing.

**Federated content-addressing became cultural default.** Bitcoin made hash-as-identity default mental model in 2009. Git made it default mental model in software engineering. IPFS made distributed content-addressing operational. The cultural acceptance of "the bytes ARE the identity" finally caught up to the cryptographic possibility. Engineers no longer have to be sold on the idea; they have lived with it for a decade.

These three constraints relaxed in the same window. Combined with the protocol-level work (canonical IR, JCS canonicalization, CDDL-grounded type system, multi-solver consensus, signed mementos, .proof catalogs) detailed in earlier papers, the substrate is finally cheap enough that "every save lifts, every save composes, every save mints what is missing, every save signs, every load verifies" stops being a performance issue.

The window for a single content-addressable substrate that federates verification across every language is now. Five years from now, every major language will have its own ad-hoc internal version, and they will not interoperate. The window for one substrate is open and finite.

## §10: Counterarguments

This section engages the obvious objections seriously. Each is real; each has a response; some responses are partial.

### "Abstract interpretation has been tried; it doesn't scale."

Abstract interpretation as an internal tool inside compilers and analyzers (sparse, smatch, Coverity, Infer, CodeQL) does scale. It runs on Linux, on Chromium, on millions of lines of production code. What does not scale is whole-program verification with hand-authored full functional correctness proofs (CompCert, seL4). The substrate this paper describes is the former, not the latter. The substrate uses abstract interpretation primitives (WP, lattices, monotone propagation) precisely because they scale. What is new is making the results persistent, content-addressed, and federated. Existing analyzers are silos; the substrate connects them.

### "Cousot 1977 with a database is not a paper-worthy contribution."

The mathematical novelty is small. The engineering novelty is large. Cousot's framework was developed as a per-build, per-tool, per-session local analysis. Lifting it to a federated content-addressed substrate is the engineering work, not the mathematics. Paper 06 made the analogous argument for content-addressing as a discipline shift. This paper extends it to verification. The math is fifty years old. The substrate is new.

### "What about non-leaf-discharge bug classes?"

The theorem's scope is honest. It covers leaf-discharge bug classes, which are most of the OWASP Top 10 and most of the kernel CVE corpus. It does not cover logic errors, performance bugs, specification bugs, or concurrency bugs not reducible to lock-holding. These remain in the domain of richer specification work (full functional correctness, separation logic, temporal logic). The substrate provides infrastructure for richer specifications when authors write them; what changes is that even the richer specifications get cached and federated. The substrate is a floor, not a ceiling.

### "The contracts will be wrong."

Some will be. The substrate's response is the same as After Reputation's: contracts are content-addressed, signed, and pinable. A buggy contract has a CID; a corrected contract has a different CID; consumers can pin either or migrate. The substrate makes contract correctness an iterative engineering activity rather than a one-shot bet. The cached lemma cache also makes spotting wrong contracts easier: a contradictory lemma stands out because it appears as `p → ⊥` in the substrate, flagged.

### "The verifier could be unsound."

The verifier's witnesses are content-addressed. A buggy verifier produces buggy witnesses. The substrate's response is to support multiple verifiers in a portfolio: Z3, Coq, Vampire, CVC5, and (for shapes none of these handle) hand-authored derivations. When verifiers disagree on whether `p → q` holds, the substrate carries both witnesses and lets the consumer's policy decide which to trust. This is exactly the multi-solver-protocol-v2 framework detailed in earlier papers. Verifier unsoundness becomes a contract about the verifier (which versions are trusted) rather than a global crisis.

### "WP propagation can't handle real programs (aliasing, side effects, concurrency)."

WP propagation in its raw 1975 form does struggle with these. The substrate's response is to extend WP into the language constructs that real programs use: separation logic for aliasing, atomicity contracts for concurrency, effect systems for side effects. Each extension is a shape the lifter recognizes and emits as part of the IR. The substrate is agnostic to which extensions are in use; it caches whatever edges the lifter produces. Authors of richer extensions become signers in the substrate; their work composes with simpler signed edges. Soundness across extensions is the proof obligation each extension's signer takes on.

### "Adoption: nobody will refactor to use this."

They do not have to. The substrate reads what is there. Existing if-statements, asserts, type signatures, and framework annotations lift to leaf preconditions. Existing code does not change. Existing developers do not learn a new language. The lifter is invisible. The dropper inserts at gaps. The substrate runs in the build system's cache layer, alongside existing tools. Adoption is "do nothing differently and let the substrate read your code." The asymmetric adoption pressure is the same as After Reputation's: the first language whose stewards sign canonical baseline catalogs makes that language demonstrably more trustable than the others.

### "Content-addressable substrates have been promised before and underdelivered."

True. The response is twofold and identical to After Reputation's. First, the substrate is asking for tooling that fits inside existing engineering practice, not cultural transformation. Developers do not need to learn a new mental model; they need to opt into a verification step that runs invisibly. Second, the substrate solves a problem developers actually have (verification fragmentation, supply-chain trust, brittle CVE response) rather than a problem cryptographers wish developers had. The match between substrate and demand is tighter than for the prior substrate-shaped promises.

It may still fail. Predicting cultural adoption is hard. But the failure modes of "developers do not want this" are different from the failure modes of "the technology cannot ship." The technology ships. The cache grows. The theorem holds.

## §11: What you'd reach for first, and why you don't need to

If you read the substrate's claim and then look at the v1.5.0 protocol, you might reach for four things you expect to need. None are needed. The rebuttals are operational discipline you can absorb up front rather than learning by re-implementing.

**You might reach for: edge mementos as a first-class type.**

The substrate is a DAG of cached implications, so the natural artifact is "an edge memento" with structure `(p-CID, q-CID, witness-CID, signer, signature)`. v1.5.0 doesn't have that type.

It doesn't need to. A `ContractDecl` with `pre = p`, `post = q`, with the witness carried in `EvidenceCertificate.proofData`, is byte-equivalent to an edge memento. Content-addressing means equivalent structure hashes to the same CID regardless of what the type is named. ContractDecl-overloading works.

Whether ContractDecl-overloading remains the right collapse at scale, or whether elevation to a first-class `EdgeMemento` type is cleaner, is a question we genuinely don't know yet. The call-edges-as-structured-artifact infrastructure only landed in #348. There is no operational history. Elevating before evidence is over-engineering. Decided by data, year out.

**You might reach for: foundation boundary-provenance vocabulary in the protocol.**

Predicates like `untrusted(x)`, `network_input(x)`, `attacker_controlled(x)`, `freed(p)`, `lock_held(L)` need to be normative for cross-kit propagation to work. Surely the protocol must standardize them?

It doesn't. Catalogs version independently of the protocol. The foundation-baseline catalog (per language) is where standardized predicates land, signed by the canonical authority for each language. The protocol carries arbitrary atomic formulas; the catalog standardizes the vocabulary. Catalog grow is not protocol grow.

**You might reach for: per-language drop-shape mementos.**

For the dropper to write missing code, it needs to know `requires sanitized` translates to `@SafeSQL` in Java, `#[verifier::requires]` in Rust, `assert` in Python. Where does that mapping live? In the protocol?

In the kit. Each kit's dropper is per-kit code with hardcoded knowledge of the host language's idioms for the foundation-catalog predicates it cares about. This scales fine for the launch corpus (a few dozen common predicates). At larger scale, drop-shape mementos may be worth standardizing. Empirical question, decided after operating.

**You might reach for: allocation/read site addressing in the IR.**

The completeness lemma cites `Allocations × Reads`. The IR doesn't have explicit `Allocation` or `Read` types. Surely the IR needs to grow?

It doesn't. The IR's `Lambda` binds (allocation), `Let` binds (allocation), `Var` references (read). The SSA structure is implicit in the binding nesting. Lifters walk the AST and emit the right structure. The Cartesian product is computed kit-side from the existing IR primitives. The IR is at the right granularity.

### The empirical milestone

The right gate for "v1.5.0 IS the substrate" is not a v1.6 bump or a re-read of this paper. It is the conformance epic, tracked at issue #277. Twelve kits producing byte-identical mementos against the same fixtures. The remaining kits are the pressure path; when conformance closes, the substrate-as-substrate claim is empirically grounded by demonstration, not by argument.

This is the milestone that retires the "do we need v1.6" question. Until conformance closes, anyone can construct a hypothetical for which v1.5.0 might be insufficient. After conformance closes, the hypotheticals are decidable empirically against the byte-equivalence record. The discipline is to ship v1.5.0, ship this paper as discipline-on-the-substrate, accumulate operational data, and propose v1.6 only if and when empirical operation surfaces a genuine gap.

## §12: The diplomatic substrate framing, extended

After Reputation argued that Sugar is the diplomatic substrate between every truth-claim about software ever made. This paper extends the framing.

Verification today is not just fragmented across tools. It is fragmented across disciplines that do not speak to each other. Static analyzers cannot consume SMT solver outputs. SMT solvers cannot consume proof assistant certificates. Proof assistants cannot consume type-system constraints from arbitrary languages. Type systems cannot consume runtime invariants. Runtime invariants cannot compose into supply-chain claims. Each discipline has its own model of what is true, and the models do not federate.

Content-addressable signed edges in a thin Heyting category are the lingua franca that lets them federate. Static analyzers become edge producers (here is what I checked, signed). SMT solvers become edge producers (here is the implication, with witness, signed). Proof assistants become edge producers (here is a proof of a hard claim, signed). Type checkers become edge producers (here are the structural constraints I emitted, signed). Hand-derived chains become edge producers when nothing else fits. The substrate is the cache they all write to and read from.

The protocol is not a verifier. It is the diplomatic substrate that lets every verification approach finally compose.

This is why "bug classes vanish structurally" is not aspirational rhetoric. It is the operational consequence of the substrate plus the structural-elimination theorem of §5. Each leaf-discharge bug class becomes one proof-obligation pattern that, once the substrate has cached enough witnesses, is dischargeable everywhere it appears. Every existing static analyzer's findings can be recast as edges. Every existing SMT lemma can be recast as an edge. Every existing proof assistant's certificate can be recast as an edge. The substrate accepts them all and composes them.

What humans contribute, in the long run, is novel witness shapes for genuinely new patterns. Most everyday verification work becomes lookup. The genuinely creative work, designing new safety primitives, becomes the human's. The mechanical work disappears into the substrate.

### The same conversation, from two sides

This paper and *The Vertical Stack and the Road to Standardization* (paper 04) are the same conversation seen from two sides. Paper 04 maps what falls out in regulated industries when this paper's discipline is properly applied: DO-178C, Common Criteria EAL5+, ISO 26262, FDA SaMD, FedRAMP, IEC 62304, NIST SSDF, SLSA, the EU Cyber Resilience Act. Each of those frameworks asks for guarantees that prior verification approaches cannot underwrite at scale, because their results are ephemeral and unsigned. The substrate's federated, content-addressed, monotonically-accumulating proof DAG is what underwriters can underwrite.

Paper 04 is the destination. Paper 07 is the path. Without leaf-discharge structural elimination, regulated industries cannot underwrite the substrate's claims. With it, the substrate enters the procurement, certification, and audit pipelines that paper 04 maps. Every chunk of standardization horizon paper 04 catalogs becomes operationally tractable once the discipline of this paper is in place.

The two papers compose: 04 is what discipline-properly-applied gets you in the regulated world; 07 is the discipline that earns those outcomes. Read together, they describe a single arc from "verification today is fragmented and ephemeral" through "the substrate replaces fragmentation with federated proof" to "regulated industries can finally underwrite software the way they underwrite physical engineering."

## §13: What this paper is NOT

- It is not a roadmap for fully automated bug-class elimination across every language at once. The substrate ships incrementally; coverage grows as lifters and droppers extend their reach.
- It is not a sales pitch. The substrate is the substrate; whether anyone adopts it is a separate question.
- It is not formally airtight. Each of the §3 categorical claims could be sharpened with reference to the type theory's exact definition; each of the §5 proof obligations could be discharged with reference to specific protocol artifacts; the universal quantification claim could be type-theoretically precise. The argument is sustained, not airtight.
- It is not exhaustive on consequences. The eBPF verifier collapse, the kABI breakage detection, the AI-as-contract-implementation-generator collapse, the CVE remediation pipeline, each could be its own paper.

It is an argument that bug-class elimination becomes a theorem rather than a heuristic once the substrate is in place, and that the theorem's proof is constructive and short. The mechanical consequences fall out. The civilizational scale is a property of the substrate, not a marketing claim about it.

## §14: Acknowledgments

Patrick Cousot and Radhia Cousot framed abstract interpretation in 1977 with the foresight that data-flow analysis is mathematically a Galois connection between concrete and abstract domains. Their framework is the load-bearing mathematics of this paper. The substrate this paper describes is, in the most precise sense, their framework lifted to a content-addressed federated substrate. Fifty years on, their work is still doing the work.

Edsger Dijkstra published `wp` in 1975, on the path that started with Hoare's 1969 axiomatic semantics. The propagation algorithm at the heart of the DAG is theirs. The substrate makes it persistent and federated; the algorithm itself is unchanged from 1975.

Sparse and Smatch maintainers have demonstrated for two decades that abstract interpretation runs on real kernels at production scale. Their work is the existence proof that the substrate's propagation primitive is operationally tractable.

The cypherpunks-mailing-list lineage is, again, the formative context. The 1995 dedup-via-hashing insight is the architectural cut that makes content-addressing thinkable. The 1998 Digital Confetti insight on incentive-aligned distribution is the architectural cut that makes federation thinkable. eDonkey, ShareReactor, BitTorrent, Bitcoin, Git, IPFS: each is an operational ancestor that taught the field that distributed content-addressed substrates work at planet scale. The substrate this paper describes is the verification-substrate member of that family.

The Apache JCS team page lists the architect of this protocol as a member during the iFilm era. The lineage is verifiable.

The structural-elimination theorem was articulated in conversation with Claude Opus 4.7 (1M context) on 2026-05-04. The proof is constructive; the framing emerged in dialogue; the responsibility for the substrate's actual operation rests with the maintainers.

## §15: Citation

> Savo, T. (2026). *After Verification: Bug Classes as Missing Edges in the Federated Proof Substrate*. Sugar Papers, vol. 7. Content-addressed at: blake3-512:&lt;CID at publication&gt;. Available at https://github.com/TSavo/provekit/blob/main/docs/papers/07-after-verification-bug-classes-as-missing-edges.md.

---

*Last edit: 2026-05-04. Previous papers: [01](01-whitepaper.md), [02](02-bluepaper.md), [03](03-substrate-not-blockchain.md), [04](04-vertical-stack-and-standardization.md), [05](05-witness-pluralism-and-jurisdiction-neutral-transport.md), [06](06-after-reputation-software-as-federated-truth-claims.md).*
