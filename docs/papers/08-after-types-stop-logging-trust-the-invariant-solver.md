# After Types: How I Learned to Stop Logging and Trust the Invariant Solver

> **Status.** Sustained argument. Engages counterarguments. Written to be cite-able.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [03 Substrate, not Blockchain](03-substrate-not-blockchain.md), [04 Vertical Stack and Standardization](04-vertical-stack-and-standardization.md), [05 Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md), [07 After Verification](07-after-verification-bug-classes-as-missing-edges.md).
>
> **Premise the earlier papers established.** A protocol for content-addressable, cryptographically-signed, byte-deterministic claims about software behavior, federated across signers, composable end-to-end, jurisdiction-neutral, and machine-checkable. *After Reputation* argued that the substrate replaces reputation as the load-bearing trust mechanism. *After Verification* showed that the same substrate, with droppers closing the loop, makes leaf-discharge bug classes structurally impossible.
>
> **What this paper argues.** That two correctness primitives engineers have leaned on for fifty years, types and runtime logs, lose their load-bearing role once the substrate ships. Both survive but as different things: types as editorial scaffolding, logs as observability. Neither remains the wall against bugs. The wall is somewhere else now.

## §0: The claim

For fifty years the dominant correctness paradigm has been `compile-time-shape-check + runtime-print-and-pray`. Types categorize the shape of values. Logs narrate what those values became. Both are workarounds for an invariant system that did not exist.

Types categorize shape because shape was the most a compiler could check at compile time without a content-addressed federated proof cache to lean on. Logs narrate runtime events because narration was the most you could do after prevention failed. The substrate from paper 07 verifies invariants directly: arrival-by-arrival, content-addressed, federated, byte-deterministic. Once invariant proof is a primitive, types collapse to editorial scaffolding and logs collapse to observability. Neither remains load-bearing for correctness.

This is the substrate confession the title points at. Engineers have always known the paradigm was a workaround. The reason logs exist is that types could not carry the invariant. The reason types collapse to "an int? a string? a struct?" is that anything richer required a proof system the language could not afford to ship. The substrate is the proof system the language could not afford. Now it is content-addressed and federated, so every language gets it for free.

What follows is what changes once that lands.

## §1: Types today

A type system in a mainstream production language (C, Java, Go, Python, Kotlin, TypeScript, Rust, Swift) categorizes the shape of values. It catches a narrow band of errors: passing the wrong number of arguments, dereferencing through the wrong constructor, indexing a list with a non-integer. Rust extends shape with ownership and borrow checking. Kotlin and Swift add nullability. TypeScript adds gradual structural typing. Each extension catches one class of bugs at compile time that earlier languages caught at runtime, or not at all.

What mainstream types do not carry, and never have:

- **State-dependent contracts.** "This list is sorted." "This lock is held." "This file handle is open." A type signature `List<int>` says nothing about ordering. `Mutex<T>` says nothing about whether the lock is currently held by this thread. The shape category is too coarse.

- **Value-relational invariants.** "The `total` field equals the sum of `items[i].price`." "The `size` field equals `len(buffer)`." "The `cursor` is in `[0, len)`." These are properties between fields, expressible at the type level only with dependent types.

- **Control-flow-dependent facts.** "On the `Ok` branch, `result.value` is not null." "After this guard, the cast is safe." Most languages handle these with refinement at the AST level (TypeScript's narrowing, Kotlin's smart casts) but the refinement does not survive function boundaries.

- **Provenance and trust labels.** "This string came from `sanitize_for_sql`." "This number is bounded by `validate_range`." Tainting and untainting are first-class invariant transitions; types do not carry them in any mainstream system.

- **Cross-call temporal contracts.** "`acquire` was called before `release`." "`open` was called before `read`." Linear types and session types address this in research languages; mainstream types do not.

- **Numerical invariants.** "x is positive." "x is in [0, 100]." Refinement types capture these. Mainstream types do not.

The few systems that *do* express invariants in types (Coq, Agda, Lean, F*, Idris, Liquid Haskell, Dafny) are proof systems wearing a type-system hat. Their ergonomics are bad in production not because the underlying idea is wrong but because each one is doing substrate-level work without a substrate's federated cache. Every Coq proof discharges a fresh obligation. Every F* refinement is verified anew. The proof artifacts are project-local, unsigned, unshared. The same lemma is rediscovered in every project, every CI run, every developer's machine.

The mainstream type system shipped because shape categorization is what you could afford to verify on every build. Anything richer required an invariant system. The invariant system did not exist.

## §2: Logs today

A production codebase in 2026 is instrumented top to bottom: structured logging, distributed traces, error monitoring, metrics streams, performance counters, audit trails. The combined cost of log ingest, storage, indexing, and search across the industry is north of $20B per year on observability vendors alone, with internal infrastructure on top of that.

The honest justification for that spend is captured in one sentence: "Well, at least we can see what went wrong."

That sentence is a confession. It admits that prevention failed somewhere upstream. The compiler did not catch the bug. The static analyzer did not catch it. The test suite did not catch it. The fuzzer did not hit the path. The reviewer did not see it. The deployment pipeline did not block it. Now production has the bug, and the only remaining recourse is to read about it after the fact in the logs.

This is not a value judgment about logging. Logs are useful and will remain useful for many things. The point is structural: logs exist as a load-bearing correctness tool because no upstream tool was strong enough to prevent the bug. If prevention had been strong enough, the log line that says "uncaught NullPointerException at OrderProcessor.java:283" would not need to exist, because the path from `nullable input` to `dereference` would not have compiled.

Three observations follow:

**The log industry is structured around bug categories prevention failed at.** The largest categories of production observability cost are exception monitoring (bugs prevention missed), performance regression (resource invariants prevention did not encode), and security incident telemetry (sanitization invariants prevention did not enforce). Each is a category where compile-time-shape-check could not carry the invariant.

**Distributed tracing is whole-program forensics.** A trace tells you that request *r* visited services *s1, s2, s3* and that *s2* spent 4.7 seconds in a downstream call. The trace is necessary because no static analysis sees across service boundaries. The substrate sees across service boundaries: an edge `p → q` is the same edge whether `p` and `q` are in the same process or in different ones. The traces persist because the prevention does not.

**Structured logging is structured forensic narration.** Engineers add `log.info("user.id=%s, action=%s, balance=%d", id, action, bal)` not because they want to read it later (most log lines are read by no one) but because *if* something goes wrong they want the variables that were in scope. This is debugging-by-anticipation. The substrate's per-arrival WP says exactly what was in scope, byte-deterministically, so a debugging session that today requires log replay can in principle become a substrate query.

Logs are useful. Logs are the safety net for everything prevention misses. The size of the log industry is the size of what prevention has missed.

## §3: The shift

The substrate proves invariants directly. Each invariant is a content-addressed predicate. Each one-step implication is a content-addressed signed edge. Verifying that a program is correct in the leaf-discharge frame is graph reachability from boundary axioms to sink preconditions, each link carried by a cached edge whose CID is endpoint-determined.

This is the same shape as `After Verification`. The shift here is what falls out for the two correctness primitives engineers have built around for fifty years.

**Types stop being load-bearing for correctness.** They survive as editorial scaffolding (autocomplete, refactoring affordances, signature legibility, IDE feedback). The substrate carries what types tried and failed to carry: state-dependent contracts, value relationships, provenance labels, cross-call temporal facts, numerical bounds. Mainstream types remain useful at the editing surface; they are no longer the wall against bugs.

**Logs stop being load-bearing for correctness.** They survive as observability (latency, throughput, business analytics, audit trail, operations dashboard, incident postmortem narration). The substrate prevents the outages logs were going to narrate. Logs remain useful at the observability surface; they are no longer the safety net for missed bugs.

The paradigm shifts from `compile-time-shape-check + runtime-print-and-pray` to `compile-time-substrate-proof + runtime-observability`. The compile-time check carries the invariant, not the shape. The runtime narration is for performance and ops, not for catching what prevention missed.

This is the substrate's promise turned inward at the developer's daily workflow. Paper 07 said the substrate retires verification fragmentation. This paper says the substrate retires the *crutches* engineers built when verification fragmentation was the only option.

## §4: What types still do

Demoting types from "load-bearing for correctness" to "editorial scaffolding" is not a deletion. Types still earn their keep, just at a different layer.

**IDE feedback.** Autocomplete needs shape. Hover-tooltip needs signature. Go-to-definition needs the type to know where the definition lives. None of these require the type to carry an invariant; they require the type to categorize.

**Refactoring affordance.** Renaming a function across a codebase is type-safe because the type checker knows where the function is called from. Substrate edges do not replace this. The two coexist: types tell you where to apply the rename, the substrate tells you whether the renamed program still proves its invariants.

**Expression discipline.** A statement like `int x = "hello"` is a syntax-level mistake. The type system catches it instantly, with an error message a developer reads and fixes in two seconds. The substrate would also catch it, eventually, as a missing edge. The type system catches it faster, at the keystroke. Speed of feedback at the editing surface is the type system's permanent value-add.

**Documentation.** A function signature `fn parse(input: &str) -> Result<Json, ParseError>` tells a reader more than the function name does. The substrate's invariants are richer but harder to read at a glance. Types remain the human-legible signature.

**Public API surface.** A library exposes types to its callers because callers need shape to call. The substrate's invariants travel along edges that already have shape; the shape is the editorial face of the edge.

What changes is the *contract*. A type signature is no longer a sufficient correctness contract. It is a sufficient *editing* contract. The correctness contract is the bundle of edges from the function's accumulated WP at entry to its WP at exit, content-addressed and signed. Types document the editing surface; the substrate documents the correctness surface.

This is not new. Documentation and tests already separate "what the function looks like" from "what the function actually does." The substrate adds a third layer that proves the second.

## §5: What logs still do

The same demotion applies to logs. They lose their load-bearing role for correctness; they keep their value for everything correctness was never the right framing for.

**Latency and throughput.** "How long did this request take?" "How many requests per second?" These are performance questions, not correctness questions. The substrate does not answer them. Logs and metrics do.

**Business analytics.** "How many users completed the signup flow this week?" "What is the conversion rate from cart to checkout?" These are not invariants the program is supposed to enforce; they are observed quantities the business wants to track.

**Audit trail.** "Who accessed this record?" "When was this transaction approved?" Compliance regimes (HIPAA, PCI-DSS, SOC2, GDPR Article 30) require an immutable record of *who did what when*. The substrate proves the program is correct; it does not record the runtime stream of authorized operations. The audit log does that.

**Operations dashboard.** "Is the database CPU pegged?" "Has any service restart-looped?" Infrastructure observability is a different problem from correctness. SREs run on metrics and traces. Substrate proofs do not replace the dashboard.

**Incident postmortem.** When something *does* go wrong (a 3rd-party service flapped, a bug class the substrate does not yet cover slipped through, a deploy did something unexpected), the team needs forensics. Logs and traces remain the forensics trail.

What changes is the *expectation*. The expectation that "the log will catch what the type system missed" is gone, because the type system was not the wall against bugs in the first place. The substrate is. What the substrate cannot catch (logic errors, performance bugs, specification bugs, see paper 07 §5) is precisely what *no* prevention tool can catch and what observability remains useful for. Logs are sized to forensics, not to redundancy with a prevention tool that did not exist before.

The economic implication is that the observability bill goes down for the categories the substrate covers. Exception monitoring shrinks because exceptions in leaf-discharge classes shrink. Security incident telemetry shrinks because injection / use-after-free / null-deref incidents shrink. Performance and business observability are unaffected because the substrate does not cover them.

## §6: The substrate as invariant carrier

The substrate from paper 07 is, viewed through this paper's lens, a content-addressed invariant carrier with three properties no prior invariant system has had:

**Federation.** Every invariant is content-addressed. Every implication edge is content-addressed and signed. Two parties independently verifying the same fact mint byte-identical edges; the substrate deduplicates automatically. The invariant cache is not project-local. It is ecosystem-wide.

**Composition.** An edge `p → q` and an edge `q → r` compose to an edge `p → r` whose CID is `hash(p-CID, r-CID)`. Composition is O(1). The invariant a program needs is built up by composing cached edges, not by re-deriving from scratch. This is what no Coq, Agda, F*, Liquid Haskell ever offered: federated composition.

**Provenance.** Every edge carries the witness chain back to its boundary axioms. An invariant `safe_for_sql(query)` resolves not to a static label but to a path through the DAG: `untrusted_input → sanitized_for_sql → safe_for_sql`, with each step content-addressed and signed. Provenance is the invariant.

These three properties are why the substrate replaces both types and logs as the correctness wall. Types could not federate (every project has its own type-checked codebase, no sharing of proof obligations). Logs could not compose (a log line is a fact about one execution, not a proposition about all executions). The substrate has both.

A predicate is a node. A one-step implication is an edge. A program's correctness is a set of paths. A bug is a missing edge. Verification of a new program is graph reachability over the cached edges. New verification mints new edges. The cache amortizes across the entire ecosystem.

This is the architectural shape. The reason it lands now and not earlier is that content-addressing, federated cryptographic signatures, and byte-deterministic canonicalization all had to ship together. Any one of them missing collapses the architecture: without content-addressing the edges do not deduplicate, without signatures they do not federate, without byte-determinism the CIDs do not agree across implementations. ProvekIt v1.5.0 is the first system to ship all three.

## §7: A worked example

Consider a Java/Kotlin web service that takes user input and constructs a SQL query. The code is fully instrumented: structured logging, distributed tracing, exception monitoring with stack traces and breadcrumbs, audit logging for compliance.

A developer writes:

```java
String query = "SELECT * FROM orders WHERE user_id = " + userInput;
ResultSet rs = stmt.executeQuery(query);
```

The Java type checker accepts this. `String` is a `String`. `executeQuery` takes a `String`. The shapes line up. The type system has done its job.

The static analyzer flags this in some shops, misses it in others, depending on configuration and the level of inter-procedural reach the analyzer is running with.

The fuzzer does not find it because the fuzzer does not have a corpus of SQL injection inputs aimed at this endpoint, or it runs only on integration tests where this code path is not exercised, or the code path is gated by an authentication check the fuzzer cannot pass.

The code reviewer reads the line. Maybe they catch it. Maybe they have a deadline.

The code ships. A user (or an attacker scanning for low-effort wins) sends `userInput = "1 OR 1=1"`. The query becomes `SELECT * FROM orders WHERE user_id = 1 OR 1=1`. The database returns every order in the table. The attacker exfiltrates the data.

Now the observability stack does its job. The error monitor flags the unusually-large response. The audit log records the queries. The distributed trace shows the request path. The SRE pages the security team. The security team reads the logs. The team reconstructs the incident. The team writes a postmortem. The team patches the line:

```java
PreparedStatement ps = conn.prepareStatement("SELECT * FROM orders WHERE user_id = ?");
ps.setLong(1, Long.parseLong(userInput));
ResultSet rs = ps.executeQuery();
```

The patch ships. The team commits to running a static-analysis tool with stricter rules. The team commits to a fuzzer corpus that includes injection payloads. The team commits to a security review checklist. Each commitment is process work that adds friction to future development. The bug-class root cause (the type system did not carry the invariant `safe_for_sql`, and the log was the only safety net) is not addressed.

The substrate's path:

The lifter reads the original `query = "..." + userInput; executeQuery(query)` line. It produces an arrival at `executeQuery(query)` whose precondition is `safe_for_sql(query)` (because `executeQuery` is in the foundation catalog with this leaf precondition signed by a curator). It walks the DAG backward from that arrival. The accumulated WP at the arrival is `untrusted(userInput) ∧ identity(query, "..." + userInput)`, which simplifies to `untrusted(query)`. The substrate looks up the edge `untrusted → safe_for_sql`. The foundation catalog refuses to sign that edge directly (paper 07 §5 base case). The compiler reports a missing edge and refuses the build.

The fix surfaces in the editor before the line ever leaves the developer's machine:

```
error[E0xxx]: missing edge `untrusted → safe_for_sql` at OrderQuery.java:42
   |
42 |     ResultSet rs = stmt.executeQuery(query);
   |                                      ^^^^^ requires `safe_for_sql`
   = note: predecessor `query` carries `untrusted` from `userInput` at line 41
   = help: insert a sanitization edge: use `PreparedStatement` (mints `safe_for_sql`)
   = help: or assert `validated_against_schema` if upstream guarantees it
```

The developer fixes the line. The fix mints the missing edge: `untrusted_input → parameterized_query → safe_for_sql`. The edge is content-addressed; everyone else who hits the same pattern in the same codebase or a different codebase reuses it. The bug never reached production. The log line documenting the incident never got written. The postmortem never happened. The process commitments to stricter analysis and fuzzer corpora never got made.

The economic point: the *cost* of catching this at edit-time is one error message and one editor fix. The cost of catching it at production-time is a security incident, a customer-data exposure disclosure, regulatory cost, postmortem time, process friction, and recurring observability spend on the categories the substrate now covers.

The structural point: the type system saw `String → String`. The log saw a large response. The substrate saw a missing edge. Only one of those three is a correctness check.

## §8: Why now

Three things that did not coincide before do now.

**The substrate v1.5.0 ships with twelve language kits closing C1 through C8 conformance.** The same canonical IR underlies Rust, Java, Python, Go, TypeScript, C#, C, C++, Ruby, Swift, PHP, Zig. An invariant minted in one kit is reusable from every other. This is the federation property: not a Java-only proof system, not a Rust-only proof system, but a content-addressed substrate the languages share.

**Lifter and dropper close the WP loop.** The lifter consumes program code and produces canonical IR with arrivals. The dropper consumes canonical IR and produces back the program-language artifact. The pair is bijective enough that round-trip equality is checkable and the substrate's invariants are anchored to the source the developer reads. Paper 07 articulated this; v1.5.0 ships it.

**Foundation catalog enforces curatorial discipline.** The base layer of signed predicates and edges is curator-controlled. The catalog forbids semantic collapses (`untrusted → safe`, `freed → alive`, `unlocked → lock_held`). This is what makes the §5 base case in paper 07 actually work. It is also what makes types and logs deferrable, because the substrate now has an enforced soundness baseline.

The combination is what makes the paradigm shift from `shape-check + log-narrate` to `substrate-prove + observe` cross the threshold from research idea to production-defensible architecture. Each piece existed in research before; none cohered into a shippable substrate until v1.5.0.

## §9: Counterarguments

**"Types are still useful for tooling."** Yes. This paper says so explicitly in §4. Types remain useful as editorial scaffolding. The claim is not that types disappear; the claim is that types stop being the correctness wall. The two claims are independent.

**"Logs are still useful for observability."** Yes. This paper says so explicitly in §5. Logs remain useful for performance, business analytics, audit, operations, postmortem. The claim is not that logs disappear; the claim is that logs stop being the safety net for missed bugs. The two claims are independent.

**"My framework requires types for serialization (Jackson, serde, Pydantic)."** Tooling concerns, fully compatible. Types carry the serialization shape. The substrate carries the invariant. A field can be `String`-typed for Jackson AND `safe_for_sql`-tagged in the substrate; the two annotations live at different layers and do not conflict.

**"Refinement type systems (Liquid Haskell, F*, Dafny) already do invariants in types."** Correct, and §1 acknowledges them. The point is that they do invariants without federation: every project re-derives the same lemmas, every CI run re-runs the same proofs, the cache is project-local. The substrate adds federation. Liquid Haskell with substrate-backed lemma reuse would be a strictly better Liquid Haskell.

**"Logs catch logic errors and specification errors that the substrate cannot catch."** Correct. Paper 07 §5 explicitly scopes the substrate to leaf-discharge bug classes. Logic errors, performance bugs, specification bugs, and concurrency bugs not reducible to lock-holding are not in scope for substrate elimination. They are not in scope for type systems either. They remain in scope for logging, testing, code review, and other tools that operate above the leaf-discharge layer. This paper's claim is about the leaf-discharge corner specifically. That corner is most of the OWASP Top 10 and most of the kernel CVE corpus, but it is not all bugs.

**"What about runtime-only invariants the compiler cannot see?"** Boundary axioms. Anything the compiler cannot see is signed at the boundary by whoever can see it (the protocol stack, the OS, the kernel, the hardware, the curator). The substrate's edges flow from boundary axioms inward. Runtime-only invariants are first-class boundary nodes; the substrate does not erase them, it makes them auditable as signed entry points.

**"You're asking developers to delete their type system / kill their log stack."** No. The diplomatic substrate framing in §10 says the opposite. Both stay. They get demoted from load-bearing-for-correctness to load-bearing-for-the-other-things-they-are-load-bearing-for. The change is in *expectation*, not in deletion.

**"This is just shifting where the proof obligation lives."** Yes, and that is the architectural point. The proof obligation today lives in three uncoordinated places: the type checker (catches shape), the test suite (catches sampled inputs), the log stack (catches what slipped through). Shifting the proof obligation to a content-addressed federated substrate means each obligation is discharged once, ecosystem-wide, and the discharge is reusable across projects, languages, and time. That is the leverage no prior arrangement had.

**"Mainstream developers will not adopt this."** Adoption is paper 04's job, not this paper's. This paper's claim is structural: *if* the substrate ships and *if* it covers leaf-discharge bug classes, *then* types and logs lose their load-bearing role for correctness and survive in their other roles. The conditional holds independent of adoption rate. Adoption affects when the world reaches the after-types regime; structure determines what the regime looks like.

## §10: The diplomatic substrate framing

Nobody is asking you to delete your type system. Nobody is asking you to turn off your logs. The substrate is additive. It carries invariants the type system never carried. It prevents bugs the log was going to narrate. Your type system stays. Your log stack stays. Both are useful, both have value, both keep doing what they were doing for the things they are good at.

What changes is the *expectation*. The expectation that "if the type checker passes, the program is shape-correct" stays true (because it always was). The expectation that "if the type checker passes, the program is *correct*" was always wishful, and the wishful part is what the substrate replaces. The expectation that "if the log catches an error, we can debug it" stays true. The expectation that "if the log catches an error, that is the safety net we relied on" was always weak (because by the time the log catches the error, production has the bug), and the weak part is what the substrate replaces.

The paradigm before: types catch shape, tests catch sampled inputs, logs catch what slipped past, postmortems catch what the team missed. Each layer is a workaround for the previous one's gaps.

The paradigm after: the substrate catches leaf-discharge invariant violations at edit time. Types carry shape for tooling. Tests cover what tests cover (logic, integration, performance). Logs cover observability and forensics. Each layer does what it is actually good at, and the leaf-discharge gap is filled by the substrate rather than by hopeful redundancy across the other three.

This is friendlier to existing engineering organizations than the framing might suggest. No tooling gets ripped out. The substrate slots in alongside what is already there. The team's debugging muscle memory keeps working; it just gets used less often, on smaller problems, in narrower categories.

## §11: What this paper is NOT

It is not a claim that all bugs disappear. The substrate covers leaf-discharge bug classes, scoped explicitly in paper 07 §5. Logic errors, specification bugs, performance bugs, and concurrency bugs not reducible to lock-holding are out of scope for the substrate and remain in scope for tests, reviews, and observability.

It is not a claim that you should delete your type system today. Editorial scaffolding is still useful; the substrate runs alongside, not in replacement.

It is not a claim that you should kill your log stack today. Observability is still useful; the substrate does not narrate runtime, only proves invariants at compile time.

It is not "types are bad." Types are useful at the editing surface. The claim is narrow: types are the wrong primitive for invariant proof. The right primitive is a content-addressed federated edge.

It is not "logs are bad." Logs are useful for performance, business analytics, audit, operations, postmortem. The claim is narrow: logs are the wrong place to put the correctness wall. The right place is the substrate.

It is not a claim that the substrate is universal across all programming languages on day one. v1.5.0 ships twelve kits; that is the empirical baseline. The argument generalizes because the IR is language-agnostic and the substrate is content-addressed, but kits are still a per-language engineering effort.

It is not Strangelove the movie. The title is a genre tag, not a thesis about catastrophe. The actual thesis is constructive: stop relying on logs as the safety net, trust the invariant solver because the invariant solver now exists. The "stop logging" half is editorial advice; the "trust the invariant solver" half is the load-bearing claim.

## §12: Acknowledgments

This paper continues the After-X arc with paper 06 (*After Reputation*) and paper 07 (*After Verification*). The structural-elimination theorem and the algebraic frame come from paper 07. The federated trust model comes from paper 06. The vertical stack and standardization argument comes from paper 04. The witness-pluralism transport from paper 05. The substrate primitives from papers 01 and 02. This paper assembles those into a paradigm shift for what types and logs are *for*.

Edsger Dijkstra's *A Discipline of Programming* (1976) is the source of the WP-propagation framing the substrate operationalizes. Dijkstra argued that program correctness is a calculation; the substrate makes the calculation content-addressable.

Tony Hoare's *An Axiomatic Basis for Computer Programming* (1969) is the source of the precondition / postcondition contract framing the substrate inherits.

C. B. Jones, *Tentative Steps Toward a Development Method for Interfering Programs* (1983) is the source of the rely-guarantee framing the substrate's edges generalize.

The Cousot-Cousot abstract interpretation framework (1977) is what the substrate's lift-and-walk pipeline materializes at federated cache scale. Paper 07's §3 articulated this; this paper inherits it.

## §13: Citation

Cite as:

> ProvekIt Substrate Working Notes (2026). *After Types: How I Learned to Stop Logging and Trust the Invariant Solver*. Paper 08 of the After-X arc.
