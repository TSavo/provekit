# Change-Driven Development

**Date:** 2026-04-27
**Status:** Positioning + methodology doc, paired with the standing-invariant-runtime spec
**Author:** Captured from a session-long architectural dialogue with TSavo

## What this is

A methodology and a product positioning. The methodology: every change to a
codebase starts as a problem statement (a bug report, a feature request, a
refactor motivation, a performance complaint, a security concern), gets fed
through a fix loop that derives a Z3-grounded invariant, and ships the
invariant as a permanent contract the codebase pledges to satisfy on every
commit forever after. The positioning: this is what ProveKit *is*, and the
fix loop + standing runtime are the substrate that makes the methodology
mechanical instead of aspirational.

This is not bug-driven development. The naming was wrong. Bugs are one
shape of change; new features, refactors, performance work, security
hardening are also changes. They all share the same shape: a problem
statement (what currently exists is wrong in some specific way) followed by
a fix (a code change that addresses the problem). All of them benefit from
the same substrate treatment: derive a formal property the fix establishes,
mint an invariant, enforce it forever. **Change-driven development.**

## The thesis

> Every change to the codebase becomes a permanent constraint the codebase
> pledges to satisfy. Constraints accumulate monotonically. Software ages
> *backwards* — older code becomes more protected, not less, as the
> invariant corpus around it grows. Tech debt has a defined direction now,
> and it points down. Velocity becomes correctness rather than trading
> against it.

Every other software-quality system has correctness *decay* over time:
features pile up, original design assumptions get violated, the test suite
falls behind, the type definitions get loose, the linter config gets
exceptions. Change-driven development with substrate enforcement is the
only construction I'm aware of where correctness *compounds* with every
shipped commit.

## What other tools cover, and the gap they leave

| Tool | Coverage shape | Limit |
|------|----------------|-------|
| Tests | Point coverage: "given input X, expect output Y" at one call site | Cannot cover code that doesn't exist yet; doesn't generalize from one example |
| TypeScript / type systems | Type-shape coverage of variable contents | Cannot express value-level constraints without dependent types |
| Biome / ESLint / linters | Pattern coverage: "this shape often indicates a bug" | False-positives on legitimate code; heuristic, not provable |
| CI / staging | Behavioral coverage at integration time | Runs after commit lands; too late for hook-time rejection |
| Formal verification | Full coverage but hand-written specs | Doesn't compose with bug-driven workflow; specs lag reality |

The gap nothing fills: **universal-over-paths properties enforced at
git-commit speed across every call site that exists today AND every call
site that ever gets added later, derived from the actual problems a team
has actually encountered.**

That gap is the entire ProveKit product. Tests can't fill it (point
coverage). Types can't (no value-level constraints in mainstream
languages). Linters can't (heuristics, not invariants). CI can't (too
late). Formal verification can't (specs don't co-evolve with bugs). Only
the fix loop + shadow AST + Z3 path enumeration + git-hook construction
fills it, and it does so as a side effect of the action a developer was
already going to take ("fix this bug" / "add this feature" / "make this
change").

## The mechanism, end-to-end

```
user files a change request (bug, feature, refactor, perf, security)
  │
  ▼
fix loop ingests the change request as a symptom
  │
  ├── Investigate stage maps symptom → candidate code sites
  ├── Locate stage refines → callsite via SAST
  ├── Classify stage routes the change kind
  ├── B3 stage matches against axiomatic principles (fast path)
  ├── C1 derives Z3-grounded invariant (the property the fix must establish)
  ├── C1.5 cross-LLM-validates the invariant against the change request
  ├── C2 opens a throwaway git overlay
  ├── C3 generates the patch in the overlay
  ├── C5 generates a mutation-verified regression test
  ├── C6 attempts (one shot, no retries) to graduate the invariant to a
  │   cross-codebase principle; usually fails (per-codebase) and the
  │   bundle ships without a principle
  ├── D1 assembles bundle: patch + invariant + regression test + audit
  └── D2 emits patch.diff + pr-body.md + writes invariant to
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
  ├── re-resolve every invariant's bindings against current AST
  ├── enumerate paths to each protected callsite/sink via shadow AST
  ├── Z3-check each path against the universal property
  ├── decay → yellow alarm (binding can't resolve)
  ├── violation → red, commit rejected
  └── all hold → silent pass, hook exits 0
  │
  ▼
codebase is incrementally locked down. Every fix becomes a permanent
constraint. Every new commit gets verified against every standing
invariant, including invariants from changes nobody on the current team
remembers shipping.
```

## The user journey

The entry point that makes this product distribute itself:

1. User asks a vibe-coding agent to build a product. Agent ships a
   working-on-the-happy-path codebase with high interconnection and
   plenty of edge cases the agent didn't anticipate.
2. User uses the product. Surface works. Edge case breaks.
3. User goes back to the LLM and types "fix this bug."
4. **This is the install moment.** The fix loop runs inside the
   conversation the user was already having. The bug gets fixed AND the
   first invariant gets minted AND the git hook gets installed AND the
   `.provekit/` substrate gets bootstrapped. Zero friction. Zero new
   tooling decisions. Zero adoption ceremony.
5. From that moment forward, every subsequent change to the codebase
   ratchets the floor up. Each new "fix this bug" / "add this feature" /
   "make this change" produces another permanent contract. The vibe-
   coding agent that introduced the original bug now has to satisfy a
   growing corpus of constraints from every previous incident the user
   has reported.

The user never made a deliberate decision to adopt ProveKit. The product
got installed transparently inside the action the user was already taking.
Adoption is identical to the use case.

## The compound flywheel

```
user files change request
  ↓
fix loop produces patch + invariant + regression test
  ↓
invariant added to .provekit/invariants/
  ↓
git hook now enforces this invariant on every commit
  ↓
[time passes, AI generates new code]
  ↓
new code creates new dataflow paths
  ↓
git hook enumerates new paths, Z3-checks them against every invariant
  ↓
new code that violates ANY past constraint is rejected at commit time
  ↓
user files new change request (about a different bug)
  ↓
[loop closes; flywheel turns]
```

Each turn of the flywheel:
- Adds one constraint
- Permanently shrinks the future failure surface
- Costs the user one bug report
- Returns: one fix + permanent protection against bug-class regression

The compounding property: the AI's freedom to introduce regressions
**monotonically decreases** as a function of the codebase's change history.
The codebase becomes the substrate that *teaches the AI* which mistakes it
has already learned not to make in this context.

## The recursive depth

The vibe-coding AI is BOTH the source of the bugs AND the agent that fixes
them. The bugs that exist are exactly the failure modes the AI couldn't
prevent on its own. The invariants minted from those bugs are external
constraints the AI couldn't have derived for itself.

Each fix loop cycle teaches the codebase a constraint the AI didn't know
it needed. The substrate ends up encoding the AI's specific failure
topology — every mistake the AI has ever made in this codebase becomes
a permanent constraint the AI must satisfy on every subsequent generation.

The AI doesn't get smarter. The codebase gets smarter ABOUT the AI.

## Eschatology

A codebase built this way for years has properties no traditional codebase
can have:

**Software ages backwards.** Older code becomes more protected as the
invariant corpus wraps it in additional contracts. Refactoring gets safer
over time, not riskier. The cost of touching old code drops as the
invariant corpus grows around it.

**Velocity becomes correctness.** More commits = more contracts shipped =
stricter correctness floor. The team that ships 100 fixes a year ends up
with a more locked-down substrate than the team that ships 10. Vibe-coded
projects produce code at AI-speed and end up with the strongest contract
corpora because they're producing the most fixes.

**Tech debt inverts.** It points down, not up. Time wraps the codebase in
more protection, not less.

**The git history becomes a queryable knowledge graph.** Every commit
pairs a problem statement (the change request) with a formal specification
(the invariant) and an executable witness (the regression test). Lessons
can't fade because they're encoded in the substrate. Six months later,
when someone wonders "why does this work this way?", the answer is in the
invariant store, not in tribal memory.

**Code review changes shape.** Today: "is this change correct?" — a
review of behavior, requiring the reviewer to chase dataflow through their
head. With this paradigm: "do you agree the change request describes a
real problem and the invariant is the right specification?" The reviewer
reviews the spec; the runtime checks the dataflow. Review becomes 10x
faster and 10x more rigorous simultaneously.

**The codebase teaches new contributors mechanically.** A new human or
AI joining the project doesn't need to read documentation about "how this
codebase works." They run `provekit verify --list` and see every contract
the codebase pledges to satisfy. The contracts are executable
specifications. New contributors learn the codebase by reading the
invariants, not by reading prose.

**The codebase becomes its own senior engineer.** It never leaves; it
never forgets; it never gets distracted; it never lets a regression slide
because of velocity pressure. Every constraint it has ever learned, it
still enforces. The substrate is the institutional memory.

## Categorical reduction

Two surfaces, no heuristics:

- **Principles**: cross-codebase axioms. Rare. Hard-won. Pass adversarial
  validation against a universal corpus. Used by B3/recognize for fast-
  path bug recognition. Examples after re-validation: probably division-
  by-zero, integer-overflow-with-bounded-input. Maybe 10-50 entries total
  in a mature library, not hundreds.

- **Invariants**: per-codebase, hash-bound to specific AST nodes via the
  substrate. The default output of every fix loop run. Enforced by
  `provekit verify` git hook on every commit. Universal over paths to the
  bound sink in *this* codebase.

Heuristics are biome's product, not ProveKit's. Empty-catch, falsy-
default, exhaustive-deps, unused-vars — that whole surface is owned by
linters. ProveKit competing there has zero value differential. The
interesting space is exactly the two extremes: locally provable
(per-codebase invariants, the bulk of output) and universally provable
(principles, the rare graduation).

The validator IS the definition. A candidate that fails adversarial
validation isn't a flawed principle that better prompting can fix — it's
not a principle at all, and the validator is correctly telling us so.
There's no platonic principle hiding behind the noise waiting for the
right prompt to unmask it. Most changes don't yield a generalizable
shape, the validator detects that, and the right response is "ship the
per-codebase invariant only" not "retry with refinement." C6 should
fail-fast (one shot, no retries); 19-minute principle-refinement loops
were the architecture refusing to accept "no principle" as an answer.

## The pitch, in one line

> *Feed your change request into the product. Get out a codebase that can
> never regress. The git hook does the static analysis across every
> existing call site and every new call site that ever gets added,
> mechanically, no LLM in the verification path.*

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
3. The categorical reduction (principles vs invariants, no heuristics) is
   the source of truth for any future architecture decisions.
4. The marketing line is grounded in the architecture and traceable to
   the substrate that enforces it.
5. Future contributors can answer the question "why is provekit different
   from biome / TypeScript / unit tests?" with a coherent technical
   answer, not a marketing slogan.

## Open questions deferred to v2+

- **Symbolic node identity across renames/extractions/moves.** Current
  binding mechanism is content-addressable (sha256 of node content). A
  rename decays the binding and surfaces as an alarm asking the human to
  re-run the fix loop on the renamed locus. v2 can track identity across
  cosmetic edits without LLM involvement; v1 ships without it.
- **Cross-codebase invariant porting.** When and how do per-codebase
  invariants graduate to cross-codebase principles, beyond the C6
  adversarial validator? Is there a "common shapes across many
  invariants" emergent surface?
- **Distributed invariant stores.** Imports of one codebase's invariants
  into another (e.g., a shared library's contracts becoming the parent
  project's contracts).
- **Time-decay metadata.** Invariants tagged with their motivating change
  class so a violation report can name "this is the same class as X from
  six months ago."
- **Witness-replay.** When a violation is found, mechanically synthesize
  a failing test case from the Z3 witness so the developer sees a
  concrete failing input, not just a "Z3 says SAT" verdict.

These are v2+. v1 ships without them and remains the load-bearing claim.

## Bottom line

ProveKit's product is not "AI fixes your bugs faster." That's the
productivity story everyone else is selling. ProveKit's product is:
**every change request you file becomes a permanent universal constraint
on every line of code anyone (human or AI) ever writes from then on,
mechanically enforced at git-commit speed.** Software ages backwards.
Velocity becomes correctness. The codebase ratchets toward provability,
monotonically, with no decay over time. That's the moat. That's the
product. That's the bow.
