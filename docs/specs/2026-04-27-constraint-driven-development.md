# Constraint-Driven Development

**Date:** 2026-04-27
**Status:** Positioning + methodology doc, paired with the standing-invariant-runtime spec
**Author:** Captured from a session-long architectural dialogue with TSavo

## What this is

A methodology and a product positioning. The methodology, named honestly:
**development whose trajectory is driven by the codebase's accumulating
impossibilities — the things it cannot become.** Each fix mints a permanent
constraint; the cumulative constraint set monotonically reduces the
codebase's degrees of freedom; the value is in the reduction. ProveKit's
fix loop is the mechanism that converts each problem statement into a
constraint, and the standing runtime is the substrate that enforces every
accumulated constraint on every commit, mechanically, at git-commit speed.

This is constraint-driven development, not change-driven. The earlier
naming had the subject inverted: development is not driven by the inflow
of change requests, it is driven by the *outflow* of constraints those
change requests produce. The change request is the trigger; the constraint
is the product; the constraint corpus is what shapes what the codebase can
become next.

This sits in the X-driven-development family alongside TDD, BDD, ATDD. In
all of them, X is the artifact you produce that constrains future code.
Tests, behaviors, acceptance criteria — all artifacts that bound what
comes next. CDD is the same pattern with a stronger artifact: a formal
invariant that constrains every path through the codebase to a protected
sink, including paths that don't exist yet.

## The thesis

> Every problem the codebase encounters becomes a permanent constraint on
> what it cannot do. The constraint corpus locks down the possibility
> space monotonically. The codebase's degrees of freedom decrease with
> every commit. *That decrease is the value.*

Every other software-quality system is additive: tests add behavior
assertions, types add shape definitions, documentation adds prose
descriptions. CDD is *subtractive*: each constraint removes a region of
possible-codebase-states from the admissible set. After 100 constraints,
there are 100 things this codebase cannot do, regardless of who or what
generates the next commit. The codebase isn't free to become anything; it
can only become things that satisfy every accumulated impossibility.

This is the same shape as every system of value:

- **Types** derive their power from what they reject, not what they admit.
  `string | number` is valuable because it forbids objects, functions,
  undefined.
- **Hoare logic** preconditions and postconditions are forbiddances, not
  permissions.
- **Physics** laws are statements of impossibility — you can't create
  energy, you can't decrease entropy in a closed system, you can't exceed
  c. The "rules" of any well-defined system are the constraints, not the
  permissions.

Constraint-driven development applies the same construction to a
codebase's correctness corpus.

## Software ages backwards

In a CDD-substrate codebase, every commit adds a permanent constraint to
the substrate. Older code accumulates *more* protection over time, not
less, because the invariant corpus around it grows. Tech debt has a
defined direction now, and it points down. Refactoring gets *safer* as the
codebase ages, because every refactor is mechanically verified against
every constraint that was ever shipped. Code from year one is more
reliable in year three than in year one — three years of problems
encountered have wrapped it in three years of constraints. There is no
decay.

This inverts every traditional property of software:

- **Velocity becomes correctness.** The traditional tradeoff between
  shipping speed and code quality disappears. Every commit isn't just
  behavior — it's behavior + constraint. More velocity = more constraints
  shipped = stricter possibility-space lockdown. The team that ships 100
  fixes a year ends up with a more-locked-down substrate than the team
  that ships 10. Vibe-coded projects, which produce code at AI-speed, end
  up with the strongest constraint corpora because they're producing the
  most fixes. The pace that would normally destroy a traditional codebase
  IS the mechanism that fortifies a CDD-substrate codebase.

- **Tech debt inverts.** It points down, not up. Time wraps the codebase
  in more protection, not less. The cost of touching old code drops as
  the constraint corpus grows around it.

- **Code review changes shape.** Today: "is this change correct?" — a
  review of behavior, requiring the reviewer to chase dataflow through
  their head. With CDD: "do you agree the problem statement describes a
  real issue and the constraint is the right specification?" The reviewer
  reviews the spec; the runtime checks the dataflow. Review becomes 10x
  faster and 10x more rigorous simultaneously, because the human and the
  machine are doing the work each is good at.

- **The codebase becomes its own senior engineer.** It never leaves; it
  never forgets; it never gets distracted; it never lets a regression
  slide because of velocity pressure. Every constraint it has ever
  learned, it still enforces. The substrate is the institutional memory.

## What other tools cover, and the gap they leave

| Tool | What it constrains | Limit |
|------|--------------------|-------|
| Tests | Behavior at one call site, given specific inputs | Cannot constrain code that doesn't exist yet; doesn't generalize from one example |
| TypeScript / type systems | Variable shapes | Cannot express value-level constraints without dependent types |
| Biome / ESLint / linters | Pattern shapes that often indicate bugs | False-positives on legitimate code; heuristic, not provable |
| CI / staging | Behavioral integration | Runs after commit lands; too late for hook-time rejection |
| Formal verification | Hand-written specs of full system | Doesn't compose with bug-driven workflow; specs lag reality |

The gap nothing fills: **universal-over-paths constraints enforced at
git-commit speed across every call site that exists today AND every call
site that ever gets added later, derived mechanically from the actual
problems a team has actually encountered.**

That gap is the entire ProveKit product. The fix loop + shadow AST + Z3
path enumeration + git-hook construction is the only construction that
fills it, and it does so as a side effect of the action a developer was
already going to take ("fix this bug" / "add this feature" / "make this
change"). The constraint accumulates as a consequence of normal
problem-solving; no separate ceremony required.

## The mechanism, end-to-end

```
user files a problem statement (bug, feature, refactor, perf, security)
  │
  ▼
fix loop ingests the problem statement as a symptom
  │
  ├── Investigate stage maps symptom → candidate code sites
  ├── Locate stage refines → callsite via SAST
  ├── Classify stage routes the change kind
  ├── B3 stage matches against axiomatic principles (fast path)
  ├── C1 derives Z3-grounded constraint (the impossibility statement)
  ├── C1.5 cross-LLM-validates the constraint against the problem
  ├── C2 opens a throwaway git overlay
  ├── C3 generates the patch in the overlay (one way to satisfy the constraint)
  ├── C5 generates a mutation-verified regression test
  ├── C6 attempts (one shot, no retries) to graduate the constraint to
  │   a cross-codebase principle; usually fails (per-codebase) and the
  │   bundle ships without a principle
  ├── D1 assembles bundle: patch + constraint + regression test + audit
  └── D2 emits patch.diff + pr-body.md + writes constraint to
      .provekit/invariants/<sha>.json
  │
  ▼
human reviews the change, accepts the patch, commits
  │
  ▼
.provekit/invariants/<sha>.json is now source-controlled
  │
  ▼
provekit verify pre-commit hook runs on every subsequent commit
  │
  ├── re-resolve every constraint's bindings against current AST
  ├── enumerate paths to each protected sink via shadow AST
  ├── Z3-check each path against the universal constraint
  ├── decay → yellow alarm (binding can't resolve)
  ├── violation → red, commit rejected
  └── all hold → silent pass, hook exits 0
  │
  ▼
codebase's possibility space is incrementally locked down. Every fix
becomes a permanent constraint. Every new commit gets verified against
every standing constraint, including constraints from problems nobody on
the current team remembers encountering.
```

The patch is an artifact. The constraint is the product. The patch is one
way to satisfy the constraint at the moment of fixing; future code must
satisfy the constraint through any path that ever reaches the protected
sink, whether that path exists today or gets added by an AI agent six
months from now.

## The user journey

The entry point that makes this product distribute itself:

1. User asks a vibe-coding agent to build a product. Agent ships a
   working-on-the-happy-path codebase with high interconnection and
   plenty of edge cases the agent didn't anticipate.
2. User uses the product. Surface works. Edge case breaks.
3. User goes back to the LLM and types "fix this bug."
4. **This is the install moment.** The fix loop runs inside the
   conversation the user was already having. The bug gets fixed AND the
   first constraint gets minted AND the git hook gets installed AND the
   `.provekit/` substrate gets bootstrapped. Zero friction. Zero new
   tooling decisions. Zero adoption ceremony.
5. From that moment forward, every subsequent problem statement the user
   files ratchets the floor up. Each new "fix this bug" / "add this
   feature" / "make this change" produces another permanent constraint.
   The vibe-coding agent that introduced the original bug now has to
   satisfy a growing corpus of constraints from every previous incident
   the user has reported.

The user never made a deliberate decision to adopt ProveKit. The product
got installed transparently inside the action the user was already
taking. Adoption is identical to the use case.

## The constraint flywheel

```
user files problem statement
  ↓
fix loop produces patch + constraint + regression test
  ↓
constraint added to .provekit/invariants/
  ↓
git hook now enforces this constraint on every commit
  ↓
[time passes, AI generates new code]
  ↓
new code creates new dataflow paths
  ↓
git hook enumerates new paths, Z3-checks them against every constraint
  ↓
new code that violates ANY past constraint is rejected at commit time
  ↓
user files new problem statement (about a different issue)
  ↓
[loop closes; flywheel turns; constraint corpus grows by one]
```

Each turn of the flywheel:
- Adds one constraint
- Permanently shrinks the future failure surface
- Permanently reduces the possibility space the codebase can occupy
- Costs the user one problem statement
- Returns: one fix + permanent protection against the constraint's
  violation across every existing AND future call site

The compounding property: **the AI's freedom to introduce regressions
monotonically decreases as a function of the codebase's problem history.**
The codebase becomes the substrate that *teaches the AI* which mistakes it
has already learned not to make in this context.

## The recursive depth

The vibe-coding AI is BOTH the source of the bugs AND the agent that
fixes them. The bugs that exist are exactly the failure modes the AI
couldn't prevent on its own. The constraints minted from those bugs are
external impossibilities the AI couldn't have derived for itself.

Each fix loop cycle teaches the codebase a constraint the AI didn't know
it needed. The substrate ends up encoding the AI's specific failure
topology — every mistake the AI has ever made in this codebase becomes a
permanent constraint the AI must satisfy on every subsequent generation.

The AI doesn't get smarter. The codebase gets smarter ABOUT the AI.

## The git history becomes a queryable knowledge graph

Every commit pairs a problem statement (the trigger) with a formal
constraint (the impossibility statement) and an executable witness (the
regression test). `git log` stops being a record of changes and becomes a
corpus of "problems this codebase has encountered and the impossibilities
it has codified in response." Six months later, when someone wonders "why
does this work this way?", the answer is in the constraint store, not in
tribal memory. Lessons can't fade because they're encoded in the
substrate as machine-checkable contracts.

## The codebase teaches new contributors mechanically

A new human or AI joining the project doesn't need to read documentation
about "how this codebase works." They run `provekit verify --list` and
see every constraint the codebase pledges to satisfy. The constraints are
executable specifications. New contributors learn the codebase by reading
the constraints — by understanding what's forbidden — not by reading
prose. The substrate teaches by negative space.

## Categorical reduction

Two surfaces, no heuristics:

- **Principles**: cross-codebase axioms. Rare. Hard-won. Pass adversarial
  validation against a universal corpus. Used by B3/recognize for fast-
  path bug recognition. Examples after re-validation: probably division-
  by-zero, integer-overflow-with-bounded-input. Maybe 10-50 entries total
  in a mature library, not hundreds.

- **Constraints (per-codebase invariants)**: hash-bound to specific AST
  nodes via the substrate. The default output of every fix loop run.
  Enforced by `provekit verify` git hook on every commit. Universal over
  paths to the bound sink in *this* codebase.

Heuristics are biome's product, not ProveKit's. Empty-catch, falsy-
default, exhaustive-deps, unused-vars — that whole surface is owned by
linters. ProveKit competing there has zero value differential. The
interesting space is exactly the two extremes: locally provable
(per-codebase constraints, the bulk of output) and universally provable
(principles, the rare graduation).

The validator IS the definition. A candidate that fails adversarial
validation isn't a flawed principle that better prompting can fix — it's
not a principle at all, and the validator is correctly telling us so.
Most problems don't yield a generalizable shape, the validator detects
that, and the right response is "ship the per-codebase constraint only"
not "retry with refinement." C6 should fail-fast (one shot, no retries);
19-minute principle-refinement loops were the architecture refusing to
accept "no principle" as an answer.

## CDD positioned against TDD

Both methodologies share the same core insight: the artifact you produce
constrains the code that comes next. They differ in the *reach* of the
constraint:

- **TDD constrains via tests.** Tests are point-coverage assertions at
  known call sites. They cover code you wrote tests for; they say nothing
  about new call sites added later, nor about variations of the input
  space the test author didn't anticipate.

- **CDD constrains via universally-quantified invariants.** Constraints
  are universal-over-paths properties at protected sinks. They cover
  every call site that exists AND every call site that ever gets added,
  derived mechanically from a single example.

CDD doesn't replace TDD. They compose:
- TDD covers the call sites you tested, with high specificity about input
  shapes and expected outputs
- CDD covers all paths to protected sinks, with universal coverage but
  bounded to SMT-expressible properties

A mature codebase running both gets behavior coverage at known points
plus universal coverage at protected sinks. Anyone who buys TDD already
buys the methodological premise; CDD just extends the reach to the cases
TDD cannot cover.

## The pitch, in one line

> *Every problem your codebase encounters becomes a permanent constraint
> on what it cannot do. The git hook does the static analysis across
> every existing call site and every new call site that ever gets added,
> mechanically, no LLM in the verification path. Code as fast as you
> want; the constraints accumulate at git-commit speed; possibility space
> locks down monotonically; software ages backwards.*

Every word is mechanically true once the standing-invariant-runtime spec
is implemented. The dogfood proof shipped 2026-04-27 against the planted
asc/desc bug demonstrates the existence claim for the fix-loop half. The
spec at `docs/specs/2026-04-27-standing-invariant-runtime.md` is the
dependency-ordered build plan for the enforcement half.

## What this paper is for

This is the institutional record of the architectural realization. It
exists so that:

1. The next person to read this code (human or AI) understands what the
   product is *for*, not just what the components are.
2. The fix loop's null-principle outcomes are correctly understood as
   working-as-designed, not as failure modes worth retrying around.
3. The categorical reduction (principles vs constraints, no heuristics)
   is the source of truth for any future architecture decisions.
4. The marketing line is grounded in the architecture and traceable to
   the substrate that enforces it.
5. Future contributors can answer the question "why is provekit different
   from biome / TypeScript / unit tests?" with a coherent technical
   answer, not a marketing slogan.
6. The naming is honest: development is driven by what the codebase
   cannot become, not by what's coming in. The constraint corpus is the
   driver. Constraint-driven development.

## Open questions deferred to v2+

- **Symbolic node identity across renames/extractions/moves.** Current
  binding mechanism is content-addressable (sha256 of node content). A
  rename decays the binding and surfaces as an alarm asking the human to
  re-run the fix loop on the renamed locus. v2 can track identity across
  cosmetic edits without LLM involvement; v1 ships without it.
- **Cross-codebase constraint porting.** When and how do per-codebase
  constraints graduate to cross-codebase principles, beyond the C6
  adversarial validator? Is there a "common shapes across many
  constraints" emergent surface?
- **Distributed constraint stores.** Imports of one codebase's
  constraints into another (e.g., a shared library's contracts becoming
  the parent project's contracts).
- **Time-decay metadata.** Constraints tagged with their motivating
  problem class so a violation report can name "this is the same class
  as X from six months ago."
- **Witness-replay.** When a violation is found, mechanically synthesize
  a failing test case from the Z3 witness so the developer sees a
  concrete failing input, not just a "Z3 says SAT" verdict.

These are v2+. v1 ships without them and remains the load-bearing claim.

## Bottom line

ProveKit's product is not "AI fixes your bugs faster." That's the
productivity story everyone else is selling, and it's bounded by
human-equivalent quality.

ProveKit's product is: **every problem your codebase encounters becomes a
permanent constraint on what every line of code anyone (human or AI) ever
writes from then on, mechanically enforced at git-commit speed.** The
codebase's degrees of freedom decrease monotonically. Software ages
backwards. Velocity becomes correctness. The codebase ratchets toward
provability, with no decay over time.

The naming is honest because the methodology is honest: development is
driven not by changes coming in but by impossibilities accumulating. The
constraint corpus is the substrate; the substrate is the product; the
product is the codebase's monotonically-shrinking possibility space.

That's the moat. That's the bow.
