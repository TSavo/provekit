# ProofIR Realization Compiler

**Status:** v0.1.0 design draft
**Date:** 2026-05-06
**Layer:** extension design over ProofIR, ORP, CDD, lifters, and host kits
**Related:**
- `2026-04-27-constraint-driven-development.md` - constraint corpus as product; patch as conditional infrastructure
- `2026-05-06-obligation-realizer-protocol.md` - shared witnesser/dropper/monitor protocol shape
- `2026-04-30-lift-plugin-protocol.md` - host artifact to canonical ProofIR
- `2026-04-30-ir-compiler-protocol.md` - canonical ProofIR to solver dialects; distinct from this spec
- `2026-05-02-ir-compiler-protocol-v2.md` - opacity-aware solver compiler protocol
- `2026-05-06-checker-bytecode-protocol.md` - executable checker targets
- `2026-05-06-fix-receipt-protocol.md` - accepted repair evidence
- `docs/papers/09-lossy-boundary-compression.md` - boundary-domain loss and output constraint
- `docs/papers/11-after-commits-proof-carrying-change.md` - fixes as proof-carrying change

## Section 0. Purpose

This document defines the architectural role of a ProofIR realization
compiler: a compiler that turns a ProofIR obligation into one or more
candidate artifacts that satisfy that obligation under an explicit target
surface and policy.

The name is intentionally narrow. This is not the existing IR compiler
protocol. The existing IR compiler translates canonical ProofIR into
solver dialects such as SMT-LIB, TPTP, Gallina, Lean, and Isabelle/HOL.
That compiler answers:

```
How do we ask this prover about this formula?
```

The realization compiler answers:

```
What artifact would make this obligation true?
```

The realization compiler is also not a compiler from ProofIR to complete
application behavior. ProofIR is a canonical language for contract
boundaries. It names obligations: preconditions, postconditions,
invariants, protocol obligations, value predicates, resource states,
signer claims, and implication edges. A realization compiler works in
that domain. It emits artifacts whose observable boundary behavior
discharges those obligations.

The artifact MAY be source code. It MAY be bytecode. It MAY be a schema,
validator, wrapper, guard, witness value, fixture, protocol body, checker,
or `.proof` envelope body. It is accepted only after the normal ProvekIt
loop reads it back or witnesses it under policy.

The slogan:

```
Code is a satisfying assignment to a constraint system.
```

## Section 1. The Realization Lemma

**Lemma (artifact realization as constraint solving).** Given a ProofIR
obligation `O`, host context `H`, target surface `T`, and policy `P`, a
realization compiler is sound only insofar as it searches for artifacts
`A` satisfying:

```
conforms(A, T)
and binds(A, H)
and discharges(A, O)
and acceptable(A, P)
```

The emitted artifact is not trusted because it was emitted. It is
accepted only when an independent validation path establishes the
required discharge.

For an implication obligation:

```
p -> q
```

the realization problem is:

```
find A such that:
  under p,
  A entails q,
  A conforms to target surface T,
  A preserves the binding map B,
  A is admissible under policy P,
  and lift_or_witness(A) produces closure for p -> q.
```

The compiler MAY use templates, symbolic search, SMT, enumeration,
counterexample-guided refinement, type-directed synthesis, heuristic
search, or LLM-generated candidates. None of those strategies is a trust
anchor. They are proposal mechanisms. The trust anchor is closure under
policy.

### Section 1.1 Constraint-driven origin

Constraint-driven development says:

```
The constraint is the product.
The patch is conditional infrastructure.
```

The realization compiler is the generalized patch stage from that loop.
If the current host artifact already satisfies the constraint, the right
output is a witness. If it does not, the compiler searches for an
artifact that would satisfy it.

```
constraint + host context
  -> witness if already true
  -> satisfying candidate if not true
```

This distinction matters. A run that emits no source patch but produces
a durable witness has succeeded. A run that emits source text but cannot
produce a witness has not succeeded. The important object is the
admissible region carved out of future outputs.

### Section 1.2 Output-space reduction

In ordinary probabilistic code generation, the output space is every
syntactically plausible program the generator might write.

In ProvekIt realization, the output space is:

```
{ A | conforms(A, T) and binds(A, H) and discharges(A, O) and acceptable(A, P) }
```

This is the same CDD posture described in the lossy boundary compression
paper: let the generator search; let the substrate reject.

The more probabilistic the producer, the more valuable the rejection
surface becomes.

### Section 1.3 Proof-first realization

The most general form of realization is proof-first.

For an implication:

```
p -> q
```

the compiler may equivalently target:

```
not(q) -> not(p)
```

or:

```
unsat(p and not(q))
```

This is the contrapositive form of the same boundary obligation. It is
also the operational shape of bug repair. A bug witness is a concrete or
symbolic inhabitant of:

```
p and not(q)
```

A correct realization eliminates that inhabitant class.

Therefore the compiler's most portable intermediate product is not a
Java patch, TypeScript patch, Rust patch, WASM module, or EVM checker.
It is a target-neutral proof or proof plan that explains how the
violation condition is made impossible.

```
violation condition: p and not(q)
proof objective:     unsat(p and not(q))
realization target:  artifact A that makes that proof true in host space
```

Language-specific generation then becomes projection:

```
proof plan -> Java guard
proof plan -> Zod validator
proof plan -> Rust match
proof plan -> OpenAPI schema refinement
proof plan -> SQL constraint
proof plan -> WASM checker
proof plan -> EVM checker
proof plan -> .proof body
```

The same proof plan can have multiple target realizations. The source
implementations are not equivalent. The discharged boundary obligation
is.

Proof-first realization is still not trusted by construction. A proof
plan is accepted only when its cited proof validates and the emitted
artifact is shown to realize that proof under the target surface and
policy.

Proof-first realization is policy-selectable but strongly preferred. A
policy MAY permit direct candidate generation without a proof plan, but
that mode is degraded evidence. It can reach a destination; it cannot
claim the same safety posture. The candidate still must pass syntax,
binding, lift or witness, closure, and policy acceptance, but the run
lacks the target-neutral explanation of why the forbidden region was
eliminated.

Policies SHOULD distinguish:

| Mode | Meaning |
|---|---|
| `proof_required` | Reject candidates without a validating `ProofPlan`. |
| `proof_preferred` | Prefer proof-first candidates; accept proofless candidates only with explicit degraded-evidence marking. |
| `proof_optional` | Permit direct candidate generation when target evidence is otherwise sufficient. |

High-risk targets SHOULD use `proof_required`. Interactive preview MAY
use `proof_preferred`. Exploratory local tools MAY use `proof_optional`.

### Section 1.4 Language-general projection

Proof-first realization makes the system generalizable across languages
because the proof objective is target-neutral.

The proof plan says:

```
this forbidden region is empty
```

or:

```
this eliminator removes every inhabitant of p and not(q)
```

The language backend says:

```
here is how this target surface expresses that eliminator
```

This turns per-language support into projection support, not reinvention
of the correctness argument. A Java backend, TypeScript backend, Rust
backend, SQL backend, WASM backend, and EVM backend can all consume the
same `ProofPlan`. Their emitted artifacts differ. Their obligation
closure target does not.

## Section 2. Non-goals

This spec rejects the following interpretations.

1. **ProofIR as a universal implementation language.** ProofIR does not
   represent every semantic detail of every host language. It represents
   boundary obligations.

2. **Compiler output as evidence.** A generated artifact is not proof.
   Acceptance requires witness, re-lift, or policy-accepted evidence.

3. **Dropping as compilation.** Inserting a satisfying artifact into a
   repository is a separate host-integration problem. It belongs to a
   dropper, editor, agent, or human workflow.

4. **LLM generation as authority.** An LLM may propose candidates. It
   does not certify candidates.

5. **Core verifier execution of extension code.** Core verification MUST
   NOT execute realization bytecode or extension metadata. Extension-aware
   tooling may execute it under policy; core verification validates CIDs,
   signatures, references, and normative substrate structure.

## Section 3. Relationship to ORP

The Obligation Realizer Protocol defines the shared shape for host-space
realizers:

```
Witnesser: show me it holds.
Dropper:   make it hold, then show me.
Monitor:   keep showing me while it runs.
```

The realization compiler sits inside ORP as a producer of candidates.

```
ORP Realizer
  uses realization compiler
  emits WitnessResult, TransformResult, or RefusalResult
```

A witnesser may use the compiler to produce checker bytecode or witness
values. A dropper may use the compiler to produce a patchless satisfying
artifact before it decides where to put that artifact. A monitor may use
the compiler to produce a runtime checker attachment.

The compiler itself does not decide whether a host repository should be
mutated. It produces artifacts and validation records. Mutation authority
belongs to ORP `transform` mode and remains scoped by ORP policy.

### Section 3.1 Corrected direction

The forbidden direction remains:

```
ProofIR -> complete host application
```

The permitted direction is:

```
ProofIR obligation + target constraints -> satisfying artifact candidate
```

The distinction is not rhetorical. A complete host application must
choose algorithms, state layout, business process, user interaction,
performance tradeoffs, logging, error recovery, deployment behavior, and
implementation texture outside the boundary obligation. A satisfying
artifact only needs to discharge the boundary obligation named by the
problem.

## Section 4. Core abstractions

### Section 4.1 RealizationProblem

A `RealizationProblem` is the input to the compiler.

```json
{
  "kind": "RealizationProblem",
  "schemaVersion": "1",
  "obligation": {
    "kind": "edge",
    "sourcePredicateCid": "blake3-512:...",
    "targetPredicateCid": "blake3-512:..."
  },
  "host": {
    "kit": "typescript",
    "contextKind": "source-candidate",
    "artifactCids": ["blake3-512:..."],
    "entrypoint": "POST /orders"
  },
  "target": {
    "surface": "typescript-source-fragment",
    "language": "typescript",
    "runtime": "node",
    "format": "module-function-body"
  },
  "bindings": [
    {
      "proofVar": "input",
      "hostPath": "body.userId",
      "typeHint": "string"
    }
  ],
  "constraints": [
    {
      "kind": "typecheck",
      "tool": "tsc",
      "policy": "no-new-dependencies"
    },
    {
      "kind": "project-style",
      "policy": "prepared-statements-only-for-sql"
    }
  ],
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"RealizationProblem"`. |
| `schemaVersion` | MUST be `"1"` for this draft shape. |
| `obligation` | Predicate, edge, gap, or body-claim to discharge. |
| `host` | Existing host context the candidate must bind to. |
| `target` | The artifact surface being synthesized. |
| `bindings` | ProofIR variable to host-space binding map. |
| `constraints` | Target and policy constraints beyond the ProofIR formula. |
| `policyCid` | Policy governing admissibility. |
| `inputCids` | Prior artifacts the problem composes against. |

### Section 4.2 TargetSurface

The `target.surface` field names the artifact class the compiler may
emit. Initial surface families:

| Surface | Meaning |
|---|---|
| `host-source-fragment` | A source fragment such as a guard, function body, validator, annotation block, wrapper, or migration statement. |
| `host-source-file` | A whole source file that still requires host compilation or dropping. |
| `host-patch-candidate` | A patch-shaped candidate. This is still not accepted until re-lift and closure. |
| `checker-bytecode` | WASM, EVM, native object, JVM bytecode, eBPF, or other bytecode whose job is to check or witness a boundary. |
| `witness-values` | Concrete values satisfying or refuting a predicate. |
| `schema-artifact` | JSON Schema, OpenAPI fragment, Zod schema, Pydantic model, Bean Validation annotation set, or related schema surface. |
| `proof-envelope-body` | A `.proof` body or memento body governed by grammar and invariant constraints. |
| `protocol-body` | An extension protocol body governed by grammar and ProofIR invariants. |

Target surfaces are constraints, not destinations. A candidate for
`host-source-fragment` can be previewed in the LSP without being inserted
into a repository. A candidate for `checker-bytecode` can be witnessed
without becoming application bytecode.

### Section 4.3 ProofPlan

The canonical `ProofPlan` wire shape is defined by
`2026-05-06-obligation-realizer-protocol.md` §4.3. The realization
compiler produces that ORP sub-artifact when it can explain target-
neutrally how the violation condition is eliminated.

```json
{
  "kind": "ProofPlan",
  "schemaVersion": "1",
  "problem": {
    "kind": "orp-realization",
    "planCid": "blake3-512:..."
  },
  "obligation": {
    "kind": "edge",
    "sourcePredicateCid": "blake3-512:...",
    "targetPredicateCid": "blake3-512:...",
    "gapCid": "blake3-512:..."
  },
  "policy": {
    "mode": "proof_preferred",
    "policyCid": "blake3-512:..."
  },
  "violationCondition": {
    "kind": "formula",
    "formulaCid": "blake3-512:..."
  },
  "objective": {
    "kind": "unsat",
    "formulaCid": "blake3-512:..."
  },
  "eliminators": [
    {
      "kind": "strengthen-precondition",
      "predicateCid": "blake3-512:..."
    },
    {
      "kind": "construct-postcondition",
      "predicateCid": "blake3-512:..."
    }
  ],
  "proofWitnessCid": "blake3-512:..."
}
```

For `p -> q`, `violationCondition` names the canonical formula:

```
p and not(q)
```

The `objective` names the proof target, usually unsatisfiability of that
violation condition. The `eliminators` list describes how a realization
may make the violation impossible. Initial eliminator families:

| Eliminator | Meaning |
|---|---|
| `strengthen-precondition` | Reject inputs or states for which `q` would fail. |
| `construct-postcondition` | Transform or construct output so `q` holds. |
| `preserve-invariant` | Maintain an invariant across a transition. |
| `guard-effect` | Prevent an unsafe effect unless required facts hold. |
| `adapt-boundary` | Insert an adapter, wrapper, schema, or validator at a boundary. |
| `attest-runtime` | Produce runtime evidence that the bound value satisfies `q`. |
| `monitor-transition` | Emit a checker that keeps witnessing the obligation over time. |

A proof plan is reusable across target surfaces. A single plan may
project to Java Bean Validation, JML, Cofoja, Spring guards, TypeScript
Zod, Pydantic, OpenAPI, Rust source, SQL constraints, WASM bytecode, EVM
bytecode, or a `.proof` body.

### Section 4.4 CandidateArtifact

A `CandidateArtifact` is a proposed satisfying assignment.

```json
{
  "kind": "CandidateArtifact",
  "schemaVersion": "1",
  "problemCid": "blake3-512:...",
  "artifact": {
    "surface": "typescript-source-fragment",
    "bytesCid": "blake3-512:...",
    "mediaType": "text/typescript",
    "role": "prepared-statement-call"
  },
  "strategy": {
    "kind": "llm-guided-cegis",
    "producerCid": "blake3-512:...",
    "attempt": 3
  },
  "diagnostics": []
}
```

The candidate body may be carried inline in an unsigned local result for
interactive preview, but any signed or content-addressed claim refers to
the candidate bytes by CID.

### Section 4.5 RealizationResult

A realization compiler returns exactly one result:

```
WitnessedCandidate | RejectedCandidate | RefusalResult
```

A `WitnessedCandidate` means the candidate survived validation.

```json
{
  "kind": "RealizationResult",
  "schemaVersion": "1",
  "status": "witnessed-candidate",
  "problemCid": "blake3-512:...",
  "candidateCid": "blake3-512:...",
  "validation": {
    "proofPlanCid": "blake3-512:...",
    "syntaxCid": "blake3-512:...",
    "typecheckCid": "blake3-512:...",
    "postLiftCid": "blake3-512:...",
    "closureWitnessCid": "blake3-512:..."
  },
  "diagnostics": []
}
```

A `RejectedCandidate` records a failed attempt when policy wants durable
negative evidence or counterexample-driven repair.

```json
{
  "kind": "RealizationResult",
  "schemaVersion": "1",
  "status": "rejected-candidate",
  "problemCid": "blake3-512:...",
  "candidateCid": "blake3-512:...",
  "reasonCode": "CLOSURE_FAILED",
  "counterexampleCid": "blake3-512:...",
  "diagnostics": []
}
```

A `RefusalResult` means no candidate was produced.

```json
{
  "kind": "RealizationResult",
  "schemaVersion": "1",
  "status": "refused",
  "problemCid": "blake3-512:...",
  "reasonCode": "UNSUPPORTED_TARGET_SURFACE",
  "message": "this compiler cannot produce evm-checker-bytecode",
  "diagnostics": []
}
```

## Section 5. Compiler pipeline

The realization compiler is a staged search and validation loop.

```
realize(problem):
  obligation = slice(problem.obligation, problem.host, problem.bindings)
  constraints = assemble(problem.target, problem.constraints, problem.policyCid)
  proof_plan = plan_proof(obligation, constraints)
  strategy = select_strategy(proof_plan, constraints)

  for attempt in strategy.budget:
    candidate = propose(strategy, proof_plan, constraints)
    verdict = validate(candidate, proof_plan, constraints)

    if verdict.closes:
      return WitnessedCandidate(candidate, verdict)

    strategy = refine(strategy, verdict)

  return RefusalResult or RejectedCandidate
```

### Section 5.1 Obligation slicing

The compiler MUST first reduce the ProofIR graph to the obligation
relevant to the target boundary. The slice includes:

- source predicate and target predicate for edge obligations;
- required preconditions and postconditions;
- referenced invariants;
- resource-state transitions;
- signer and policy claims;
- binding map entries;
- accepted prior edges that may discharge sub-obligations;
- opacity or unsupported-position records from solver compilers.

The slice is content-addressed. If two realization runs solve the same
obligation slice under the same target constraints, they should name the
same problem CID.

### Section 5.2 Proof planning

After slicing, the compiler SHOULD construct a proof plan when the
obligation admits one.

For implication edges, the default plan is contrapositive:

```
p -> q
```

becomes:

```
prove unsat(p and not(q))
```

or, equivalently:

```
not(q) -> not(p)
```

This proof plan identifies the class of forbidden executions, values, or
states. Target generation then asks how the target surface can eliminate
that class.

Examples:

- strengthen an API schema so invalid inputs cannot satisfy `p`;
- insert a guard so execution does not continue when `q` would fail;
- construct a returned value so the postcondition holds;
- parameterize a SQL query so untrusted input cannot become SQL syntax;
- add a state transition check so an illegal resource edge is impossible;
- emit checker bytecode that attests the boundary at runtime.

If proof planning fails, the compiler MAY still attempt direct candidate
generation, but the result must record that no proof plan was available.
Policy MAY reject candidates that lack proof plans. Under
`proof_preferred`, such results MUST be marked as degraded evidence.
Under `proof_required`, they MUST be refused.

### Section 5.3 Admissibility assembly

The compiler assembles all non-logical constraints into the same search
problem. Examples:

- target language grammar;
- typechecking command and compiler version;
- allowed imports and dependencies;
- target runtime ABI;
- bytecode format;
- project style policy;
- no public API change;
- deterministic serialization;
- grammar conformance;
- re-liftability;
- signature policy;
- resource budget;
- forbidden side effects.

These are first-class constraints. A candidate that proves the logical
predicate but violates an admissibility constraint is rejected.

### Section 5.4 Strategy selection

The compiler MAY use any strategy accepted by policy. Initial strategy
families:

| Strategy | Use |
|---|---|
| Direct template | Known guards, wrappers, annotation sets, schema refinements, prepared statements. |
| Solver-backed synthesis | Numeric, Boolean, finite-domain, algebraic, or decidable fragments. |
| Enumerative search | Small value domains, fixture generation, finite schema examples. |
| CEGIS | Generate candidate, obtain counterexample, refine candidate. |
| Type-directed synthesis | Construct values or source fragments from type and predicate shape. |
| LLM-guided search | Ask a probabilistic model for candidates, then reject mechanically. |
| Compiler backend | Lower to WASM, EVM, JVM, native object, or other checker target. |
| Protocol-body synthesis | Emit a grammar-constrained `.proof` body or extension protocol body. |

Policy MAY forbid a strategy, require multiple strategies, or require
independent witnesses for high-risk targets.

### Section 5.5 Candidate validation cascade

The validation cascade is target-specific but follows this order:

1. **Bytes exist.** The candidate has canonical bytes and a CID.
2. **Shape conforms.** The bytes parse as the declared target surface.
3. **Target checks pass.** The target compiler, grammar checker, or ABI
   checker accepts the artifact.
4. **Bindings resolve.** ProofIR variables map to the intended host
   names, values, ranges, or paths.
5. **Proof plan matches.** If a `ProofPlan` exists, the candidate must
   realize one or more of its eliminators.
6. **Lift or witness succeeds.** The candidate is converted back to
   canonical ProofIR, or a policy-accepted witness is produced.
7. **Closure holds.** The required predicate or edge is discharged.
8. **Policy accepts.** Signers, CIDs, provenance, budget, and strategy
   requirements satisfy policy.

No earlier stage implies a later stage. In particular, target typecheck
does not imply ProofIR closure.

## Section 6. Dropping as a second constraint problem

The realization compiler may produce a satisfying artifact without
knowing where it belongs in a repository.

Dropping is the separate problem:

```
find edit E such that:
  applies(E, repo)
  places or derives candidate A,
  preserves declared project constraints,
  and re_lift(repo + E) proves obligation O.
```

This is a second constraint system. It may be solved by a language
dropper, an LSP code action, an agent, a human, or a project-specific
script.

The separation is load-bearing:

```
realization:
  find A satisfying O

dropping:
  find E placing A into host artifact H
```

When the candidate is enough, no drop is required. This is common in
LSP, review, and agent workflows: showing a witnessed candidate may be
more useful than editing a file automatically.

## Section 7. LSP shape

The LSP is an interactive surface for realization. It is not the source
of semantic truth.

Current ProvekIt LSP code already follows this direction: the Rust LSP
server exposes diagnostics, hover, code lenses, and code actions, and it
delegates verification to a backend or daemon. Per-language plugins lift
host artifacts. The next shape is to let code actions request
realization candidates.

Expected LSP loop:

```
editor document
  -> lifter/plugin
  -> ProofIR obligation or gap
  -> diagnostic
  -> code action: generate satisfying candidate
  -> realization compiler
  -> witnessed candidate preview
  -> optional dropper-produced WorkspaceEdit
  -> didChange
  -> re-lift
  -> closure witness or remaining diagnostic
```

The LSP owns:

- source ranges and diagnostic display;
- hover and code-lens presentation;
- code-action orchestration;
- previewing candidate artifacts;
- applying `WorkspaceEdit` values when a dropper returns them;
- surfacing receipts and witnesses.

The LSP does not own:

- ProofIR semantics;
- solver soundness;
- content addressing;
- signature acceptance;
- realization correctness;
- dropper authority.

### Section 7.1 Patchless code actions

The first useful LSP action does not need to edit a file.

```
Generate satisfying candidate
```

This action returns a witnessed candidate artifact. If a dropper is
available, the LSP may also offer:

```
Apply candidate with <language> dropper
```

If no dropper is available, the candidate remains useful. It tells the
engineer what source, schema, bytecode, or body would satisfy the
constraint, and it carries evidence for that claim.

## Section 8. Examples

### Section 8.1 SQL safety

Obligation:

```
untrusted(input) -> safe_for_sql(query)
```

Target:

```
typescript-source-fragment
```

Rejected candidate:

```ts
const q = `select * from orders where user_id = '${escapeSql(input)}'`;
await db.query(q);
```

Accepted candidate:

```ts
await db.query("select * from orders where user_id = ?", [input]);
```

The accepted candidate is not accepted because it looks better. It is
accepted because the lifter can read the parameterized query shape and
the verifier can close:

```
untrusted(input) -> parameterized_query(query) -> safe_for_sql(query)
```

under the named policy.

In proof-first form:

```
p:      input is untrusted and reaches query construction
q:      query is safe for SQL execution
not(q): query is not safe for SQL execution

violation condition:
  untrusted(input) and reaches_query(input, query) and not(safe_for_sql(query))
```

The proof plan eliminates the violation by ensuring the untrusted value
is bound as a parameter rather than parsed as SQL syntax. TypeScript,
Java, Rust, Python, or Go may all realize that eliminator differently.
The proof target is the same.

### Section 8.2 Java boundary annotation

Obligation:

```
0 <= amount and amount <= 10000
```

Target surfaces may include:

```
java-bean-validation-annotation-set
java-spring-controller-guard
java-jml-contract
java-cofoja-contract
```

Candidate artifacts:

```java
@Min(0)
@Max(10000)
int amount
```

or:

```java
//@ requires 0 <= amount && amount <= 10000;
```

or:

```java
@Requires("0 <= amount && amount <= 10000")
```

These are different host surfaces. The lifted boundary predicate is the
same. A dropper may later decide which surface belongs in a given Java
project. The realization compiler can produce and witness the candidate
surface before that decision is made.

### Section 8.3 Zod and OpenAPI

Obligation:

```
email(user.email)
```

Target surfaces:

```
zod-schema-fragment
openapi-schema-fragment
```

Candidate artifacts:

```ts
z.object({ email: z.string().email() })
```

or:

```json
{
  "type": "object",
  "required": ["email"],
  "properties": {
    "email": { "type": "string", "format": "email" }
  }
}
```

The candidate surfaces are not source-equivalent. They are boundary-
equivalent when lifted to the same predicate or to predicates connected
by accepted implication edges.

### Section 8.4 WASM or EVM checker

Obligation:

```
balance_after >= 0
```

Target:

```
checker-bytecode
```

The compiler may emit WASM or EVM bytecode that checks the predicate
over a declared binding ABI. The bytecode is checker bytecode, not
application bytecode.

Acceptance requires:

- bytecode CID;
- compiler CID;
- target runtime and ABI;
- binding map;
- checker policy;
- witness that the checker implements the predicate;
- evidence from an execution or attestation that the checker returned
  the required result for the bound value.

Core verification does not execute this bytecode. ORP-aware extension
tooling may execute it under policy and publish the resulting witness.

### Section 8.5 `.proof` body synthesis

Obligation:

```
this body conforms to the .proof grammar and invariants
```

Target:

```
proof-envelope-body
```

The compiler may synthesize a deterministic CBOR body, catalog member,
fixture, or conformance witness body. The resulting bytes are identified
by CID. A separate outer witness can attest that those bytes conform to
the `.proof` format.

This preserves the self-reference rule: a `.proof` file cannot honestly
contain its own final file CID inside itself. The inner artifact is
content-addressed. The outer witness names the inner CID.

## Section 9. Content-addressed run graph

Every stage of realization is a DAG node:

```
RealizationProblem CID
  -> ProofPlan CID
  -> CandidateArtifact CID
  -> SyntaxCheck CID
  -> Typecheck CID
  -> PostLift CID
  -> ClosureWitness CID
  -> RealizationResult CID
```

For dropped repairs:

```
WitnessedCandidate CID
  -> DropPlan CID
  -> Patch CID
  -> TransformedArtifact CID
  -> PostLift CID
  -> ClosureWitness CID
  -> FixReceipt CID
```

This is the "DAGs of DAGs" property. Each step is an edge. Each edge can
be signed, pinned, refused, superseded, compared, or witnessed.

## Section 10. Trust rules

1. **The candidate is not evidence.** Evidence is the witness, re-lift,
   closure proof, or policy-accepted attestation.

2. **The compiler is not the verifier.** It proposes candidates and
   records strategy provenance. It does not certify its own output.

3. **LLMs are search heuristics.** They may generate candidates, repair
   candidates after counterexamples, or choose templates. Mechanical
   acceptance remains outside the model.

4. **Proofless realization is degraded evidence.** Policy may allow it,
   but the result must say that no target-neutral proof plan was
   available. Proofless candidates are not equal in evidentiary strength
   to proof-first candidates.

5. **Target compilers are constraints.** `javac`, `tsc`, `rustc`, WASM
   validators, EVM assemblers, grammar checkers, and schema validators
   can reject candidates. Passing them does not imply ProofIR closure.

6. **Droppers are post-realization adapters.** A dropper may consume a
   witnessed candidate, but its patch is accepted only after re-lift and
   closure.

7. **Core verification does not execute extension bytecode.** Execution
   belongs to extension-aware tools under explicit policy.

8. **Refusals are first-class.** Unsupported predicates, surfaces,
   policies, bindings, or budgets must fail closed.

9. **Policy names the acceptable evidence.** A candidate may be valid
   under one policy and refused under another.

## Section 11. CLI and API shape

This spec does not require a CLI, but the Rust CLI is the natural
operator surface because it is already the all-language CLI.

Possible commands:

```
provekit realize <gap-or-obligation-cid> --target <surface> --policy <cid>
provekit realize inspect <realization-result-cid>
provekit realize witness <candidate-cid> --problem <cid>
provekit realize drop <witnessed-candidate-cid> --kit <language>
```

Library and RPC consumers should use the same data shapes. Other kits do
not need their own CLIs. They expose lifters, witnesses, droppers, and
realization backends. The Rust CLI calls them when needed.

## Section 12. Acceptance criteria for v0

A v0 implementation is conformant to this design if it can:

1. Build a `RealizationProblem` for a known ProofIR gap.
2. Build a `ProofPlan` for at least one implication edge by naming the
   violation condition `p and not(q)`.
3. Produce at least one `CandidateArtifact` for a declared target
   surface.
4. Reject candidates that fail syntax, typecheck, binding, proof-plan
   matching, lift, or closure.
5. Promote only candidates with closure witnesses to
   `WitnessedCandidate`.
6. Keep dropping optional and separate.
7. Surface witnessed candidates through CLI or LSP without applying them.
8. Hand a witnessed candidate to an ORP dropper when a dropper is
   available.
9. Emit content-addressed run artifacts with enough provenance to audit
   the search.

The first milestone should be patchless:

```
gap -> witnessed candidate
```

The second milestone should compose:

```
witnessed candidate -> optional drop -> re-lift -> fix receipt
```

## Section 13. Summary

The realization compiler gives ProvekIt a disciplined way to generate
code without trusting generated code.

It compiles:

```
obligation + constraints
```

into:

```
witnessable satisfying artifacts
```

It does not compile ProofIR into arbitrary applications. It solves for
artifacts whose boundary behavior discharges named obligations. That is
why it composes with constraint-driven development, lossy boundary
compression, ORP, droppers, LSP, checker bytecode, `.proof` self-
conformance, and proof-carrying change.

The patch is still conditional infrastructure.

The constraint is still the product.
