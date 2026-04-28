# Constraint-Driven Development

> **ProvekIt.** The k is silent. The slogan is the product: *Prove It.*

**Date:** 2026-04-27
**Status:** Positioning + methodology doc, paired with the standing-invariant-runtime spec
**Author:** Captured from a session-long architectural dialogue with TSavo

## What this is

A methodology and a product positioning. The methodology, named honestly:
**development whose trajectory is driven by the codebase's accumulating
impossibilities — the things it cannot become.** Each fix mints a permanent
constraint; the cumulative constraint set monotonically reduces the
codebase's degrees of freedom; the value is in the reduction. ProvekIt's
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

That gap is the entire ProvekIt product. The fix loop + shadow AST + Z3
path enumeration + git-hook construction is the only construction that
fills it, and it does so as a side effect of the action a developer was
already going to take ("fix this bug" / "add this feature" / "make this
change"). The constraint accumulates as a consequence of normal
problem-solving; no separate ceremony required.

## Two intake directions, one pipeline

The intake stage generalizes over two equally-valid input shapes:

- **Prospective intake.** The user files a problem statement before the
  change has happened. "The system stops responding to recent feedback
  after 30 invocations." The fix loop derives intent, mints a constraint,
  generates a patch that satisfies it, and ships a regression test.
- **Retrospective intake.** The change has already happened — there's a
  commit in git history with a diff and a commit message. The intent
  extractor reads both, derives what the change was trying to accomplish,
  mints a constraint capturing that intent, and ships any regression test
  that would lock the intent in. If the existing test suite already
  covers the intent, no test gets added; if it doesn't, ProvekIt writes
  the missing test as part of its output.

Both directions reduce to the same canonical operation: **artifact-of-
change → intent → constraint → output bundle.** The downstream pipeline
is identical regardless of which direction fed it. The only difference is
where the intent comes from. This unification is v0 architecture, not a
v1 extension — the framework is coherent because both directions live
under one intake.

The retrospective direction unlocks two important properties:
- **Existing codebases self-bootstrap.** Point ProvekIt at a five-year-old
  codebase and run the retrospective intake in batch over the existing
  commit log. Thousands of constraints get mined from history that nobody
  ever wrote down. The constraint corpus arrives populated.
- **Vibe-coded codebases self-test.** When the AI commits a feature
  without writing the corresponding test, the intent extractor reports
  the gap and the fix loop's output is a follow-up commit that adds the
  missing regression test. The codebase's test coverage grows
  mechanically alongside its constraint corpus, no human discipline
  required.

## The intent report

The pipeline's canonical structured output, regardless of intake
direction. JSON-shaped:

```json
{
  "source": "prospective" | "retrospective",
  "trigger": {
    "kind": "problem_statement" | "commit",
    "ref": "user-text or commit-sha",
    "diff": "<unified diff if retrospective>",
    "commitMessage": "<message if retrospective>"
  },
  "intents": [
    {
      "lineRange": [42, 48],
      "filePath": "src/store/sqlite/repositories.ts",
      "intent": "ensure most-recent K invocations reach evolve",
      "hasRegressionTest": false,
      "testGenerationOpportunity": true,
      "constraintCandidate": {
        "smtSketch": "(assert (and ...))",
        "kind": "order",
        "validationStatus": "candidate"
      }
    }
  ],
  "outputBundle": {
    "patch": "<diff if change is needed>",
    "addedTests": ["<test code if missing>"],
    "constraintArtifact": ".provekit/invariants/<sha>.json"
  }
}
```

The report is queryable, diffable, and source-controlled. It carries the
LLM's output through every gate (Z3 SAT, fidelity check, mutation
verification) and surfaces the result of each. When the LLM's intent
extraction fails any gate, the intent gets dropped from the bundle, not
shipped with a "trust me bro" caveat.

## One fork: the verifier's verdict

The deepest architectural compression: **the pipeline forks exactly once,
and the fork is mechanical**. Every input shape, every harness, every
intake direction, every input kind (bug report / change request / commit /
property assertion) routes through the same downstream pipeline. The only
branching point is the verifier's verdict on the derived invariant
against the current code:

```
intake (any harness, any input kind, prospective or retrospective)
  → B0 derives intent
  → C1 mints invariant
  → verifier checks invariant against current code (path enumerator + Z3)
       ├── holds   → emit invariant + missing tests, ship
       └── violated → C3 generates patch + emit invariant + missing tests, ship
```

The fork has nothing to do with whether the user typed "fix this bug" or
"add this feature" or whether a commit just landed or whether an LLM
agent called `/prove`. Those are all upstream concerns the harnesses
handle; the pipeline sees an intent and a codebase and asks Z3 whether
the codebase satisfies the intent's invariant. Z3's verdict is the only
routing signal. No spiky-intelligence decisions inside the pipeline.

Two consequences of this compression that simplify the architecture:

**Bug-vs-change is non-distinguishing.** Both produce intent → invariant
shapes the verifier handles identically. A bug report yields an invariant
that says "this state must not be reachable." A change request yields an
invariant that says "this property must hold." The verifier treats them
the same way: ask if the code currently satisfies the property.

**Retrospective-vs-prospective is non-distinguishing.** Both produce the
same artifact-of-change shape feeding into B0. A landed commit's diff
yields one intent; a typed problem statement yields one intent; the
verifier doesn't know or care which. It checks the invariant against the
codebase as it stands.

What differs between the cases is the empirical distribution over
verdicts:

- **Retrospective intake (commit just landed)** typically returns "holds"
  — the commit usually establishes the property. C3 is skipped; we get
  invariant + missing tests. (Exception: the commit may have fixed only
  one path while the invariant covers a wider sink. Verifier flags
  un-protected paths; C3 fires; codebase gets the fix the human didn't
  write.)

- **Prospective intake (user states a property)** typically returns
  "violated" — the user is filing because something's wrong. C3 fires.
  (Exception: the user is asserting something that's already true.
  Verifier returns "holds"; ship invariant + missing tests; no patch.
  The user gets a permanent constraint without any code change.)

The pipeline doesn't know about these distributions. It just routes on
verdicts. The cases above are emergent properties of how the harnesses
shape inputs, not architectural fork-points.

This is what makes "the constraint is the product" mechanically true.
**The invariant is always shipped. The patch is conditional
infrastructure to make the invariant true when the code disagrees.** The
invariant has primacy; everything else (test, patch, hook installation)
is in service of locking it in. A run that ships only an invariant + a
test (no patch) is a complete and successful pipeline run, not a
degenerate case. The constraint corpus grew by one; the codebase
already satisfied it.

## The mechanism, end-to-end

```
[INTAKE] either:
  - user files a problem statement (prospective), OR
  - a commit lands or already exists (retrospective: diff + message)
  │
  ▼
B0: intent extractor reads the input, produces intent report (JSON)
  │
  ├── for each identified intent:
  │   - is there a regression test that locks it in?
  │   - is there a constraint-shape (SMT-expressible) candidate?
  │
  ▼
fix loop processes each intent:
  │
  ├── Investigate stage maps intent → candidate code sites
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
  ├── D1 assembles bundle: intent report + patch + constraint + regression
  │   test (including any that were missing from the intake) + audit
  └── D2 emits patch.diff + pr-body.md + writes constraint to
      .provekit/invariants/<sha>.json + writes any added tests to the
      appropriate test paths
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

## Input shapes the gate handles

The architecture is one pipeline (artifact-of-change → intent → constraint
→ output bundle) with thin input adapters on top. The pipeline is
invariant; the inputs vary. Five conceptual input shapes feed the same
universal gate, distinguished by *who* triggers them, *when*, and *how*
the output gets routed. These are input shapes the pipeline accepts, not
products ProvekIt ships. (See "Distribution: two channels" below for the
actual product surface.)

1. **Interactive shape — "fix this bug" / "make this change."** A user
   files a problem statement directly to the LLM in conversation. The
   pipeline runs prospectively, ships a patch + invariant + tests. The
   bug-fix variant reads from GitHub Issues; the change variant reads
   from any issue tracker. Same pipeline, different inboxes — and the
   inboxes are third-party, not something ProvekIt ships adapters for.

2. **Historical shape — `provekit mine-history`.** Walks the existing
   git log, formats each commit as artifact-of-change, runs the pipeline
   retrospectively. Bootstraps the constraint corpus on adoption day for
   codebases with years of existing history.

3. **Continuous shape — pre-receive / PR webhook.** Every commit (or
   PR) gets handed to a ProvekIt agent asynchronously. The agent enhances
   the change with whatever the codebase needs to satisfy correctness:
   adds missing tests, mints constraints, opens a follow-up PR with the
   augmentations against the user's branch. Never commits to main
   directly. Always reviewable.

4. **Report-only shape — verify-and-file.** Zero write authority. Runs
   `provekit verify` on schedule (cron, GitHub Actions); when anything
   decays or violates, files a GitHub Issue with the binding details and
   Z3 witness. The output of this shape becomes input to the interactive
   shape — the issue gets typed into the LLM, the fix loop runs.
   Recursive feedback: report creates the work queue interactive consumes.

5. **MCP shape — `/prove <natural-language assertion>`.** Any LLM
   agent that speaks MCP (Claude Code, Cursor, agentic IDE plugins,
   Copilot if it gains MCP) gets prove-as-a-tool. The agent passes a
   declarative property in natural language; ProvekIt derives an
   invariant, runs the verify pipeline against the codebase, returns a
   structured verdict (holds | violated | undecidable + Z3 witness).
   Read-only by default; optionally promotes the property to a
   permanent constraint. Ad-hoc verification at conversational speed.

The five span the cost-vs-friction grid: interactive is highest-friction
highest-leverage; historical is one-time bootstrap; continuous is
zero-friction always-on; report-only is zero-write safety mode; MCP is
conversational pull. The product is the gate; these are the input shapes
the gate handles. ProvekIt does not ship Linear webhooks, Slack bots,
GitHub Issues integrations, email connectors, IDE plugins for every
editor, or custom event-bus subscribers. Anyone can build those on top
of the CLI; ProvekIt's job is to expose a clean enough gate that they
become trivial to wire.

## Distribution: two channels

The five input shapes above describe what the gate can consume. The
distribution story is shorter and operationally cleaner: ProvekIt ships
through exactly **two channels**.

### Channel 1 — CI Action

`provekit verify` runs as one step in any developer's existing CI
pipeline. GitHub Actions, GitLab CI, Buildkite, Jenkins, plain shell —
the developer adds one step; ProvekIt becomes a required check; branch
protection blocks merges that fail. **That's the install.** No bespoke
integration, no separate dashboard, no event-bus subscriptions. The
GitHub Action wraps `provekit verify` and exposes its verdict to the
existing PR check surface every developer already understands.

### Channel 2 — Library entry points (IDEs, agent runtimes, platforms)

`provekit` exposes a clean library surface — typed entry points for
intake, verify, fix, and the standing-runtime store — that any IDE,
agent runtime, or platform can call. Claude Code and Cursor integrate
ProvekIt to prove correctness during agent sessions. Holyship integrates
ProvekIt as a gate in its gate library, intercepting the agent's `report`
boundary and running the full pipeline before the entity advances.
(Holyship is described in detail in its own section below as a worked
example of the agent-runtime integration shape.) Future IDEs and agent
runtimes plug into the same library entry points; ProvekIt doesn't
write per-IDE plugins, it provides the surface the integrators target.

### What's explicitly NOT the product

These exist conceptually as input shapes but are not artifacts ProvekIt
ships, bills for, or supports as first-class integrations:

- Linear webhooks
- GitHub Issues bots
- Slack integrations
- Email connectors
- Per-IDE plugins (the IDE owners ship those, calling our library)
- Custom event-bus subscribers

They're third-party adapters anyone can build on top of the CLI or the
library. We don't write or maintain them.

**The marketing line:**

> *ProvekIt is the fourth horseman of the git commit — tsc, lint, test,
> prove. Every developer adds it to their CI. Every IDE integrates it
> to prove correctness.*

## The two-part architecture: A is the product, B is a plugin

ProvekIt decomposes into two parts with **low coupling** between them and
**high cohesion** within each. They communicate through one artifact: the
invariant store on disk. This decomposition is the load-bearing axis of
the entire architecture.

### Part A — The gate (the part that ensures correctness)

Mechanical, deterministic, fast, no LLM in the verification path.
`provekit verify`, the path enumerator, the Z3 path checker, the cache
layer, the invariant store reader. Lives at every venue: file-edit hook,
pre-commit, PR check, IDE diagnostic, Holyship gate boundary, MCP
`/prove` (read mode). Always-on, cheap, sub-second-to-seconds.

**Part A is the product.** It's small, mechanical, auditable, free. The
fourth horseman of the git commit. The thing every developer adds to
their CI. The substrate that turns "AI velocity is dangerous" into
"AI velocity compounds correctness."

### Part B — The constraint-minting pipeline (the part that expands correctness)

LLM-touching, expensive, full-pipeline. B0 (intent extraction) → C1
(invariant minting) → C3 (patch generation when needed) → C5 (regression
test generation) → C6 (rare promotion to principle). Runs at gate-
promotion moments only: explicit user request, offline-on-commit harness,
MCP `/prove` (write mode), Holyship `report`-boundary gate. Minutes per
invocation; token cost matters.

**Part B is a plugin.** It's a *slot* in the architecture, not a single
implementation we ship. ProvekIt provides a reference implementation of
B that uses claude-agent-sdk + ts-morph + git worktrees, but that's one
option among many. Integrators bring their own:

- **LLM.** Claude, GPT, local Llama, an enterprise on-prem model. The
  `LLMProvider` abstraction is the well-known one, but it's one of many
  swappable pieces.
- **Language server bindings.** Our reference uses ts-morph to read
  TypeScript ASTs. An IDE has its own LSP that already knows the
  codebase deeply; an integrator's B might bind there instead of
  re-parsing.
- **Toolpath.** How agent tools (Read, Edit, Write, Bash) are invoked.
  Claude Code has its own tool model; Cursor has its own; an enterprise
  sandbox has yet another. B's reference uses the claude-agent-sdk's
  tool surface; an integrator's B uses whatever their environment
  ships.
- **Code sandbox.** Where generated code runs during validation. Our
  reference uses git worktrees on local disk. An IDE might use its
  in-process sandbox; a hosted runtime might use a container or microVM.
- **Diff process.** How code changes get produced and applied. We
  produce unified diffs against a worktree. An IDE's B might produce
  in-editor edits via its own diff model.
- **PR flow.** How the output gets routed back. We emit `provekit-fix.patch`
  and `provekit-fix.md`. An integrator's B might open a PR via the IDE's
  source-control API, comment on a Linear ticket, post to Slack, or
  drop a follow-up commit on a branch.

What B must produce to be a valid implementation: an `IntentReport` JSON
artifact (per the runtime spec's schema), and any minted invariants
written to `.provekit/invariants/<sha>.json` in the project's invariant
store. That's the contract. Anything that produces those artifacts —
through whatever LLM, whatever toolpath, whatever sandbox — is a valid B.

### Why the split matters strategically

**Part A is permanent. Part B is a moving target.**

A doesn't change as LLMs get better. The path enumerator, Z3, the cache,
the invariant store — none of that gets replaced when frontier models
ship. A is small, mechanical, auditable, free; the kind of code that
lives 20 years in millions of CI pipelines without ever needing a major
version bump.

B, by contrast, gets cheaper and better every quarter on someone else's
R&D budget. The intent extractor, C1, C3, C5 — every part of B benefits
from frontier-model improvements without us shipping anything. We swap
the LLMProvider implementation (or the integrator swaps the entire B
plugin) and B gets sharper. We don't compete on B; we ride the curve.

The competitive shape that follows:

- **A is the moat.** Open-source, ubiquitous, the thing every developer's
  CI runs and every IDE integrates. Distribution by zero friction. The
  constraint corpus that accumulates inside each customer's codebase IS
  the customer's data; we don't own it, but we're the only thing that
  produces it. Switching costs grow with each constraint minted.

- **B is replaceable infrastructure.** We ship a B today as a reference
  implementation because no one else does it well yet. Whoever ships a
  better B in two years can take over that piece without disturbing A.
  The customer's constraint corpus survives any B swap. The customer
  can BYO LLM (via LLMProvider), BYO toolpath (via the IDE's plugin),
  BYO sandbox, BYO diff process — without leaving A.

- **Token cost lives in B, not A.** A user with 1,000 invariants on
  their codebase pays exactly zero per `provekit verify` run. The
  customer who wants to mint MORE invariants pays per fix-loop run.
  That's the natural pricing surface: A is free because A is cheap to
  run; B is paid because B has real LLM cost. As LLMs get cheaper, B
  gets cheaper. Whoever owns A wins the long game even if B becomes a
  race-to-the-bottom commodity.

**The marketing implication:** never lead with "we use the best AI to
fix your bugs." Every competitor says that. Always lead with "we make
your codebase mathematically refuse to regress." The first claim ages
out by next quarter; the second ages forever. ProvekIt's pitch must
always be A. B is the means; A is the product.

The LLMProvider abstraction we already have is the explicit shape of
the A/B coupling. Customers can plug Anthropic, OpenAI, local Llama,
whatever frontier model they trust — A doesn't care. The full B-as-plugin
extension goes further: a customer can plug their own intent extractor,
their own toolpath, their own sandbox. The constraint store is OUR data
structure; whatever populated it is just a tool that wrote into it.

## The pipeline as a composition of swappable processes

Once you accept "B is a plugin," the right next move is recognizing
that B is *not one plugin*. B is a **composition of N plugins**, one
per pipeline stage, each with its own structured contract.

The reference fix loop chains these stages:

```
B0 → Investigate → Locate → Classify → B3 → C1 → C1.5 → C2 → C3 → C4 → C5 → C6 → D1 → D2
```

Each box is a process behind an interface. Each can be replaced by any
other process that respects the same contract:

- **Investigate** — `(BugSignal, ProjectTree) → CandidateLocation[]`. Our
  reference is an LLM call; an integrator could use code search, an LSP
  query, or a deterministic heuristic for known bug classes.
- **Locate** — `CandidateLocation → BugLocus`. Our reference uses
  ts-morph + dataflow walks; an IDE that already has an LSP can bind
  there instead of re-parsing.
- **C1 formulate-invariant** — `(BugSignal, BugLocus) → InvariantClaim`
  (with Z3 SAT proof). LLM today; could be deterministic for recognized
  bug-class shapes.
- **C2 sandbox** — `ProjectRoot → OverlayHandle`. Git worktree on local
  disk today; could be a container, microVM, in-process VFS, remote
  workspace.
- **C3 generate-fix** — `(OverlayHandle, InvariantClaim) → FixCandidate`.
  Our LLM agent in an overlay; could be a different LLM, a different
  toolpath, a deterministic synthesizer.
- **C5 generate-regression-test** — same shape, same swap-ability.
- **D2 apply-bundle** — our cherry-pick onto target; could be the IDE's
  source-control API, a PR-creation service, a Slack-message-with-diff
  bot.

Every stage is fungible because every stage has a stable, structured
contract. The reference fix loop pipes them in our preferred order with
our preferred implementations. An integrator can replace any single
stage without disturbing the rest. Want our Investigate but your own
C3? Plug your C3 into the chain. Want our gate but a deterministic
invariant-derivation backend instead of an LLM C1? Implement C1 as a
SAT solver and feed it forward.

This is the deepest expression of the Unix nature: **not one tool, but
a pipeline of swappable tools, each contracted, each replaceable.** The
reference fix loop is one particular `find | grep | xargs` of these
stages; an integrator can write their own pipeline using their own
tools at any stage.

Practical consequence: every stage's input/output type should be a
public, documented contract. The library entry points expose stages
individually, not just `runFixLoop`. An integrator who wants only our
C5 imports `generateRegressionTest` directly, feeds it their own
InvariantClaim, gets a TestArtifact back. They don't need our
orchestrator.

This is also what makes "Part B as plugin" genuinely true rather than
aspirational. B isn't ONE plugin slot — B is N plugin slots, one per
stage, each with its own contract. Integrators don't have to choose
between "use ours entirely" and "rebuild from scratch"; they pick
stage by stage.

What we ship today (the entire fix loop machinery: Investigate, Locate,
Classify, B3, C1, C1.5, C2, C3, C4, C5, C6, D1, D2, plus B0
retrospective, plus mine-history, plus the orchestrator, plus the
LLMProvider abstraction) is collectively **the reference Part B
implementation**. Not the canonical product surface; a worked example
of what the contract looks like when you implement it. Same shape as
Linux + GNU coreutils: the kernel is canonical (Part A); coreutils is
one reference implementation of the userspace contract; busybox and
Darwin are alternatives. The contract is what's stable; implementations
vary.

## Language partitioning: principles per language

The principle library is **partitioned by language**, not flat. Universal
axioms apply everywhere. Language-specific axioms apply only inside
their language's universe.

- `.provekit/principles/universal/` — applies to any language. The
  starter seven (division-by-zero, modulo-by-zero, NaN equality,
  null/undefined dereference, unhandled async failure, array index
  out of bounds, use-after-close).
- `.provekit/principles/c/` and `.provekit/principles/cpp/` — memory
  safety axioms (use-after-free, double-free, buffer overflow, format-
  string mismatches, return-pointer-to-local, integer signed-overflow
  as UB, strict aliasing). Largest language-specific set because the
  language's compile-time safety story is weakest.
- `.provekit/principles/java/` — catch-Throwable, synchronized-DCL
  needing volatile, ConcurrentModificationException via stale iterator,
  string-equality-by-`==`, etc. Smaller set; Java's runtime catches
  more.
- `.provekit/principles/python/` — mutable default argument, late-
  binding closures, `==` vs `is`, integer-division semantics. Small.
- `.provekit/principles/rust/` — arithmetic overflow in release mode,
  async-cancellation safety, Drop-order surprises. Smallest set; the
  borrow checker enforces most of what would be axioms.
- `.provekit/principles/go/` — nil-pointer deference (different shape
  from JS), goroutine leaks, panic in deferred function, time.After
  leaks in select loops.
- `.provekit/principles/typescript/` — `==` vs `===` beyond NaN,
  `this` binding in nested vs arrow function, hoisting with `var`,
  truthiness coercion edge cases.

Each language's bounded set stays small — typically 3-12 entries.
Total library across all languages: maybe 40-60 entries in a mature
state, never thousands. The size per language is *inversely
proportional* to the language's compile-time safety story: stronger
type system → more axioms absorbed by the compiler → smaller language-
specific principle set.

B0/C1 detects the project's language(s) at intake (TypeScript via
package.json, Rust via Cargo.toml, Go via go.mod) and matches against
universal + language-specific principle sets only. Cross-language
matching is wasted work and a false-positive risk.

## Rust gets value too: the layer above the type system

The principle library is small for Rust because the borrow checker
already enforces what would otherwise be axioms. But the **per-codebase
observation corpus is unbounded everywhere, and Rust's clean substrate
makes those observations *more* enforceable** than in any unsafe
language.

What ProvekIt's observation layer adds to a Rust codebase that the
type system can't catch:

- **Business-logic invariants.** "Every withdrawal debits account A
  and credits account B atomically." Rust types model values, not
  domain rules.
- **Cross-module data integrity.** "Sum of debits equals sum of credits
  across the ledger." Whole-program reasoning the borrow checker
  doesn't perform.
- **State-machine invariants.** "Order can only transition pending →
  paid → shipped." Rust supports this with type-state, but most code
  doesn't use type-state because it's expensive to write. ProvekIt
  mints the invariant from a real bug; enforces it forever after with
  no rewrite.
- **API-contract invariants.** "Every authenticated request carries a
  non-expired token at any call site requiring auth." Types express
  presence, not expiration or auth-context-required-here.
- **External-system invariants.** "Every database write is followed by
  commit-or-rollback before the connection closes." Compiler can't see
  the database; ProvekIt models resource lifecycle as state transitions.
- **Concurrency observations beyond data races.** Rust prevents data
  races; doesn't prevent deadlock patterns. Universal-over-paths
  invariants catch lock-ordering violations and similar.

Why those observations are MORE enforceable in Rust than in C/C++:

1. **Cleaner substrate** — regular syntax, explicit lifetimes, annotated
   mutability give the path enumerator free metadata.
2. **Borrow check as precondition** — aliasing/lifetime properties are
   already proven by the compiler; ProvekIt's reasoning gets to assume
   them.
3. **Pattern-matching exhaustiveness** — match arms are statically
   exhaustive, making state-machine observations machine-checkable.
4. **Const-evaluation** — `const fn` and const generics give the path
   enumerator concrete values to symbolic-execute over.
5. **Pre-filtered bug population** — bugs that survive into a Rust
   codebase are almost by definition domain-logic bugs; ProvekIt's
   value is highest exactly where the language stops doing your work.

Strategic asymmetry across the market:

- **C/C++:** Highest-pain, weak-existing-solution. Pitch is "Don't
  rewrite in Rust. Install ProvekIt with the C++ axiom set; get
  Rust-like memory-safety guarantees on your existing codebase."
- **Rust:** Low-pain on primitives, high-pain on domain logic. Pitch
  is "You already have memory safety. ProvekIt adds the layer above
  the type system — business logic, state machines, cross-module
  invariants, API contracts. The compiler catches what fits in a
  type; ProvekIt catches what doesn't."

Both markets are strong; different sales, equally legitimate. The
universal axiom set is the same in both. The language-specific
principle partition is large for one, small for the other. The
observation corpus is unbounded in both — and that's the value-
accumulating surface.

## Product constraints: ProvekIt's own impossibility set

The architectural decisions in this doc form a constraint-driven spec
*about the product itself*. ProvekIt's shape is the intersection of an
accumulating set of impossibility statements — what ProvekIt *cannot
become* — derived through the same methodology the product enforces on
its users' code. Capturing them explicitly:

1. **No LLM in the verification path.** The gate is mechanical or it
   isn't a gate.
2. **No SaaS-only deployment as the canonical surface.** ProvekIt must
   run locally; SaaS is one venue among many.
3. **No bundled integrations** (Linear, Slack, IDE-specific plugins,
   webhooks). Composition through Unix shapes is the substitute. We
   ship the tool; the world ships the pipeline.
4. **No principle without adversarial validation.** The validator IS
   the definition; failing candidates are not principles.
5. **No silent constraint removal.** Decay is an alarm requiring
   human acknowledgment; never a quiet retraction.
6. **No heuristic tier.** Biome / ESLint own that surface; ProvekIt
   competes only at the universally-axiomatic level (principles) or
   the per-codebase-bound level (invariants).
7. **Part B is a plugin, not a product.** The constraint-minting
   pipeline is a slot integrators implement; ProvekIt ships a
   reference B but not THE B.
8. **Part A is open source.** The gate is the moat through ubiquity,
   not exclusivity. Locking it down forfeits the distribution.
9. **One pipeline fork only.** The pipeline forks once, on the
   verifier's empirical verdict. Never on intake direction, never on
   bug-vs-change, never on who triggered.
10. **No constraint without a regression test that locks it in.** The
    test is the executable witness. Constraints that cannot be tested
    are not constraints.
11. **No invariant without Z3 SAT proof of reachability.** Oracle #1
    is the existence proof. Untestable claims don't enter the corpus.
12. **No regression test without mutation verification.** Oracle #9 is
    the test-quality proof. A test that passes against the unfixed code
    is a placebo and is rejected.
13. **No LLM-tier work in pre-commit blocking context.** The
    LLM-touching pipeline runs at gate-promotion moments only:
    explicit user request, offline-on-commit, agent-runtime tool-call
    boundary. Pre-commit is mechanical-only.
14. **No marketing claim without mechanical backing.** Every
    user-facing assertion has a Z3 proof, an audit trail, or a clear
    disclaimer. "We use the best AI" is permanently off the
    pitch.
15. **No silent scope changes.** A constraint scoped to one callsite
    cannot quietly broaden to a sink-level scope, or vice versa. Scope
    changes are explicit retire-plus-remint operations, surfaced in
    the audit trail.

These 15 constraints define ProvekIt by negative space. Anything that
respects all 15 is ProvekIt; anything that violates any one isn't,
no matter how feature-rich or well-marketed. The product is the
intersection.

This is also the product's promise to its users: every constraint here
is permanent. Future versions tighten the set or add to it; they do not
relax existing items. ProvekIt ages backwards too — older versions of
the product are *not* less constrained than newer ones. The same shape
the methodology applies to user codebases applies to ProvekIt itself.

## Operational layering: when does each piece run, and who owns the gate?

The five input shapes describe *who* triggers the pipeline. The
operational layering describes *when* each component within the pipeline
runs, what it's allowed to invoke, and — equally importantly — *who owns
the gate at each tier*. "Where to install ProvekIt" is a gate-ownership
decision: different stakeholders pay different latency budgets, and
ProvekIt's job is to expose a clean enough interface (CLI, library,
GitHub Action) that each gate-owner can wire it in.

Two strict rules govern the layering:

1. **Static analysis runs everywhere, all the time.** Z3, path
   enumeration, decay detection, cache lookup — these are deterministic
   and fast (cache-warm). They have no token cost and no LLM dependency.
   Run them on every keystroke if you can; certainly on every commit.

2. **The LLM runs only at gate-promotion moments.** Intent extraction
   (B0), invariant minting (C1), patch generation (C3), test generation
   (C5) — these have token cost and LLM-shaped failure modes. Fire them
   only at moments where a permanent obligation is being created or a
   user has explicitly asked for one.

Under those rules, the operational layering breaks into four tiers:

### Tier 1 — File-edit / pre-commit (sync, static, sub-second to seconds)

**Gate owner: the developer.** They installed the hook; they own the
latency budget.

Triggered by IDE on-edit, Claude Code hook on save, or `git commit`. The
on-edit variant runs `provekit verify --changed-files <paths>` and
re-checks only invariants whose bindings touch the changed files; the
rest cache-hit. The pre-commit variant runs the four horsemen — tsc /
lint / test / prove — and blocks the commit on any violation. No LLM in
the verification path at either sub-tier.

The developer gets continuous correctness pressure at typing speed and a
deterministic gate at commit time. Every commit, no matter how fast it
was generated, gets verified against the codebase's accumulated
impossibility set before it lands. This is the deterministic gate that
prevents regressions of any standing constraint. It is the moat.

If performance pushes the on-edit path past one second per edit, the
cache is failing and the architecture has a real bug.

### Tier 2 — Repo / PR check (async, full pipeline, blocks merge)

**Gate owner: the repo.** Branch protection rules require a passing
ProvekIt verdict before merge. The PR sits open while CI runs ProvekIt;
the merge button stays grey until verdict + augmentations land.

Triggered by a PR open or push to a protected branch. The CI Action
runs `provekit verify` and, optionally, the full pipeline — B0 captures
the diff + message as an intent, C1 mints a candidate constraint, the
verifier checks the codebase, conditional C3 generates any patch needed,
C5 emits any missing test. Output gets routed back as a follow-up commit
on the user's branch with the augmentations.

This is the *constraint-minting* tier. Tier 1 enforces the existing
corpus; Tier 2 grows it. The repo owner sets the policy; the developer
pays no commit-time latency for the constraint-minting half — the work
happens between when they commit and when they next look at their PR.

### Tier 3 — Agent-runtime / tool-call boundary (sync, full pipeline, minutes)

**Gate owner: the agent runtime.** When the agent emits `report`, the
flow engine intercepts and runs ProvekIt against whatever the agent
produced. The agent stalls until the gate returns.

This is exactly the gate Holyship is designed to host. Holyship's
`claim` / `report` API and its gate-on-evidence architecture are the
natural home for ProvekIt's full pipeline. The agent waits because the
gate is doing the work the agent was supposed to do; the 20-minute
pause when the pipeline runs end-to-end is correct behavior, not a bug.

Triggered every time an agent in Holyship's worker pool produces a
report. The gate runs the full pipeline; the agent transitions only on a
passing verdict. Every prove-gate failure that ships through the fix
loop mints a NEW constraint, so the gate-set the next agent must satisfy
is strictly larger than the previous one. The gate library grows
monotonically as the codebase ships work.

### Tier 4 — IDE / session boundary (sync, varies, IDE policy)

**Gate owner: the IDE.** Claude Code, Cursor, or similar intercepts the
agent's session-end and runs ProvekIt before showing the diff to the
user. The IDE owns the gate; the user opted in via hook config.

Triggered when the user explicitly invokes the pipeline (`provekit fix
<issue>`, the MCP `/prove` tool inside an LLM conversation, an issue
typed from GitHub Issues into the interactive shape) or when the IDE's
session-end hook fires. The full pipeline runs; cost is paid because
the user (or the IDE policy on behalf of the user) explicitly chose
to invoke it.

### Why the layering matters

The two rules at the top — "static everywhere, LLM at promotion only" —
are what make ProvekIt *cheap to run continuously and expensive only at
moments that earn the cost*. The four tiers exist because four
different stakeholders own four different gates and pay four different
latency budgets:

- The **developer** pays sub-second to a-few-seconds at edit and commit time.
- The **repo** pays minutes asynchronously, blocking merge.
- The **agent runtime** pays minutes synchronously, blocking the agent.
- The **IDE** pays whatever its policy budgets at session boundaries.

The same code path serves all four tiers — the verifier's static gate
is the deterministic core; the LLM-touching pipeline is the
constraint-minting layer. Only the trigger, the gate-owner, and the
latency budget change. The architecture exposes the right surface
(`verify`, `fix`, MCP tool, GitHub Action, library imports) for each
gate-owner; the underlying machinery is the universal pipeline this spec
describes.

This layering is what makes constraint-driven development *operational*
at every team size. The expensive tier is amortized across the team;
the cheap tier runs at every keystroke; the gates compose; correctness
ratchets up monotonically while developer velocity is preserved.

## Holyship: a worked example of agent-runtime integration

Holyship (the flow engine + worker pool for agentic software at
`~/platform/platforms/holyship`) is the prominent example of channel 2
in action: an agent runtime that integrates ProvekIt as a gate in its
gate library. Holyship is one integrator among many — Claude Code,
Cursor, and any future agent platform integrates the same library
entry points. Holyship is described in detail here because it's the
cleanest fit for the agent-runtime gate-ownership model and because
it's where the four-horsemen-of-git-commit lineup gets enforced at
agent-velocity.

Holyship defines pipelines as state machines, enforces transitions with
deterministic gates, and gives agents only `claim` and `report`. The
engine — not the agent — decides what comes next, based on gate evidence.
ProvekIt sits at the agent's tool-call boundary: when an agent emits
`report`, the flow engine intercepts, ProvekIt runs the full pipeline
against whatever the agent produced, and the agent stalls until the
gate returns a verdict. The 20-minute pause when the pipeline runs
end-to-end is correct behavior; the agent waits because the gate is
doing the work the agent was supposed to do.

The four horsemen of git-commit map directly to Holyship's gate library:

- **tsc** gate — types consistent
- **lint** gate — patterns clean
- **test** gate — behavior matches assertions
- **prove** gate — universal-over-paths constraints satisfied

Each gate has the same shape from Holyship's perspective: deterministic,
fast (cache-bound), no LLM in the verification path, returns a
structured verdict the engine routes on. The fixer agent gets called
with the failing gate's evidence; the reviewer agent doesn't have to
read the whole diff because the gate already named the violation.

The compounding property gets even sharper inside Holyship: every
prove-gate failure that ships through the fix loop mints a NEW
constraint. The next agent that tries to write code in that area has to
satisfy a strictly larger gate-set. Holyship's gate library expands
monotonically as the codebase ships work. *Holyship doesn't just enforce
gates — it grows them.* That's the deepest fit: Holyship is the platform
that makes constraint-driven development operational at agent-velocity,
and prove is what makes the gate growth mechanical.

The MCP-able `/prove` tool is the agent-facing API for this gate. An
agent in the `coding` state writes code, calls `/prove the data layer
respects the most-recent-K invariant`, gets a verdict. Fail → fix; hold
→ emit `pr_created`. Every agent in Holyship's worker pool gets
prove-as-a-tool with no per-agent configuration.

Holyship is the worked example here because it's the cleanest
match between ProvekIt's shape and an existing agent-runtime
architecture. Other agent runtimes that integrate ProvekIt — Claude
Code, Cursor, future platforms — will land on similar shapes for
similar reasons. The library entry points ProvekIt exposes are what
makes that integration tractable for any of them.

## The user journey

The entry point that makes this product distribute itself goes deeper
than "fix this bug." The retrospective intake means the user doesn't have
to file anything at all — the act of committing IS the input.

Two adoption shapes, both invisible:

**Shape A — explicit problem statement.** User encounters an edge case,
types "fix this bug" to the LLM. Fix loop runs inside the conversation
the user was already having. Bug gets fixed, first constraint gets minted,
git hook gets installed, `.provekit/` substrate gets bootstrapped. Zero
friction. Adoption is identical to the action the user was already taking.

**Shape B — implicit commit-as-input.** User installs ProvekIt. From that
moment forward, every commit that lands gets retrospectively mined. The
intent extractor reads the diff plus commit message, derives intent,
mints a constraint where one is constraint-shaped, and ships a follow-up
that backfills any missing regression tests. The user did nothing
explicit; the substrate captured the intent of every change anyway. The
codebase's constraint corpus grows monotonically as a function of
ordinary commit activity, regardless of whether the developer is
deliberately filing problem statements.

In both shapes, the product gets installed transparently inside actions
the user was already taking. Adoption is identical to the use case. There
is no separate ceremony.

The compounding property kicks in regardless of intake direction. Each
mined intent — whether from an explicit fix request or from a routine
commit — produces another permanent constraint. The vibe-coding agent
that introduced the original bug now has to satisfy a growing corpus of
constraints from every previous change in the codebase, mined from every
commit that's ever landed. The corpus density grows with commit rate,
not with deliberate user effort.

For existing codebases adopting ProvekIt, the retrospective intake runs
in batch over the existing commit log. A five-year-old codebase with
thousands of commits gets thousands of mined intents on day one, the
constraint corpus arrives populated, and the standing runtime starts
enforcing immediately. There's no migration period; the codebase has the
same product as a greenfield project from minute one.

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

Heuristics are biome's product, not ProvekIt's. Empty-catch, falsy-
default, exhaustive-deps, unused-vars — that whole surface is owned by
linters. ProvekIt competing there has zero value differential. The
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

> *ProvekIt is the fourth horseman of the git commit — tsc, lint, test,
> prove. Every developer adds it to their CI. Every IDE integrates it
> to prove correctness.*

Underneath that one line:

> *Every commit your codebase ever takes — every fix request, every
> feature, every refactor — becomes a permanent constraint on what the
> codebase cannot do. The intent extractor mines intent from the diff
> plus message; the fix loop mints constraints and writes any missing
> regression tests; the git hook enforces every constraint on every
> subsequent commit, mechanically, with no LLM in the verification path.
> Code as fast as you want; the constraints accumulate at commit speed;
> possibility space locks down monotonically; software ages backwards.*

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

ProvekIt's product is not "AI fixes your bugs faster." That's the
productivity story everyone else is selling, and it's bounded by
human-equivalent quality.

ProvekIt's product is: **every problem your codebase encounters becomes a
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
