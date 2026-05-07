# Lossy Boundary Compression: Why ProofIR Is Universal Because It Forgets

> **Status.** Draft whitepaper. Sustained argument. Contains a lemma. Written to be cite-able after review.
>
> **Companion to.** [01 Whitepaper](01-whitepaper.md), [02 Bluepaper](02-bluepaper.md), [03 Substrate, not Blockchain](03-substrate-not-blockchain.md), [04 Vertical Stack and Standardization](04-vertical-stack-and-standardization.md), [05 Witness Pluralism and Jurisdiction-Neutral Transport](05-witness-pluralism-and-jurisdiction-neutral-transport.md), [06 After Reputation](06-after-reputation-software-as-federated-truth-claims.md), [07 After Verification](07-after-verification-bug-classes-as-missing-edges.md), [08 After Types](08-after-types-stop-logging-trust-the-invariant-solver.md).
>
> **Premise the earlier papers established.** A protocol for content-addressable, cryptographically-signed, byte-deterministic claims about software behavior, federated across signers, composable end-to-end, jurisdiction-neutral, and machine-checkable. Papers 06 through 08 argued that once those claims become a substrate, reputation, verification, types, and logs are all demoted from load-bearing trust infrastructure into policy, tooling, and observability layers.
>
> **What this paper argues.** That ProofIR's universality over contract boundaries does not come from representing every implementation detail of every host language. It comes from refusing to. ProofIR is universal over contract boundaries: preconditions, postconditions, invariants, protocol obligations, value predicates, resource states, signer claims, and implication edges. Because the domain is narrow, lifters may discard implementation texture while preserving the obligation. That deliberate loss is what makes cross-language, cross-framework, cross-time equivalence possible. It also constrains generation: a probabilistic producer may emit many possible implementations, but only the outputs whose lifted boundary edges close are admissible.

## §0: The claim

The mistake to avoid is obvious and fatal: treating ProofIR as a universal programming language.

It is not that. ProofIR is not a second Java, a second TypeScript, a second Rust, a second OpenAPI schema language, a second JML, a second Cofoja, a second Zod, a second Pydantic, a second Spring reflection model, or a second test runner. It is not a place to re-express every implementation detail of every host artifact. It is not the full semantics of the source program in canonical costume.

ProofIR is universal in a narrower and stronger sense. It is universal over contract boundaries.

The contract-boundary domain includes:

- preconditions;
- postconditions;
- invariants;
- protocol obligations;
- value predicates;
- resource states;
- signer claims;
- implication edges.

That is the domain. Nothing else is promised.

The surprise is that "contract boundary" is a narrow kind of object and an enormous part of software. ProofIR is not universal over contract boundaries because the boundary domain is small. It is universal over contract boundaries because nearly every meaningful software obligation eventually appears at a boundary: a trust boundary, a call boundary, a protocol boundary, a resource boundary, an authority boundary, a privacy boundary, a concurrency boundary, a serialization boundary, a cryptographic boundary, an audit boundary, a business-rule boundary.

ProofIR does not cover less by refusing implementation semantics. It covers more, because the thing it keeps, the boundary obligation, is the part every language, framework, protocol, and organization already has to express somehow.

Within that domain, lossy compression is not a defect. It is the design. A lifter may discard implementation texture, framework idiom, annotation syntax, validator control flow, test harness structure, and historical commit context, as long as it preserves the boundary obligation. The obligation is the invariant-bearing part. The rest is authoring texture.

The more precise name is **obligation-preserving loss**. The lift is allowed to lose source detail only when the lost detail is outside the boundary obligation being preserved. "Lossy" here does not mean approximate. It means the equivalence relation is drawn at the contract boundary rather than at the full implementation.

This is what lets the same boundary predicate be lifted from a Spring annotation, a ProvekIt-native annotation, an OpenAPI schema, a Zod validator, and a historical OSS commit. The source implementations are not equivalent. The boundary obligations are.

The difference matters because adoption depends on it. ProvekIt-native contracts are a reference surface, not a required authoring style. Existing code already contains latent contracts in Spring, Bean Validation, Swagger/OpenAPI, Zod, Pydantic, JML, Cofoja, tests, types, schemas, comments, metadata, framework declarations, and old patches. ProvekIt's move is to lift those latent contracts into universal, comparable, solvable, translatable, content-addressable, signable ProofIR edges.

The substrate does not begin when developers agree to write ProvekIt-native annotations. The substrate begins when their existing boundary claims can be read.

## §1: The lemma

**Lemma (Lossy Boundary Compression / Output Constraint).** Let `S` be a source artifact in some host language or framework. Let `B(S)` be the set of boundary obligations expressed by `S`: preconditions, postconditions, invariants, protocol obligations, value predicates, resource states, signer claims, and implication edges. Let `L` be a lifter from `S` into ProofIR. Let `A` be the set of admissible outputs for some requested change, where admissibility is defined by closure of the required ProofIR boundary edges under an accepted witness policy.

If `L` preserves `B(S)` up to the canonical equivalence relation on boundary predicates, then:

1. `L` may discard every source-level feature outside `B(S)` without loss for any substrate operation whose semantics are defined only over boundary obligations; and
2. for any producer `P` that emits candidate artifacts `S'`, deterministic lift-and-check induces a constraint function `C(S') = edge_closed(L(S'))`, reducing `P`'s effective output space from all syntactically possible artifacts to the subset `A`.

Equivalently:

```
preserve_boundary(S) => may_forget_implementation_texture(S)
edge_closed(L(S')) => output_admissible(S')
```

provided the consumer asks only boundary-domain questions and output admissibility is defined over those questions.

The lemma is almost embarrassingly small. Its consequence is not.

ProofIR's operations are over predicates and edges. Predicate canonicalization, CID computation, implication checking, witness verification, signing, edge composition, policy acceptance, and dropper translation all operate on the boundary object. They do not require the source's complete implementation semantics. Therefore a lifter that preserves the boundary object has preserved all information relevant to those operations.

This is ordinary abstraction discipline, applied aggressively. If the interface contract is the object of study, the implementation is not part of the equivalence class. Two implementations can differ in timing, memory allocation, framework machinery, reflection paths, exception formatting, logging, helper functions, and code style while expressing the same boundary predicate. For contract-boundary purposes, they are the same.

The proof is one line:

For every substrate operation `O` defined only over `B(S)`, if `L1(S1) = L2(S2)` in canonical boundary form, then `O(L1(S1)) = O(L2(S2))`. For every generated candidate `S'`, if `edge_closed(L(S'))` is false, then `S' ∉ A` by definition of admissibility; if it is true under accepted witness policy, then `S' ∈ A` for boundary-domain purposes.

Everything else is consequence.

**Corollary (Probabilistic producers become admissibility-searched producers).** Once output acceptance is defined as `edge_closed(L(S'))`, an LLM is not a trusted authority over correctness. It is a search procedure over candidate artifacts. The substrate is the acceptance function.

This is the correct division of labor. The model explores the space of possible patches, schemas, validators, annotations, migrations, tests, and native contracts. The substrate rejects every candidate outside the admissible set. The model's probability distribution may be useful for finding candidates quickly; it is not part of the proof that any candidate is acceptable.

### §1.1: Why the output side matters

The first half of the lemma is about equivalence: different source surfaces can collapse to the same boundary object. The second half is about constraint: once the boundary object is canonical, it can reject candidate outputs.

That rejection property is the part that matters most in the LLM era.

A deterministic compiler emits one output for a given input. A probabilistic model emits a distribution over possible outputs. Some candidates are good. Some are subtly wrong. Some satisfy the user's prose but violate a boundary obligation the prose did not mention. Some preserve behavior but weaken validation. Some add a guard in the wrong layer. Some pass tests while expanding the accepted input domain. The model's fluency is not evidence of admissibility.

The substrate changes the question from:

```
Did the model produce plausible code?
```

to:

```
Does the model's output lift to boundary edges that close?
```

That is a codomain restriction. The model may sample from a huge space of syntactically plausible patches, schemas, validators, annotations, migrations, and tests. ProvekIt narrows that space after generation to the subset whose boundary predicates satisfy the required edges. The narrowing is not a prompt trick. It is not "be careful" in system-message form. It is a mechanical acceptance gate over lifted output.

This connects to the earlier constraint-driven-development spec: each fix mints a permanent constraint on what the codebase cannot become. It also connects to paper 07's generative-completion section: models may generate candidate code, but the substrate verifies mechanically whether the candidate closes the DAG. This paper adds the boundary-compression reason that loop can be cross-language and cross-framework. The model can output Spring, Zod, OpenAPI, Pydantic, JML, Cofoja, SQL migrations, tests, or ProvekIt-native declarations. The substrate does not need to trust the surface. It lifts the boundary obligation and checks the edge.

This is why obligation-preserving loss is stronger than faithful re-expression for AI-produced artifacts. A faithful host-language IR would inherit the model's implementation sprawl and make every candidate hard to compare. A boundary IR discards the sprawl and asks the only admissibility question that matters at the contract layer: did this output preserve or strengthen the required obligation?

In a probabilistic-output regime, the valuable object is not the model's preferred completion. The valuable object is the constraint that filters completions.

### §1.2: What the lemma does not say

The lemma does not say that implementation details are unimportant. They are important for performance, debugging, ergonomics, security side channels, resource consumption, exception shape, deployability, maintainability, and business logic.

The lemma says only that those details are outside the ProofIR boundary-domain unless they themselves are lifted as boundary obligations.

If a resource constraint matters, lift it as a resource-state predicate. If timing matters, lift it as a protocol or resource obligation. If authorization matters, lift it as a precondition. If mutation order matters, lift it as a state-transition invariant. If exception behavior matters to a caller, lift it as a postcondition. But do not pretend the entire host implementation must become ProofIR merely because some implementation detail matters somewhere.

ProofIR is not universal by being maximally expressive. It is universal by choosing the right cut.

## §2: The domain cut

The domain cut is the whole architecture.

Prior formal methods often fail adoption by asking developers to move their authoring surface into the verifier's preferred language. Write the spec in this syntax. Rewrite the function in that subset. Add annotations in the verifier's house style. Avoid framework features the tool cannot understand. Use the blessed encoding. The work may be sound, but the adoption path is narrow: become the kind of team that authors formal contracts directly.

ProvekIt reverses the posture.

The world already writes contracts. It just does not call them that consistently.

A Spring controller method with `@RequestParam @Min(1) @Max(100)` is a boundary contract. A Bean Validation DTO with `@NotNull`, `@Email`, and `@Size(max = 255)` is a boundary contract. An OpenAPI schema with `minimum`, `maxLength`, `required`, and `format: email` is a boundary contract. A Zod validator that says `z.string().email().max(255)` is a boundary contract. A Pydantic model with `Field(gt=0, le=100)` is a boundary contract. A JML clause `requires amount > 0; ensures balance == \old(balance) - amount;` is a boundary contract. A Cofoja `@Requires("x >= 0")` is a boundary contract. A regression test asserting that negative quantities are rejected is a boundary contract, albeit a sampled and weaker one. A historical OSS commit that adds a null check before a dereference is a boundary contract made visible by repair.

The authoring surfaces differ. The boundary obligation may not.

ProofIR lives at the cut where those surfaces collapse:

```
input.amount: integer
requires input.amount >= 1
requires input.amount <= 100
```

That predicate can arrive from an annotation, a schema, a validator, a native ProvekIt declaration, a test, or a patch. Once lifted, its origin is provenance, not semantics. The semantics are the canonical predicate and its implication edges.

This is why ProvekIt-native contracts must be understood as a reference surface, not a required authoring style. Native contracts matter because they show the substrate's object model cleanly. They give authors a direct way to write the thing itself. But requiring native authoring as the adoption path would throw away the largest contract corpus in existence: the contracts already embedded in ordinary code.

The substrate does not need developers to become formal-methods people before it can read them. It needs lifters that can see the formal boundary claims they are already making.

## §3: Java equivalence without Java equivalence

Consider a Java service boundary requiring an account transfer amount to be positive and no larger than a configured limit.

In Bean Validation:

```java
record TransferRequest(
    @NotNull
    @Min(1)
    @Max(10000)
    Long amount
) {}
```

In Spring MVC:

```java
@PostMapping("/transfer")
ResponseEntity<?> transfer(
    @RequestParam @Min(1) @Max(10000) long amount
) { ... }
```

In JML:

```java
/*@ requires amount >= 1 && amount <= 10000;
  @ ensures \result.accepted ==> balance == \old(balance) - amount;
  @*/
TransferResult transfer(long amount) { ... }
```

In Cofoja:

```java
@Requires("amount >= 1 && amount <= 10000")
@Ensures("result.accepted() ==> balance() == old(balance()) - amount")
TransferResult transfer(long amount) { ... }
```

These artifacts are not equivalent as Java. They have different toolchains, retention policies, runtime behavior, reflection surfaces, failure modes, exception types, annotation processors, and enforcement points. A Spring parameter annotation is not a JML method contract. A Bean Validation DTO is not a Cofoja-instrumented method. A JML specification may be statically checked by tooling that never runs in production. A Spring annotation may be enforced by runtime binding.

ProofIR does not need them to be equivalent as Java.

It needs to know whether they express the same boundary obligation:

```
requires amount != null
requires amount >= 1
requires amount <= 10000
```

and, if present:

```
ensures accepted(result) -> balance_after = balance_before - amount
```

The lifter is allowed to forget that one source used Java annotations, another used specification comments, another used annotation instrumentation, and another used framework binding. It is allowed to forget the exact exception class thrown on failure. It is allowed to forget whether validation occurred before controller dispatch or inside an instrumented method wrapper, unless that enforcement phase is itself the boundary obligation under analysis.

That forgetting is what creates equivalence.

The same predicate CID can be shared by:

- a Spring endpoint in a bank;
- a JML-specified method in a university verification suite;
- a Cofoja contract in an old enterprise Java codebase;
- a Bean Validation DTO in a payment processor;
- a ProvekIt-native declaration in a new service.

The Java implementations are not equivalent. The boundary predicate is.

Once they lift to the same predicate CID, their implication edges can be compared, solved, signed, and reused. If one organization has a signed witness that `1 <= amount <= 10000` implies `amount > 0`, every other organization can reuse that edge, regardless of whether its source surface was Spring, JML, Cofoja, or ProvekIt-native.

That is the substrate doing the thing. It is not translating Java into another Java. It is extracting the obligation from Java-shaped surfaces and placing that obligation into a shared contract-boundary edge space.

## §4: TypeScript, OpenAPI, and the same edge

Now cross the language boundary.

A TypeScript service might define:

```ts
const TransferRequest = z.object({
  amount: z.number().int().min(1).max(10000),
  destination: z.string().uuid()
});
```

The public API might also publish:

```yaml
components:
  schemas:
    TransferRequest:
      type: object
      required: [amount, destination]
      properties:
        amount:
          type: integer
          minimum: 1
          maximum: 10000
        destination:
          type: string
          format: uuid
```

A Python implementation might use Pydantic:

```python
class TransferRequest(BaseModel):
    amount: int = Field(ge=1, le=10000)
    destination: UUID
```

Again, these artifacts are not equivalent as implementations. Zod executes as TypeScript/JavaScript validation code. OpenAPI is a schema document. Pydantic is Python runtime validation plus model construction. Their coercion behavior can differ. Their error messages can differ. Their default handling can differ. Their unknown-field policy can differ. Their integer semantics can differ at the host-language edge.

ProofIR must not erase those differences if those differences are part of the boundary obligation. If coercion is allowed in one surface and forbidden in another, that is a predicate difference. If unknown fields are rejected in one and ignored in another, that is a predicate difference. If JavaScript number precision changes the accepted domain, that is a predicate difference.

But where the boundary obligation is the same, the canonical predicate is the same:

```
requires integer(amount)
requires 1 <= amount
requires amount <= 10000
requires uuid(destination)
```

The lossy compression rule is not "pretend all validators are the same." It is "preserve the boundary predicate and discard everything else."

That discipline gives us cross-language call-boundary verification.

Suppose a Java service calls a TypeScript service through HTTP. The Java caller has a Bean Validation object. The TypeScript callee has a Zod validator. The OpenAPI document sits between them as the published protocol. Today, teams ask whether the generated client is up to date, whether the schema matches the implementation, whether the runtime validator is stricter than the docs, whether the caller's DTO drifted from the callee's accepted shape.

In the substrate, each surface lifts to ProofIR:

```
Java DTO predicate      -> CID A
OpenAPI schema predicate -> CID B
Zod predicate           -> CID C
```

If `A = B = C`, the surfaces express the same boundary obligation.

If `A -> B` but not `B -> A`, the caller is stricter than the published schema.

If `B -> C` but not `C -> B`, the implementation is weaker than the published schema.

If no implication holds in one direction, the boundary has drift.

This is the ordinary work API teams already do by review, integration tests, generated clients, and staging failures. The substrate turns it into signed implication edges.

The result is not "Java and TypeScript are equivalent." They are not. The result is "this Java caller's outbound predicate implies this TypeScript callee's inbound predicate." That is the contract-boundary question, and it is exactly the question ProofIR is built to answer.

## §5: Latent contracts and adoption

The adoption corollary is more important than the technical point.

If ProvekIt required native annotations everywhere, adoption would depend on a rewrite of developer behavior. That is the wrong shape. The world's software supply chain is not going to stop and re-author itself in a new contract language before receiving value.

The better claim is stronger:

Most useful contracts already exist. They are latent.

They appear as framework annotations, schemas, validators, type refinements, tests, examples, CI checks, database constraints, migration files, sanitizer wrappers, linter rules, static analyzer suppressions, comments attached to dangerous APIs, issue fixes, and old commits that patched bugs by adding guards.

A database migration:

```sql
ALTER TABLE users
  ADD CONSTRAINT email_present CHECK (email IS NOT NULL),
  ADD CONSTRAINT email_unique UNIQUE (email);
```

is a boundary contract.

A test:

```ts
expect(() => TransferRequest.parse({ amount: 0 })).toThrow();
```

is a sampled boundary contract:

```
rejects amount = 0
```

which is weaker than:

```
requires amount >= 1
```

but still useful as evidence.

A historical patch:

```diff
- return read(buf, len);
+ if (len > MAX_PACKET) return -EINVAL;
+ return read(buf, len);
```

is a boundary contract discovered by repair:

```
requires len <= MAX_PACKET before read
```

The patch does not need to have been authored as a specification. It became one by fixing a missing obligation.

This is the lift-not-author posture. ProvekIt-native contracts are clean and useful, but the substrate's bootstrapping path is extraction. Read what exists. Canonicalize the boundary predicate. Preserve provenance. Sign the edge. Make it comparable.

This is also why lossy compression must be permitted. A lifter that tries to preserve all source texture becomes a host-language reimplementation project. A lifter that preserves boundary obligations becomes feasible, useful, and compositional. The difference between those two projects is the difference between never shipping and becoming infrastructure.

## §6: Constraint-driven development needs a boundary cut

The constraint-driven-development spec gave ProvekIt its development methodology: every fix mints a permanent constraint on what the codebase cannot become. The change request is the trigger; the constraint is the product; the accumulated constraint corpus monotonically reduces the codebase's degrees of freedom.

This paper supplies the boundary-theoretic reason that methodology can work across ordinary software.

CDD needs three facts at once:

1. A constraint must be stronger than a test because it must constrain paths that do not exist yet.
2. A constraint must be liftable from existing artifacts because adoption cannot wait for everyone to author native contracts.
3. A constraint must be able to reject generated output because more and more code will be produced by probabilistic tools.

Lossy boundary compression is the thing that lets all three coexist. The constraint is not "the implementation must keep looking like this patch." The constraint is "future artifacts must preserve or strengthen this boundary obligation." That is why a bug fix in Java can constrain a future TypeScript validator, why a database migration can constrain a future API schema, and why an LLM-generated patch can be accepted or rejected without trusting the model.

The CDD loop becomes:

```
problem or patch -> lifted boundary obligation -> signed constraint edge
future candidate -> lift -> edge closure check -> accept or reject
```

The important object is the rejection surface. A codebase under CDD is not merely accumulating facts about itself. It is accumulating impossibilities. Every accepted constraint removes a region from the future output space. The more probabilistic the producer, the more valuable that rejection surface becomes.

### §6.1: A concrete LLM admissibility example

Suppose an LLM is asked to fix SQL injection in a TypeScript service. It proposes three plausible patches.

Patch A escapes strings:

```ts
const q = `select * from orders where user_id = '${escapeSql(input)}'`;
await db.query(q);
```

Patch B uses a prepared statement:

```ts
await db.query("select * from orders where user_id = ?", [input]);
```

Patch C rejects suspicious input with a regex:

```ts
if (!/^[0-9]+$/.test(input)) throw new Error("bad input");
const q = `select * from orders where user_id = ${input}`;
await db.query(q);
```

All three may look plausible to a reviewer skimming for "SQL injection fixed." All three may pass the narrow regression test the model wrote. All three may satisfy the user's prose request at a surface level.

The substrate does not ask whether the patch looks plausible. It lifts each candidate and checks the required boundary edge:

```
untrusted(input) -> safe_for_sql(query)
```

Under a policy that accepts parameterization but does not accept ad-hoc escaping or regex validation as a general SQL-safety proof, only Patch B closes:

```
untrusted(input) -> parameterized_query(query) -> safe_for_sql(query)
```

Patch A may close only if `escapeSql` has an accepted signer, precise dialect scope, and an edge from its output to `safe_for_sql` for this sink. Patch C may close a numeric-domain predicate but still fail the SQL-safety edge unless the query construction rule and database dialect make that implication valid under policy.

The point is not that prepared statements are always the only acceptable fix. The point is that admissibility is not the model's confidence, the prettiness of the code, or the presence of a test. Admissibility is edge closure under policy. The LLM generated candidates. ProvekIt constrained the output set.

This is the clean CDD posture for probabilistic software production: let the model search, let the substrate reject.

This is also the right way to understand droppers. A dropper is not a compiler from ProofIR to a host language. It is a host-language repair emitter for a missing boundary edge. It may render a native guard, wrapper, validator, prepared statement, state transition, or annotation, but the rendered source is not trusted by virtue of being rendered. It is accepted only after the lifter reads it back and the verifier confirms that the emitted native shape closes the graph.

The direction matters:

```
missing edge -> native repair candidate -> re-lift -> closure check
```

not:

```
ProofIR + values -> host implementation
```

The first is generative completion under constraint. The second would turn ProofIR into a universal programming language by another route, which is precisely the category error this paper rejects.

## §7: Content addressing turns sameness into infrastructure

Once lifted, a boundary predicate has bytes. Once it has bytes, it has a CID. Once it has a CID, sameness is no longer a social claim.

The sentence "our OpenAPI schema matches our Zod validator" becomes:

```
cid(openapi_predicate) == cid(zod_predicate)
```

or, if one is intentionally stricter:

```
edge(openapi_predicate -> zod_predicate)
edge(zod_predicate -> openapi_predicate) absent
```

depending on policy.

The sentence "this Java caller satisfies that TypeScript callee" becomes:

```
edge(java_outbound_predicate -> ts_inbound_predicate)
```

The sentence "this fix closes the old bug" becomes:

```
edge(pre_patch_state -> required_sink_precondition)
```

with the post-patch program supplying an intermediate predicate that makes the path exist.

This is the operational consequence of the lemma. Lossy boundary compression produces stable predicate identity. Stable predicate identity enables reusable implication edges. Reusable implication edges make the substrate federated.

Without loss, no identity. If every annotation-retention policy, framework exception type, validator stack trace, AST location, source formatting choice, and helper-function name remains part of the canonical object, then two artifacts almost never hash together. The cache fragments. Cross-language equivalence fails. The substrate degenerates into one silo per framework.

With disciplined loss, the cache compounds. The same predicate lifted from Spring, ProvekIt-native annotations, OpenAPI, Zod, Pydantic, JML, Cofoja, tests, and historical commits lands in the same address space. Every signed edge against that predicate becomes reusable.

The substrate's economic claim rests on this. The global lemma cache works only if common obligations collapse to common addresses. Common addresses require forgetting everything outside the obligation.

Forgetfulness is not a concession. It is the compression function.

### §7.1: What stable boundary identity buys

Stable boundary identity is not merely a nice property of the IR. It changes the operating model around software.

**API drift becomes algebraic.** Today, API drift is discovered by broken clients, stale generated SDKs, integration-test failures, schema diff tools, and production incidents. Under boundary compression, drift is an implication result. The published schema, the caller's outbound object, the callee's runtime validator, and the database constraint each lift to predicates. Compatibility is equality or implication. Incompatibility is a missing edge. The question "did the API break?" stops being a meeting and becomes a substrate query.

**Framework migration stops being a trust reset.** A team migrating from Spring MVC to Micronaut, from Java to Kotlin, from Express to Fastify, from Pydantic v1 to Pydantic v2, or from hand-written validators to Zod does not need to re-establish every contract socially. The old surface and the new surface both lift. If their boundary predicates match, the migration preserved the contract boundary even though the implementation changed. If the predicates differ, the substrate names the difference. Migration review moves from "does this look equivalent?" to "which boundary CIDs changed, and were those changes intended?"

**Security review moves to the edge where security lives.** Many application-security failures are boundary failures: untrusted input treated as trusted, unchecked resource state reaching a sink, missing authorization predicates before sensitive operations, protocol states skipped. Lossy boundary compression makes those predicates first-class. Reviewers no longer have to infer the security posture from framework texture. They can ask for the edge: does `caller_claims(role=admin)` imply `may_execute(refund)` under the accepted signer policy? Does `untrusted(body.email)` imply `email(body.email)` before persistence? Does `maybe_closed(handle)` imply `open(handle)` before read? The review object becomes the obligation, not the host-language ritual that happened to express it.

**Compliance evidence becomes portable.** A regulatory control often says, in prose, that some boundary condition must hold: reject malformed requests, enforce authorization before access, retain audit claims, validate signed messages, preserve consent state. Today each implementation produces its own evidence packet: screenshots, logs, test reports, code review links, policy documents. With stable boundary identity, the evidence can point at signed predicates and signed implication edges. The auditor does not need to believe that a Spring annotation, a Zod validator, and an OpenAPI schema are "basically the same." The auditor can verify whether they lift to the same predicate or whether signed implication edges connect them.

**Droppers get a target, not a vibe.** A dropper cannot safely generate "the code that seems right." It needs a missing edge. Lossy boundary compression gives it one. If the substrate says the missing edge is `untrusted(query) -> safe_for_sql(query)`, the dropper may choose a host-language idiom: prepared statement in Java, parameterized query in Python, tagged SQL template in TypeScript, query builder in Ruby. The generated code differs; the target edge is the same. This is what makes cross-language repair possible without pretending languages share implementation semantics.

**Standard libraries become lemma libraries.** Once boundary predicates have stable addresses, every framework's common validators and guards become reusable theorem material. `@Email`, `z.string().email()`, `format: email`, `EmailStr`, and a ProvekIt-native `email(x)` predicate can all point at the same obligation when their accepted domains align. The common validators stop being isolated helpers and become entry points into a shared implication graph.

These are not downstream conveniences. They are the reason the lemma matters. If the lifter preserves too much, none of this composes. If it preserves too little, it is unsound. The engineering art is preserving exactly the boundary obligation, then letting the consequences compound.

## §8: Signing preserves provenance without polluting semantics

Forgetting source texture does not mean losing provenance.

The boundary predicate and the provenance record are different objects.

The predicate says:

```
requires amount >= 1
requires amount <= 10000
```

The provenance record says:

```
lifted_from:
  kind: spring_request_param
  repository: example/payment-service
  path: src/main/java/.../TransferController.java
  commit: ...
  signer: ...
  lifter: provekit-java-kit
```

or:

```
lifted_from:
  kind: openapi_schema
  path: openapi.yaml#/components/schemas/TransferRequest
  signer: ...
```

The predicate's CID must not change merely because the source path, repository, lifter version, or signer changes. Those are provenance facts. They matter for policy. They do not define the boundary predicate.

This separation is load-bearing.

If provenance is mixed into predicate identity, the same obligation lifted from two places becomes two predicates. The cache splits. Equivalence becomes expensive. Federation weakens.

If provenance is discarded entirely, consumers cannot decide which edges to trust. A predicate lifted from an audited Spring service, a generated OpenAPI file, a hand-written test, and an unreviewed fork may be semantically identical, but policy may treat their signatures differently.

The substrate needs both:

- predicate identity by canonical boundary bytes;
- trust decisions by signed provenance and witness policy.

That is the same move paper 06 made for reputation. Reputation leaves the substrate and moves to policy. Here, source texture leaves predicate identity and moves to provenance. The substrate stays clean; consumers stay sovereign.

## §9: The cross-time case

Cross-language equivalence is only half the consequence. The same lemma gives cross-time equivalence.

A historical OSS commit that fixes a vulnerability often has the shape:

1. an unsafe boundary was accepted;
2. a sink required a predicate;
3. the old path did not prove it;
4. the patch inserted a guard, sanitizer, state transition, or resource check;
5. the new path proves it.

The commit is not written as a proof. It is written as code. It has review comments, issue references, release notes, test additions, and maybe a CVE. But under the boundary-domain cut, it contains a before-and-after obligation.

Example:

```diff
- if (user.role == "admin") allow();
+ if (user != null && user.role == "admin") allow();
```

At the boundary level, the patch exposes:

```
requires user != null before role dereference
```

Another project in another language may have the same missing obligation:

```ts
if (user.role === "admin") allow();
```

The source artifacts are not equivalent. The historical Java patch and the current TypeScript code do not share implementation semantics. But the missing boundary edge is the same:

```
maybe_null(user) -> non_null(user)
```

If the historical patch has been lifted, witnessed, and signed, it becomes a receipt for the obligation. Not a proof that the TypeScript code is safe. A receipt that this class of boundary obligation has appeared before, was repaired before, and has a canonical edge shape the substrate can test against.

This is how software history becomes infrastructure. Old fixes stop being prose in changelogs and become signed edges in a federated proof DAG.

## §10: Bug Zoo as receipt infrastructure

Bug Zoo belongs here, but carefully.

Bug Zoo is not the design center of this paper. The design center is lossy boundary compression. Bug Zoo is the empirical receipt infrastructure that makes the claim falsifiable at scale.

A Bug Zoo entry can carry:

- the pre-fix artifact;
- the post-fix artifact;
- the lifted pre-fix boundary predicates;
- the lifted post-fix boundary predicates;
- the missing edge exposed by the bug;
- the edge closed by the fix;
- the witnesses accepted by policy;
- the signers who attest the lift, the witness, and the classification.

That turns a bug corpus into more than examples. It becomes a receipt set for boundary obligations.

For a SQL injection fix, the receipt says:

```
pre-fix:  untrusted(input) reaches execute_query requiring safe_for_sql(query)
missing:  untrusted(query) -> safe_for_sql(query)
post-fix: untrusted(input) -> parameterized_query(query) -> safe_for_sql(query)
```

For a null dereference fix:

```
pre-fix:  maybe_null(user) reaches dereference requiring non_null(user)
missing:  maybe_null(user) -> non_null(user)
post-fix: maybe_null(user) -> checked_non_null(user) -> non_null(user)
```

For a resource-state fix:

```
pre-fix:  maybe_closed(handle) reaches read requiring open(handle)
missing:  maybe_closed(handle) -> open(handle)
post-fix: maybe_closed(handle) -> checked_open(handle) -> open(handle)
```

These receipts do not prove that every future program is safe. They prove something narrower and more useful: this boundary obligation shape exists in real software history, this source repair closed it, this lift preserved the obligation, and this edge can now be looked up by CID.

Bug Zoo is the empirical answer to "are these predicates real or invented?" It grounds the substrate in the bug record. It lets the ecosystem say: here is the exact obligation that was missing in real code; here is the patch that closed it; here is the canonical edge; here are the signatures.

The zoo is not a design doc. It is the receipt drawer.

The implication is that empirical software history becomes a prioritization engine for the substrate. Edges that recur across Bug Zoo entries deserve foundation-catalog treatment. Predicates that appear in many languages deserve first-class vocabulary. Lifters that fail to recover known historical obligations can be tested against the receipt set. Droppers that claim to repair a class can be evaluated by whether their output closes the same edge the historical fix closed.

This is how the substrate avoids becoming a beautiful taxonomy detached from practice. The bug corpus pushes back. If a predicate never appears in real repairs, maybe it is not foundational. If a missing edge appears in a hundred ecosystems, it belongs near the root catalog. If a lifter preserves source texture but misses the obligation the patch actually repaired, the lifter is wrong. Bug Zoo turns the lossy-compression lemma into an empirical test: did the loss preserve the edge that mattered?

## §11: Counterarguments

### "Lossy compression is unsound."

Lossy compression is unsound when the discarded information is relevant to the question being asked.

This paper's lemma is explicitly conditional: the lifter may discard source features outside `B(S)` for operations defined only over `B(S)`. If an implementation feature affects the boundary obligation, it is inside `B(S)` and must be preserved. If it does not affect the boundary obligation, retaining it in predicate identity is noise.

Unsoundness comes from drawing the boundary wrong, not from loss as such.

### "But source implementations really differ."

Yes. That is the point.

A Spring annotation, a ProvekIt-native annotation, an OpenAPI schema, a Zod validator, and a historical OSS commit are not equivalent source artifacts. They differ dramatically. The claim is not source equivalence. The claim is boundary-obligation equivalence.

The source implementations are not equivalent; the boundary obligations are.

### "OpenAPI and Zod differ on coercion, defaults, and unknown fields."

Then the lifted predicates differ.

Lossy boundary compression does not license vague matching. It requires precise preservation of the boundary obligation. If Zod coerces `"3"` into `3` and the OpenAPI contract does not, that is a predicate difference. If Pydantic accepts a default that the published schema marks required, that is a predicate difference. If one validator rejects unknown fields and another preserves them, that is a predicate difference.

The point is not to collapse non-equivalent boundaries. The point is to collapse equivalent obligations despite different authoring surfaces.

### "Tests are not contracts."

Tests are not full contracts. They are evidence about contracts.

A test that rejects `amount = 0` does not by itself prove `amount >= 1`. It proves a sampled fact: `amount = 0` is outside the accepted set. A cluster of property-based tests may imply a stronger conjecture. A human or solver may promote that evidence into a signed predicate. But the substrate must preserve the difference between sampled evidence and universal obligation.

This is another place where provenance matters. A predicate lifted from a formal JML `requires` clause, an OpenAPI schema, and one unit test may canonically state the same boundary only if policy accepts the lifting evidence. The predicate identity can be the same; the trust in its provenance can differ.

### "Native ProvekIt annotations are clearer."

Yes. They are the clean reference surface.

For new code that wants maximum explicitness, ProvekIt-native contracts may be the best authoring style. The point is adoption, not aesthetics. The substrate cannot require the world to begin there. It must be able to lift latent contracts first and let native authoring grow where teams want it.

### "ProofIR is too narrow."

Narrowness is the universality mechanism.

A universal host-language IR would have to model every object system, exception system, macro system, reflection system, effect system, runtime library, concurrency primitive, memory model, and framework lifecycle. It would fail by ambition.

A universal boundary IR has a smaller job: represent the obligations that cross boundaries and the implication edges between them. That job is hard, but it is the right hard problem. It is exactly where correctness, security, API compatibility, signing, and supply-chain trust meet.

### "What about business logic?"

Business logic matters when it is made contractual.

"Premium users receive a 10% discount" can be a postcondition. "A transfer may not exceed the account balance" can be a precondition or invariant. "A refund may occur only after settlement" can be a protocol obligation. If business logic remains informal, ProofIR cannot prove it. If it becomes a boundary obligation, ProofIR can carry it.

The substrate does not read minds. It reads contracts.

## §12: Why this matters now

The next adoption fight is not whether formal contracts are good. That argument is over among people who have debugged enough production systems. The fight is whether formal contracts can be introduced without making every team change its authoring style first.

Lossy boundary compression is the adoption bridge.

It says:

- Keep your Spring annotations.
- Keep your Bean Validation DTOs.
- Keep your OpenAPI schemas.
- Keep your Zod validators.
- Keep your Pydantic models.
- Keep your JML and Cofoja where you already use them.
- Keep your tests.
- Keep your database constraints.
- Keep your historical patches.
- Add ProvekIt-native contracts where they make sense.

The substrate will read the boundary obligations underneath those surfaces and place them into the shared edge space.

That is a more radical claim than "write contracts in our syntax." It is also a more practical one. The world is already saturated with boundary claims. The missing piece is not author intent. The missing piece is canonicalization.

Once canonicalized, those claims become comparable. Once comparable, they become solvable. Once solvable, they become translatable. Once translated, they become droppable. Once content-addressed, they become durable. Once signed, they become trust-carrying. Once federated, they become infrastructure.

The first move is forgetting the right things.

The larger implication is that ProvekIt can become useful before it becomes culturally dominant.

That is rare. Most verification systems require cultural conversion before first value: write in this language, adopt this proof assistant, annotate in this style, restrict this dynamic feature, train the team in this logic. The value arrives after the organization has paid the conversion cost.

Lossy boundary compression reverses the curve. A team can get value from contracts it already wrote accidentally. The first useful artifact might be an OpenAPI file, not a ProvekIt-native annotation. The first signed edge might come from a historical bug fix, not a new proof effort. The first cross-service compatibility check might compare a Java DTO to a Zod validator, not two formal specs. The first compliance packet might cite lifted framework annotations, not a hand-authored proof document.

This changes the adoption politics. ProvekIt-native authoring becomes the high-fidelity path for teams ready to be explicit. Lifting becomes the low-friction path for everyone else. The two paths meet in the same ProofIR edge space. Early adopters do not strand their work in a private formalism; late adopters do not have to rewrite their software before joining the substrate.

It also changes the supply-chain story. A dependency does not need to expose its source implementation to expose its boundary obligations. A vendor can sign the predicates its package promises at public boundaries. A consumer can check whether local calls imply those predicates. A distributor can compare version `n` and version `n+1` by the predicates that changed. A regulator can ask for signed boundary claims without demanding the proprietary implementation behind them. Lossy compression is what makes that politically possible: the compressed object is small enough to share and precise enough to verify.

And it changes standardization. Standards bodies do not need to standardize every host language's contract syntax. They need to standardize the boundary object, its canonical bytes, its implication-edge form, its witness envelope, and its signing/provenance discipline. Spring, Zod, OpenAPI, Pydantic, JML, Cofoja, and ProvekIt-native annotations can keep evolving. The standard sits beneath them at the obligation layer. That is a tractable standardization target.

This is the paper's strategic claim: the substrate wins not by replacing the world's authoring surfaces, but by making them comparable after the fact.

## §13: What this paper is NOT

It is not a claim that ProofIR represents all implementation semantics. It does not. That is the whole point.

It is not a claim that source artifacts with the same lifted predicate are operationally identical. They are not. A Spring validator, a Zod validator, an OpenAPI schema, a JML clause, and a historical patch can share a boundary predicate while differing in execution, ergonomics, timing, failure behavior, and enforcement.

It is not a license for sloppy lifting. Obligation-preserving loss is precise: anything relevant to the boundary obligation must be preserved; anything outside it belongs in provenance, policy, host implementation, or some other predicate if it matters.

It is not a claim that ProofIR plus a set of values compiles into host-language implementation. Droppers emit native repair candidates for missing boundary edges; their output is accepted only after re-lift proves closure.

It is not a mandate to author ProvekIt-native contracts. Native contracts are the reference surface and often the cleanest one. They are not the required starting point for adoption.

It is not a Bug Zoo design document, and it is not a claim that trust disappears. Bug Zoo appears as receipt infrastructure; signing, provenance, witness policy, and curator choice remain essential.

## §14: The compressed future

The substrate's future is not a world where every codebase looks ProvekIt-native.

It is a world where a Java annotation, a TypeScript validator, an OpenAPI schema, a Python model, a formal method contract, a database constraint, a unit test, and a ten-year-old vulnerability patch can all point at the same boundary predicate.

It is a world where a caller in one language proves it satisfies a callee in another without either side pretending their implementations are equivalent.

It is a world where API drift is an implication failure, not a staging incident.

It is a world where a bug fix becomes a signed receipt for the missing edge it closed.

It is a world where the proof substrate gets stronger because old software was already full of contracts waiting to be lifted.

That world requires ProofIR to forget. Not carelessly. Not vaguely. Precisely.

ProofIR is universal over contract boundaries because it is narrow. It composes because its loss is obligation-preserving. It can compare the world's software because it refuses to carry the world's implementation texture into the identity of a boundary obligation.

The compressed object is not the program.

The compressed object is the promise the program makes at its boundary.

## §15: Citation

Cite as:

> ProvekIt Substrate Working Notes (2026). *Lossy Boundary Compression: Why ProofIR Is Universal Because It Forgets*. Paper 09 of the After-X arc.
