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

A note on claim strength. Paper 07 was scoped: leaf-discharge bug classes (SQL injection, XSS, use-after-free, null-deref, lock-held data races, OWASP Top 10, most kernel CVEs) are structurally eliminated under a sound substrate. This paper generalizes from that scoped result to a paradigm shift in how engineers think about types and logs. The generalization rests on leaf-discharge work being most of the daily-engineering correctness surface that types and logs are deployed to defend. Where logic errors, specification bugs, performance regressions, or concurrency bugs not reducible to lock-holding dominate (kernel scheduler design, distributed-systems consensus, high-frequency trading), types and logs retain a load-bearing role in those corners; what changes is the layer below them, where leaf-discharge dominates. The paper's strong reading ("types and logs lose their load-bearing role") and weak reading ("in the leaf-discharge corner, types and logs lose their load-bearing role") are both defensible. The strong reading rides on an empirical claim about bug-distribution shape; the weak reading is a direct corollary of paper 07. Read whichever the evidence in your domain supports.

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

A production codebase in 2026 is instrumented top to bottom: structured logging, distributed traces, error monitoring, metrics streams, performance counters, audit trails. The combined annual spend on observability vendors is in the tens of billions, with internal infrastructure on top of that.

Logs as a category are not monolithic. Two kinds live in the same codebase under the same `log.info(...)` calls, and they have very different relationships to prevention.

**Forensic logs.** Exception monitoring, error tracking, security telemetry, post-incident breadcrumbs, "the variables that were in scope when this NullPointerException fired." These exist because prevention failed somewhere upstream and the only remaining recourse was to read about it after the fact. The honest justification for this category is one sentence: "Well, at least we can see what went wrong." That sentence is a confession. It admits that the compiler did not catch the bug, the static analyzer did not catch it, the test suite did not catch it, the fuzzer did not hit the path, the reviewer did not see it, the deployment pipeline did not block it. The path from `nullable input` to `dereference` compiled. Production has the bug. The log line is the safety net.

**Operational logs.** Retry-loop telemetry under network partition, clock-skew warnings, partial-failure narration, hardware-fault diagnostics, third-party API timeout patterns, eventual-consistency drift, leader-election flap counts, garbage-collection pause traces. These exist because the program is operating in an environment that is irreducibly non-deterministic. No prevention tool, substrate or otherwise, eliminates network partitions or clock skew. Operational logs are how a running system narrates conditions the program *correctly* responds to but cannot prevent.

The bifurcation matters because the two categories have opposite trajectories under the substrate. Forensic logs shrink. The line `OrderProcessor.java:283 NullPointerException` does not need to exist if `nullable → dereference` cannot compile. The line `SqlInjectionAttempt detected on /api/orders` does not need to exist if `untrusted → execute_query` cannot compile. The forensic stream of "the program did something it should not have" thins to the categories the substrate does not yet cover (logic, spec, performance, concurrency-not-reducible-to-lock-holding; see paper 07 §5). Operational logs, by contrast, do not thin at all. The substrate cannot prove that the database will not flap, that the upstream API will not 503, that the clock will not skew. Operational telemetry is irreducibly necessary.

Three observations follow within the forensic category:

**The forensic side of the log industry is structured around bug categories prevention failed at.** Exception monitoring vendors (Sentry, Rollbar, Bugsnag, Honeybadger), security telemetry vendors (Wiz, Snyk's runtime products, Datadog ASM), and the forensic slices of full-stack observability (Datadog APM error views, NewRelic error inbox, Honeycomb's error queries) are sized to what prevention missed. Each is a category where compile-time-shape-check could not carry the invariant.

**Distributed tracing of forensic events is whole-program forensics.** A trace tells you that request *r* visited services *s1, s2, s3* and that *s2* threw at line 283 with these breadcrumbs. The trace is necessary because no static analysis sees across service boundaries. The substrate sees across service boundaries: an edge `p → q` is the same edge whether `p` and `q` are in the same process or in different ones. (Operational tracing for latency and throughput remains untouched; the substrate does not predict resource contention.)

**Structured forensic logging is structured debugging-by-anticipation.** Engineers add `log.info("user.id=%s, action=%s, balance=%d", id, action, bal)` not because they want to read it later (most forensic log lines are read by no one) but because *if* something goes wrong they want the variables that were in scope. The substrate's per-arrival WP records exactly what was in scope, byte-deterministically, so the forensic debugging session that today requires log replay can in principle become a substrate query.

Forensic logs shrink with the substrate. Operational logs persist because the substrate is not in the business of predicting non-determinism. The size of the *forensic* log industry is the size of what prevention has missed; the size of the *operational* log industry is the size of what prevention can never address. Sections §5 and §10 keep this distinction explicit.

## §3: The shift

The substrate proves invariants directly. Each invariant is a content-addressed predicate. Each one-step implication is a content-addressed signed edge. Verifying that a program is correct in the leaf-discharge frame is graph reachability from boundary axioms to sink preconditions, each link carried by a cached edge whose CID is endpoint-determined.

This is the same shape as `After Verification`. The shift here is what falls out for the two correctness primitives engineers have built around for fifty years.

**Types stop being load-bearing for correctness.** They survive as editorial scaffolding (autocomplete, refactoring affordances, signature legibility, IDE feedback). The substrate carries what types tried and failed to carry: state-dependent contracts, value relationships, provenance labels, cross-call temporal facts, numerical bounds. Mainstream types remain useful at the editing surface; they are no longer the wall against bugs.

**Logs stop being load-bearing for correctness.** They survive as observability (latency, throughput, business analytics, audit trail, operations dashboard, incident postmortem narration). The substrate prevents the outages logs were going to narrate. Logs remain useful at the observability surface; they are no longer the safety net for missed bugs.

The paradigm shifts from `compile-time-shape-check + runtime-print-and-pray` to `compile-time-substrate-proof + runtime-observability`. The compile-time check carries the invariant, not the shape. The runtime narration is for performance and ops, not for catching what prevention missed.

This is the substrate's promise turned inward at the developer's daily workflow. Paper 07 said the substrate retires verification fragmentation. This paper says the substrate retires the *crutches* engineers built when verification fragmentation was the only option.

The Strangelove confession is the actual experience. I logged everything because I did not trust my type system to carry the invariant. I added structured logging on every code path because I knew prevention was leaky and I wanted forensics ready when the leak surfaced. I wrote tests to assert what types could not assert. I added defensive nullability checks at every layer because the type system did not say which layer was responsible. I treated the production log stream as the safety net because nothing upstream was strong enough to be the safety net. The confession is not that any of this was *wrong*. It is that all of it was *secondary*, scaffolding around an absence. The substrate is what trust looks like when it is real. The relief in stopping is the relief of putting down a crutch you did not realize was a crutch.

## §4: What types still do

Demoting types from "load-bearing for correctness" to "editorial scaffolding" is not a deletion. Types still earn their keep, just at a different layer.

**IDE feedback.** Autocomplete needs shape. Hover-tooltip needs signature. Go-to-definition needs the type to know where the definition lives. None of these require the type to carry an invariant; they require the type to categorize.

**Refactoring affordance.** Renaming a function across a codebase is type-safe because the type checker knows where the function is called from. Substrate edges do not replace this. The two coexist: types tell you where to apply the rename, the substrate tells you whether the renamed program still proves its invariants.

**Expression discipline.** A statement like `int x = "hello"` is a syntax-level mistake. The type system catches it instantly, with an error message a developer reads and fixes in two seconds. The substrate would also catch it, eventually, as a missing edge. The type system catches it faster, at the keystroke. Speed of feedback at the editing surface is the type system's permanent value-add.

**Documentation.** A function signature `fn parse(input: &str) -> Result<Json, ParseError>` tells a reader more than the function name does. The substrate's invariants are richer but harder to read at a glance. Types remain the human-legible signature.

**Public API surface.** A library exposes types to its callers because callers need shape to call. The substrate's invariants travel along edges that already have shape; the shape is the editorial face of the edge.

**Cross-language interface contracts during rollout.** v1.5.0 ships twelve kits; languages outside that set still need a way to talk to substrate-covered code. Types are the lingua franca at those boundaries: the C ABI, gRPC stubs, OpenAPI schemas, Protobuf definitions, Avro records. While the substrate's federation is partial, types remain load-bearing at boundaries the substrate has not reached yet. As more kits land, the boundary that needs type-as-correctness-contract recedes, but it does not vanish on day one. This is a transitional load-bearing role for types that paper 04's vertical-stack rollout shape inherits.

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

Paper 07 introduced the substrate as a *proof carrier*: each edge `p → q` is a one-step WP implication, content-addressed and signed, composable into the proof of any reachable `(source, sink)` pair. The new framing this paper adds is to read the same object as an *invariant carrier*. A predicate is an invariant; an edge is the assertion that one invariant implies another; a program's correctness obligation is a set of paths between invariants. The two readings are isomorphic; the difference is which face of the same object is in front.

Why the invariant-carrier reading matters for the types-and-logs argument: an invariant carrier has three properties no prior invariant system has had simultaneously. **Federation** (content-addressing makes invariants ecosystem-wide, not project-local), **composition** (edge-of-edges is O(1) endpoint hashing, not re-derivation), and **provenance** (every invariant resolves to a witness chain back to signed boundary axioms). Types had none of the three. Refinement type systems have composition and partial provenance but no federation. Logs have provenance for a single execution but no composition into propositions and no federation. The substrate has all three; that is structurally why it can carry what types tried to carry and what logs surfaced after-the-fact.

For the algebraic frame backing these properties (thin Heyting category, endpoint-determined morphism CIDs, free composition over hashed endpoint pairs), see paper 07 §3. This paper inherits that frame; the contribution here is recognizing that the same object's invariant-carrier reading is what demotes types and logs.

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

The substrate path is not free. Honestly accounted, here is what it costs:

- **Conceptual onboarding.** The developer needs to know what `safe_for_sql` means, what the foundation catalog is, and that "missing edge" is the substrate's term for "I cannot prove this composes." This is comparable to learning what `borrow checker` means in Rust, what `unsafe` allows in Rust, what `dyn Trait` does, what `Send + Sync` requires. A new vocabulary at the editing surface, with a learning curve in days not weeks.
- **Lifter dependency.** The build environment must include the language kit's lifter for the substrate to see the program at all. v1.5.0 ships twelve kits; for languages outside the twelve, the substrate is not yet available, and the team is back to types and logs as load-bearing. (This is a temporary scoping condition, not a permanent one; paper 04 addresses the rollout shape.)
- **Foundation catalog presence.** The base predicates `untrusted`, `safe_for_sql`, `parameterized_query`, the curator-signed edges between them, and the curator's signing keys must be reachable from the build environment. In practice this means a content-addressed cache backed by federated storage; in the v1.5.0 reference deployment it is one Vault-backed signing trust root and a Paperclip-served CID resolver. Setting it up the first time has the same shape as setting up package-manager mirrors for an air-gapped network.
- **Edit-time error budget.** Substrate refusals replace runtime exceptions, but they do replace them as something the developer reads and reacts to. A team with weak edit-loop discipline will feel the substrate as friction in early days the same way teams with weak test discipline felt CI as friction in 2010. The friction-vs-prevention trade is the right one, but it is a real trade.
- **Foundation catalog evolution.** When a new predicate becomes useful (a novel sanitizer, a new boundary class), someone has to mint it and a curator has to sign the foundation-level edges. The pace of catalog evolution is part of the substrate's running cost. v1.5.0 ships an initial catalog; the catalog grows with use.

The economic point, after honest accounting: the *aggregate* cost of catching leaf-discharge bugs at edit-time across an organization is lower than the cost of catching them at production-time, summed across security incidents, customer-data exposures, regulatory exposure, postmortem time, process friction, and recurring forensic-observability spend. The substrate's per-incident cost is not zero; it is a different shape and a smaller integer.

The structural point, after the SQL-injection example: the type system saw `String → String`. The forensic log narrated a large response *after* the breach. The substrate saw a missing edge *before* the line shipped. Only one of those three is a correctness check; the other two are what teams used because no correctness check was available at that layer.

## §8: Why now

Three things that did not coincide before do now.

**The substrate v1.5.0 ships with twelve language kits closing the C1 through C8 lift-plugin-protocol conformance gate.** (The C1 through C8 gate here is the project's per-kit verifier suite that asserts byte-equivalent canonical IR across implementations; it is not the C1 through C7 organizational-invariance corollary list from paper 05, which is a separate theorem about gauge invariance.) The same canonical IR underlies Rust, Java, Python, Go, TypeScript, C#, C, C++, Ruby, Swift, PHP, Zig. An invariant minted in one kit is reusable from every other. This is the federation property: not a Java-only proof system, not a Rust-only proof system, but a content-addressed substrate the languages share. (v1.5.0 is the version this paper anticipates; the catalog at time of writing tags v1.4.1, with v1.5.0 closing the cascade currently merging.)

**Lifter and dropper close the WP loop.** The lifter consumes program code and produces canonical IR with arrivals. The dropper consumes canonical IR and produces back the program-language artifact. The pair is bijective enough that round-trip equality is checkable and the substrate's invariants are anchored to the source the developer reads. Paper 07 articulated this; v1.5.0 ships it.

**Foundation catalog enforces curatorial discipline.** The base layer of signed predicates and edges is curator-controlled. The catalog forbids semantic collapses (`untrusted → safe`, `freed → alive`, `unlocked → lock_held`). This is what makes the §5 base case in paper 07 actually work. It is also what makes types and logs deferrable, because the substrate now has an enforced soundness baseline.

The combination is what makes the paradigm shift from `shape-check + log-narrate` to `substrate-prove + observe` cross the threshold from research idea to production-defensible architecture. Each piece existed in research before; none cohered into a shippable substrate until v1.5.0.

## §9: Counterarguments

**"Types are still useful for tooling."** Yes. This paper says so explicitly in §4. Types remain useful as editorial scaffolding. The claim is not that types disappear; the claim is that types stop being the correctness wall. The two claims are independent.

**"Logs are still useful for observability."** Yes. This paper says so explicitly in §5. Logs remain useful for performance, business analytics, audit, operations, postmortem. The claim is not that logs disappear; the claim is that logs stop being the safety net for missed bugs. The two claims are independent.

**"My framework requires types for serialization (Jackson, serde, Pydantic)."** Tooling concerns, fully compatible. Types carry the serialization shape. The substrate carries the invariant. A field can be `String`-typed for Jackson AND `safe_for_sql`-tagged in the substrate; the two annotations live at different layers and do not conflict.

**"Refinement type systems (Liquid Haskell, F*, Dafny) already do invariants in types."** Correct, and §1 acknowledges them. The point is that they do invariants without federation: every project re-derives the same lemmas, every CI run re-runs the same proofs, the cache is project-local. The substrate adds federation. Liquid Haskell with substrate-backed lemma reuse would be a strictly better Liquid Haskell.

**"Logs catch logic errors and specification errors that the substrate cannot catch."** Correct. Paper 07 §5 explicitly scopes the substrate to leaf-discharge bug classes. Logic errors, performance bugs, specification bugs, and concurrency bugs not reducible to lock-holding are not in scope for substrate elimination. They are not in scope for type systems either. They remain in scope for logging, testing, code review, and other tools that operate above the leaf-discharge layer. This paper's claim is about the leaf-discharge corner specifically. That corner is most of the OWASP Top 10 and most of the kernel CVE corpus, but it is not all bugs.

**"What about runtime-only invariants the compiler cannot see?"** Boundary axioms, and this is the steelman the paper owes a serious answer. Anything the compiler cannot see is signed at the boundary by whoever can see it (the protocol stack, the OS, the kernel, the hardware, the curator). The substrate's edges flow from boundary axioms inward.

The honest consequence: boundary specification becomes the new central problem. Where bugs live in 2026 modern distributed systems is overwhelmingly at boundaries, wrong assumptions about external service contracts, eventual consistency, clock monotonicity, partial failure semantics, memory pressure, byzantine inputs from upstream services. The substrate proves that *given* the boundary axioms, the program's leaf-discharge invariants compose. It does not prove the boundary axioms are right. A signed boundary axiom that misrepresents the upstream service contract produces a program that proves correct against the wrong axiom. The bug moves from "the program did the wrong thing under correct assumptions" to "the program did the right thing under wrong assumptions." Both shapes hurt; the second is harder to detect because the proof passes.

This is not a regression from the status quo, where there is no formal axiom and engineers reason about boundaries informally with logs as the safety net. The substrate makes boundary axioms explicit, signed, content-addressed, and auditable. A wrong axiom is a lemma a curator (or community of curators) can revoke or replace; the consequences propagate through the substrate's edge cache deterministically. But the substrate's strength is the leaf-discharge corner; the substrate's weakness, honestly named, is that it is only as good as the axioms signed at its boundaries. Boundary specification becomes the new high-leverage activity, the new place where small mistakes compose into large failures, and the new domain where curatorial discipline matters most. Paper 06's federation-of-trust framing is what carries this weight; this paper's claim about types and logs assumes paper 06's curatorial machinery is functioning.

**"You're asking developers to delete their type system / kill their log stack."** No. The diplomatic substrate framing in §10 says the opposite. Both stay. They get demoted from load-bearing-for-correctness to load-bearing-for-the-other-things-they-are-load-bearing-for. The change is in *expectation*, not in deletion.

**"This is just shifting where the proof obligation lives."** Yes, and that is the architectural point. The proof obligation today lives in three uncoordinated places: the type checker (catches shape), the test suite (catches sampled inputs), the log stack (catches what slipped through). Shifting the proof obligation to a content-addressed federated substrate means each obligation is discharged once, ecosystem-wide, and the discharge is reusable across projects, languages, and time. That is the leverage no prior arrangement had.

**"Mainstream developers will not adopt this."** This paper's claim is conditional. *If* the substrate ships, *if* it covers leaf-discharge bug classes, *if* foundation-catalog curation functions, *if* lifters and droppers exist for the languages the team uses, *and if* boundary axioms get the curatorial seriousness §9's earlier point demands, *then* types and logs lose their load-bearing role for correctness in the leaf-discharge corner. Each conditional is a real constraint and each is the subject of separate work (paper 04 on rollout, paper 06 on curation, paper 05 on transport, this paper's §9 on boundary axioms). What this paper argues is structural: under the conditionals, the paradigm shift is the natural shape. Adoption rate affects when the world reaches the after-types regime; structure determines what the regime looks like once reached. Both are real questions; this paper answers the second.

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

It is not a claim about all logs. Forensic logs (the confession category) shrink with the substrate. Operational logs (network partition, clock skew, partial failure, hardware fault, third-party flap) do not shrink because the substrate cannot prove non-determinism away. The bifurcation in §2 is the load-bearing distinction; the title's "stop logging" applies to forensic instrumentation aimed at prevention failures, not to operational telemetry aimed at irreducible runtime conditions.

## §12: Acknowledgments

This paper continues the After-X arc with paper 06 (*After Reputation*) and paper 07 (*After Verification*). The structural-elimination theorem and the algebraic frame come from paper 07. The federated trust model comes from paper 06. The vertical stack and standardization argument comes from paper 04. The witness-pluralism transport from paper 05. The substrate primitives from papers 01 and 02. This paper assembles those into a paradigm shift for what types and logs are *for*.

Edsger Dijkstra's *A Discipline of Programming* (1976) is the source of the WP-propagation framing the substrate operationalizes. Dijkstra argued that program correctness is a calculation; the substrate makes the calculation content-addressable.

Tony Hoare's *An Axiomatic Basis for Computer Programming* (1969) is the source of the precondition / postcondition contract framing the substrate inherits.

C. B. Jones, *Tentative Steps Toward a Development Method for Interfering Programs* (1983) is the source of the rely-guarantee framing the substrate's edges generalize.

The Cousot-Cousot abstract interpretation framework (1977) is what the substrate's lift-and-walk pipeline materializes at federated cache scale. Paper 07's §3 articulated this; this paper inherits it.

## §13: Citation

Cite as:

> Sugar Substrate Working Notes (2026). *After Types: How I Learned to Stop Logging and Trust the Invariant Solver*. Paper 08 of the After-X arc.
