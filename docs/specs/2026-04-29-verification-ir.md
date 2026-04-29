# ProvekIt: the host language is the IR

> Author: shared session 2026-04-29 (T + Claude). Canonical architectural
> identity document.

## What ProvekIt does

> **ProvekIt proves your code was never correct. Then makes it correct.
> Forever.**

The first run against any real codebase surfaces every property the code
fails to satisfy — with concrete counterexamples, with the violated
property named, with the producer that demonstrated the failure. Not
suggestions. Proofs of failure.

The framework then does the work. A do-the-work producer takes the
violated memento and the property and produces a patch + a regression
test, both verified, both signed, both content-addressed. The developer
approves; the framework lands it. The bug stops being a bug.

The proof is durable. Once a memento says `verdict: holds` for a
property, that proof outlives every prover that contributed to it. Z3
retires? The leaf mementos signed by Z3 stay valid (their content hashes
are durable). Replace with whatever comes next — the DAG grows new leaves
without invalidating old ones. Your code from 2026 is still verified in
2076 by walking its DAG, even after every prover from 2026 has been
replaced.

That is what the framework does. Everything else in this document is the
architecture that makes those three sentences operationally true.

## The thesis in one sentence (technical)

ProvekIt is the host language's type system, plus a library for properties
beyond types, plus a swarm of producers that verify both, plus a content-
addressed memento DAG that composes verdicts into proofs.

The host language is always the IR. The framework is invariant under
host-language choice. The architectural primitive — content-addressed
hash-and-trust with producer fungibility — is the same one that has
ridden through Xdrive (1995) → Digital Confetti (1998) → BitTorrent
(2001) → Bitcoin (2008) → IPFS → Git → and now ProvekIt for proofs.

## Adoption: point it at the repo

The framework reads what is already there. No developer behavior change.
No annotation discipline required.

```
Point ProvekIt at your repo.
It reads your history.
It mints mementos for every property your code has ever satisfied.
```

Every commit is implicitly an intent statement. The before-state, the
after-state, and the commit message (or linked ticket, or incident
report) together fully express what the developer was trying to
accomplish. An LLM-producer reads that triple and extracts the IR
formula directly. Cross-validation by other producers; a Z3-producer
runs against the post-fix state to mint the verified memento.

```
diff:        +    if denominator == 0 { return Err("denominator must not be zero") }
commit msg:  "fix divide-by-zero crash reported in INC-2847"
incident:    INC-2847 ("calculate() crashes when called with b=0 from /api/quotient")

→ Producer extracts:
    property denominator_nonzero {
      scope: function calculate
      formula: forall(call: CalculateInvocation) => call.b != 0 OR call.errors_with("denominator")
    }
→ Memento: verdict=holds, signed by z3-symbolic@4.13 (post-fix), input_cids=[diff-cid, ticket-cid]
```

No human wrote that property. No `@intent` annotation was authored. The
diff existed; the commit message existed; the ticket existed. The
framework read all three and produced the durable proof.

Annotations remain available but optional. A developer CAN add `@intent`
to skip LLM inference for a particular case, but it is no longer
required for the framework to operate.

## The market: mainframe enterprises first

The conventional adoption playbook (start with modern dev shops, expand
outward) is wrong for ProvekIt. The right shape is the inverse: hardest,
oldest, most mission-critical first.

Mainframe COBOL shops at major banks, insurance carriers, healthcare
processors, and government agencies are the FIRST market because they
have:

- **Richest mineable corpora.** 50+ years of commit history (or change-
  ticket archives where version control wasn't introduced until later),
  decades of incident reports, regulatory filing trails, runbooks, and
  test suites that have run continuously through every economic cycle
  since the 1980s.
- **Highest per-bug cost.** A mainframe defect can be a $1M-$100M
  regulatory fine, an outage that affects millions of customers, or a
  breach that triggers SOX / FDIC / OCC / FFIEC consequences. Modern
  shops measure bugs in hundreds of dollars; mainframe shops measure
  them in millions.
- **Largest existing budget.** Major banks spend $50M-$500M annually on
  mainframe maintenance + modernization. They are already running 3-10
  enterprise LLM pilots each. The buying motion is in place; ProvekIt
  fits an existing line item.
- **Acute staff-replacement pain.** COBOL developers are aging out. There
  is no pipeline. Every retirement is an institutional knowledge loss
  event. The mine-history workflow directly addresses this — the
  retiring developer's knowledge becomes content-addressed mementos
  before they walk out the door. That is a problem CTOs lose sleep
  over and will pay seven figures to solve.
- **Differentiation against migration.** The current alternative is
  COBOL→Java migration, which mostly fails. ProvekIt does not require
  migration. Keep the COBOL; the framework verifies it in place. That
  is a strictly better narrative than rip-and-replace.

The TAM math:

- Mainframe market: ~$50B/year in software + services, with tens of
  trillions in business value running on it.
- Conservative capture: 1% of mainframe maintenance spend = $500M/year ARR.
- Aggressive: ProvekIt becomes the standard verification substrate for
  legacy modernization = $5B+/year ARR.

Modern dev tooling categories (Copilot, Sentry, Snyk) are $1B-ish at
their best. Enterprise legacy verification is 10x to 100x larger. The
first customer is not a Series A startup. The first customer is a
Fortune 500 bank or insurance carrier writing a $1M-$10M pilot check.

The "TS-shop early-adopter" path is the long tail, not the seed market.
TS shops adopt later, freely, after the framework has been hardened on
harder workloads.

## The mandate-able floor

The single biggest lever is what we can MANDATE that costs the developer
nothing. The answer per host language:

| Host language | Mandate-able floor | What that already covers |
|---|---|---|
| TypeScript | `tsc --strict` passes | Null safety, branded types, narrowing, exhaustiveness |
| Rust | `cargo check` + `clippy::pedantic` | Memory safety, ownership, lifetimes, trait coherence |
| Lean4 | Lean elaborator succeeds | Almost everything (dependent types) |
| Haskell | GHC compiles with `-Wall -Werror` | Pure-fn correctness, exhaustiveness, type-class laws |
| Python (typed) | `pyright --strict` passes | Modest structural typing |
| Python (untyped) | Nothing typed; runtime checks only | Almost nothing — producers do everything |
| Perl | `use strict; use warnings;` + `Carp::Assert` | Lexical scoping, declared vars, runtime assertions |
| COBOL | Compiler diagnostics + runtime instrumentation | Type-checked WORKING-STORAGE, ABEND on violations |
| Lisp | SBCL compile + `assert` macros | Static type-inference where types are inferable |

This is the adoption argument. Every TS shop already runs `tsc`. Every
Rust shop already runs `cargo check`. The framework does not introduce
a new requirement — it ENRICHES the requirement they already have.

The floor is uniform: **the host language's checker passes.** The
ceiling varies by host language's expressiveness. Strong type systems
get more for free; weak ones lean harder on the producer pool.

## The host language IS the IR

You don't author IR formulas in some custom syntax. The IR is a library
in whatever host language you are already using.

```typescript
import { property, forAll, NonZero } from "@provekit/ir";

function divide(a: number, b: NonZero<number>): number {
  return a / b;
}

const denominatorNonZero = property<DivisionNode>({
  scope: function_containing,
  formula: forAll(div => div.right !== 0),
});
```

```rust
use provekit_ir::{forall, NonZero, property};

fn divide(a: f64, b: NonZero<f64>) -> f64 { a / b.into() }

provekit::invariant! {
  forall(div: DivisionNode) => div.right() != 0
}
```

```lisp
(defun divide (a b)
  (assert (not (zerop b)) (b) "denominator-nonzero")
  (/ a b))
```

```cobol
IF DENOMINATOR = ZERO
  DISPLAY "VIOLATION: DENOMINATOR-NONZERO"
  PERFORM ABEND-ROUTINE
END-IF.
```

Different host languages, same architectural shape. Type-dialect handled
by the host's compiler; library-dialect handled by external producers
via the IR library / crate / macro / COPYBOOK.

The cleverness is structural:

1. **Zero new syntax.** IR primitives are normal functions / macros /
   conditionals in the host language. Editor support, refactoring, type
   checking, autocomplete — all FREE.
2. **Type system enforces well-formedness.** Bad IR doesn't compile.
3. **Runtime value IS the IR formula.** The expression evaluates to data
   that can be `.toString()`'d, walked programmatically, hashed for
   content addressing, translated to SMT-LIB / Datalog / behavioral
   test cases.
4. **LLMs author it natively.** Their training data is the host
   languages. Even small models (7B parameters) write fluent TypeScript,
   Rust, Lisp, Perl, COBOL. The IR-as-host-library makes every code-
   trained LLM an instant producer.
5. **Cross-language interop is canonicalization.** Different syntax,
   same FOL semantics. Same content hash → same memento slot. Two
   developers in two host languages converge to the same claim.
6. **The meta-IR is types-of-types.** TypeScript's type-level
   programming, Rust's traits + const generics, Lean4's dependent
   types, Lisp's macros — each host's reflective surface IS the meta-IR.
7. **Tooling is free.** LSP, debugger, profiler, package manager,
   distribution — every existing host-language tool just works.

The host language is the substrate. Whatever the developer writes in is
whatever the IR is. The framework canonicalizes to a hash; the swarm
distributes by hash; producers translate from the host AST to their own
backend. The host language is the only authoring surface anyone ever sees.

## Producers: every type-checker, every LLM

The framework does not compete with type systems. It incorporates them.

**Every host-language compiler is a producer.** Every successful `cargo
check` is a memento with `rustc@1.84` as the producer. Every clean
`tsc` run is a memento with `typescript@5.4` as the producer. Every
COBOL compile is a memento with the COBOL compiler as the producer.

**Every LLM is a producer.** The IR-as-host-library means any code-
trained LLM can author IR formulas. The producer pool spans frontier
models for complex reasoning, mid-tier for IR proposals, small models
for fragment-level cross-validation. The framework grades problem size
to producer capability.

**Every formal prover is a producer.** Z3, Soufflé / DDlog, Lean4 / Coq,
CBMC / Klee, QuickCheck / proptest / Hypothesis. Each implements the
producer interface, registers a capability, translates IR formulas to
its input language, emits mementos.

Different producers emit different evidence variants in the witness (Z3
emits a model; Datalog emits match-rows; rustc emits a passing-compile
token; an LLM emits a confidence score + explanation). The framework
reads the wrapper for cross-validation; reads the variant only when
audit/explanation needs it.

## Symmetry: writing and debugging are the same primitive

The producer pool is symmetrical. Same models, two roles:

- **Author role.** LLM reads (intent, code) → proposes IR formula or
  emits code candidate.
- **Debug role.** LLM reads (intent, code, failed memento with
  counterexample, IR formula) → proposes repair.

The framework provides the TOOLS that make even weak LLMs effective
debuggers: the memento DAG as working memory; verdicts as ground truth;
intent (extracted from diffs) as ground truth specification; IR formulas
as explicit specs that name what was checked; witness data as concrete
counterexamples.

Hand a 7B model `(intent, code, failed memento with Z3 counterexample,
IR formula, three lines of context around the failure)` and ask "fix
this" — and it can. Not because it is smart, but because the framework
gave it everything it needs to act on a small, bounded problem.

This is the operational form of *trust AI execution, distrust AI
orientation*. The framework HOLDS the orientation; the LLM does
EXECUTION. Weak LLMs with good scaffolding outperform strong LLMs with
no scaffolding. The framework is pure scaffolding.

## Stages vs Actions

Verification claims and side-effecting operations require different
contracts. Modeling them with the same interface forces cache-busting
hacks; the right cut is at the type level.

```typescript
// Pure, cacheable. Output is a CLAIM. Composes by reference in the DAG.
interface Stage<I, O> {
  name: string;
  producedBy: string;
  serializeInput(input: I): unknown;
  serializeOutput(output: O): string;
  deserializeOutput(witness: string): O;
  run(input: I): Promise<O>;
}

// Impure, run-every-time. Output is a RESOURCE handle. Audit-only memento.
interface Action<I, R> {
  name: string;
  producedBy: string;
  serializeInput(input: I): unknown;
  describeResource(resource: R): string;
  run(input: I): Promise<R>;
}
```

The runner has `runStage` (cache-aware, claim-producing) and `runAction`
(cache-bypassing, resource-producing, audit-write-only). The type system
makes it impossible to compose an Action's resource into a Stage's
binding hash. The proof tree only carries claims, not resources. The
audit DAG carries both, tagged.

## The recursion: claims about claims about IR

The IR's deepest property is that **the memento DAG is the proof tree**.
Properties compose by reference. A complex claim's verdict decomposes
into component verdicts via trusted combinators (∧, ∨, ⇒, ∀, ∃, plus a
small handful more). The combinators are the *trusted kernel*.

This is LCF-style verification (Edinburgh, 1972 → HOL → Coq) operationalized
as a swarm:

| Curry-Howard | ProvekIt |
|---|---|
| Type | IR property (in host language) |
| Term | Memento (claim + witness) |
| Type-checker | Producer + kernel combinator |
| Proof normalization | DAG walk |

### Three meta-levels

Same architectural primitive applied recursively across abstraction layers.

**Level 0 — code claims.** Mementos verify code. Producers run. Verdicts
compose via DAG walk.

**Level 1 — IR formula claims.** Mementos verify IR FORMULAS. Producers
verify the formulas (a) are well-formed, (b) express the inferred intent,
(c) are sound under the IR's semantics. Each LLM-extracted property is
a memento, signed, walkable, cross-validated.

**Level 2 — IR language claims.** Mementos verify THE IR LANGUAGE
ITSELF. The IR's grammar is a content-addressed artifact. Its meta-
properties (soundness of combinators, decidability of fragments) are
themselves claims, verified by producers (Coq/Lean for soundness; the
host's type checker for AST type safety; SMT for decidability).

The recursion bottoms out at the host language's metatheory. Below
that, we trust the host language. That is the only axiom.

## What this replaces

Tests, CI, code review, linters, documentation, type checking — these
are not replaced by something else. They become *aspects* of one
underlying structure: the memento DAG.

| Today's separate concern | In the unified model |
|---|---|
| Type check | Memento from the host's type-checker producer |
| Lint | Memento from a lint producer (clippy, eslint, …) |
| Test | Behavioral memento from a test-runner producer |
| Code review | Memento from a developer-signs-this-LLM-proposal producer |
| CI green light | Composite memento that all required leaves are verdict=holds |
| Documentation | Intent extracted from history + walkable mementos pointing to claims |
| Coverage report | DAG walk: which lines have at least one verdict-holding memento |
| Bug fix | New memento with `verdict: holds` referencing prior `verdict: violated` via inputCids |
| Migration safety check | Composite memento that pre/post property mementos compose correctly |
| Regulatory audit | DAG walk over the proof tree; the audit IS the walk |

All hash-equivalent, all walkable, all signed, all in one substrate.

## Cost economics invert

Today: every meaningful LLM call hits a frontier model. Tomorrow with
ProvekIt: 95% of work runs on cheap models; 5% needs frontier models.
Total spend is a fraction of today's.

Producer diversity > producer power. Five 7B models agreeing is more
trustworthy than one frontier model alone. The framework's economic
moat is "we provide the context that makes the dumbest LLM useful."
That moat is durable. Frontier models commoditize fast; scaffolding
stays load-bearing forever.

## Network effects

Mementos are content-addressed; codebases share proofs. Same property
hash = your codebase pulls another codebase's verification verdict
instead of running its own. The principle library, the standing
invariant set, every leaf claim ever produced — all become public goods
at the proof layer.

A codebase using ProvekIt contributes mementos to the swarm. A codebase
using ProvekIt benefits from others' mementos. Classic n² value scaling
at the verification layer. The first verification public-goods substrate.

## Per-language kits

The architecture is universal; the work clothes are local. Each host
language needs its own complete kit:

- Custom LLM prompts (per task, per idiom).
- Custom AST canonicalizer (host syntax → FOL hash).
- Custom producer pool (rustc/clippy/miri vs tsc/eslint vs SBCL vs … ).
- Custom IR library (`@provekit/ir` vs `provekit_ir` crate vs …).
- Custom diagnostic translator (memento → native diagnostic format).
- Custom IDE integration (LSP server or editor extension).

The factoring:

```
Universal core (write once):
  - memento store
  - workflow runtime
  - producer registry
  - kernel combinators (∧, ∨, ⇒, ∀, ∃, …)
  - claim envelope schema
  - swarm protocol (CID exchange)

Per-host-language (write per language, swarm-distributable):
  - IR library (authoring surface)
  - AST canonicalizer (host syntax → FOL hash)
  - LLM prompt set (per-task, per-idiom)
  - Producer integrations (native compilers + provers + linters)
  - Diagnostic translator (memento → native diagnostic format)
  - IDE integration (LSP server or editor extension)
```

This is exactly LSP's architecture inverted to verification. LSP defined
a universal protocol; each language community owns its language server.
ProvekIt defines a universal proof substrate; each language community
owns its kit.

The kits themselves are swarm artifacts subject to the framework's own
machinery. A kit's correctness is itself a chain of mementos. New
languages onboard by publishing a kit. No core changes required.

## Implementation phasing

**Phase 1 — universal claim envelope.** Standardize the memento witness
schema. Wrapper fixed; evidence is a tagged union. Existing producers
wrap their output in this envelope.

**Phase 2 — Stages vs Actions split.** Refactor the existing producers
to the typed interface. `openOverlay` becomes an `Action`. Cache-busting
hacks deleted.

**Phase 3 — type-dialect IR (TypeScript first).** Define the canonical
form for type-expressible properties. tsserver as a producer.

**Phase 4 — library-dialect IR + the IR npm package.** Ship `@provekit/ir`
with primitives. Canonicalizer to stable AST hash. Translators to SMT-LIB
and Datalog.

**Phase 5 — diff-driven intent extraction.** LLM-producer that reads
(diff, commit-message, linked-tickets) and emits IR formula proposals
as mementos. Cross-validation by other LLM-producers. The mine-history
workflow becomes the framework's primary adoption path.

**Phase 6 — kernel combinators.** Implement the trusted kernel. Each
combinator's soundness is itself a memento (verified by Coq or Lean).

**Phase 7 — meta-IR for new property kinds.** Type-level programming
patterns for adding new property kinds without modifying the framework.

**Phase 8 — language-server integration.** ProvekIt LSP server.
Surfaces mementos as diagnostics, shows verifier identity on hover,
displays the proof DAG inline.

**Phase 9 — first enterprise pilot.** Pilot customer (Fortune 500 bank
or insurance carrier) funds the COBOL kit under contract. Pilot
deploys against a real production codebase. Mementos for every
property the code has ever satisfied; bugs discovered; fixes minted;
audit trail built. Reference customer.

**Phase 10 — Rust kit, then Lean4 kit, then long-tail kits.** Each
host language onboarded by community-funded or pilot-funded kit work.
Cross-language canonicalization makes properties hash-equivalent
across hosts.

Phases 1-4 are weeks each (the technical foundation; mostly built today).
Phase 5 is a focused build. Phases 6-8 are weeks-to-months. Phase 9 is
the strategic inflection — pilot funding pays for the kit work and the
first reference deployment. Phase 10 is the expansion.

## The career arc closing

The architectural primitive — content-addressed hash-and-trust with
producer fungibility and swarm distribution — has ridden through five
domains:

| Year | Domain | Form |
|---|---|---|
| 1995 | Files (dedup) | Xdrive content-addressable storage |
| 1998 | File swarm + crediting | Digital Confetti |
| 2001 | File swarm at scale | BitTorrent |
| 2008 | Value (transactions) | Bitcoin |
| 2014+ | General content | IPFS, Merkle DAG everywhere |
| 2026 | **Proofs** | **ProvekIt** |

The arc ends at proofs because proofs are the most consequential domain
— they are *what makes the other domains safe at scale*. Files, money,
code, all eventually require verification of correctness. The proof
swarm is the substrate that lets every other swarm be trusted.

This is not a coincidence. The same architectural primitive, applied
recursively across content domains, finally arrives at the domain that
verifies all others. ProvekIt is the natural endpoint of the lineage.

## What this is for

A reader who understands this document understands that ProvekIt is:

- A certificate authority for software correctness (memento store).
- A workflow runtime that composes certificate requests (workflows).
- A swarm distributing certificates and workflows (CIDs + the network).
- A property language whose substrate is the host language itself.
- A self-hosting verification framework where the IR is what the
  developer was already going to write.
- A market for proof producers where every type-checker and every LLM
  participates, and the cheapest producer that can do the job gets it.
- A diff-driven intent extraction engine that mines provenance from
  decades of version-control history without any developer effort.

Each piece independently evolvable. Every layer's protocol outlasts
every layer's current implementation. The host language is the
universal substrate; the framework is a library that grows.

The customer-facing thesis is the three sentences at the top of this
document: ProvekIt proves your code was never correct. Then makes it
correct. Forever. Everything in this document is the architecture that
makes those three sentences operationally true. Vibe coding becomes
safe by default. Constraint coding becomes the default mode. Programming
becomes specification. The proof tree is the codebase's durable
identity, surviving every prover that ever contributes a leaf.

That is what makes software age backwards.
