# Sugar: architectural notes for a rewrite from scratch

**Scope.** Hypothetical: same TypeScript + Drizzle + Claude Code stack,
but starting from an empty repository with everything we know now.
What would the architecture look like that we'd build today vs. the
one that accreted incrementally?

**Context.** Sugar's architectural lineage is hash-as-trust-anchor
applied across 30 years: Xdrive (1995, content-addressable dedup) →
Digital Confetti (1998, swarmed delivery + per-byte crediting) →
BitTorrent (.torrent shape via direct suggestion to Cohen) → eDonkey
fork for MST3K DAP (Kevlar's authored fork, still running 20+ years
later) → Apache JCS contribution (iFilm era, distributed cache regions)
→ Feathercoin (genesis fingerprint) → Sugar. Sugar is the third
major application of the same primitive that produced BitTorrent and
Bitcoin — applied to software correctness instead of file distribution
or transaction integrity. This document is what the codebase would
look like if we'd had that lineage clarity from line one.

---

## What stays unchanged

These are the load-bearing fundamentals. A clean rewrite preserves
all of them:

- **The mechanical gates around LLM stages, with "no LLM in the
  verification path" as a hard constraint.** Oracle #1 (Z3 SAT),
  Oracle #1.5 (cross-LLM agreement), Oracle #2 (Z3 verdict against
  patched code), Oracle #9 (mutation verification). The LLM is soft;
  the gates are hard. This is the load-bearing safety property; touch
  nothing here.
- **The four canonical prompts (Intake / Investigate / Formalize /
  Do-the-work).** This decomposition is right. Each handles a distinct
  cognitive task; together they span the LLM-touching pipeline.
- **IntentSignal as the unified intake type.** Bug = change request =
  property assertion. The pipeline doesn't care which; it cares about
  the property the user wants to hold.
- **Bindings as a tagged union (local + graph).** Local bindings hash
  bound source bytes; graph bindings walk a relation and apply a
  predicate. Adding new kinds is a new arm of the union.
- **The fundamental fix-loop sequence**: intake → locate → formulate →
  verify → patch-if-violated → test → bundle.
- **The corpus as content-addressable artifacts** (`hash(SMT + bindings)`
  as the invariant ID).
- **The bp library's artifact / revision / invocation / signal
  ontology.** Right shape. The change is making it cover *all*
  telemetry, not just LLM prompts.

---

## What changes

Six architectural moves a clean rewrite would make differently. In
priority order (1 is the highest-leverage):

### 1. Swarm-distributable corpus

**Today.** The corpus is flat JSON files in `.sugar/invariants/`,
local to each project. Sharing principles across projects = copy-paste.
A team importing a "starter pack" of invariants from another
Sugar-using codebase has no architectural path other than file copy.

**From scratch.** The corpus is a content-addressable Merkle DAG.
Each invariant is a DAG node with a content hash. The corpus has a
root hash that fingerprints the whole codebase's accumulated correctness
claims. Two codebases can compare correctness by exchanging root hashes.
**Pull by hash from a swarm**, not by filename from a directory.

**Why this is the highest-leverage move.** The architectural lineage
(BitTorrent, Bitcoin) is fundamentally about *network artifacts that
compound across users*. Sugar today ships content-addressable
observations but distributes them as local files. Without
swarm-distribution, Sugar is "BitTorrent-shape locally"; with it,
Sugar is "BitTorrent for correctness" full stop. The corpus
becomes a network asset, not a local one, and the framework's value
compounds across teams the same way BitTorrent's value compounded
across users.

**Concrete implementation sketch.**
- Each invariant gets a CID (content-addressable identifier) on mint.
- The corpus has a manifest tying invariant CIDs to a project root
  hash; the manifest is itself content-addressable.
- Pull/push operations work via a tracker (initially HTTP, eventually
  DHT). Importing a principle pack from another project = fetch by
  CID, verify hashes, splice into local corpus.
- Public principle libraries (the cross-codebase axioms — divide-by-zero,
  null-deref, etc.) are first-class swarm artifacts, not files we
  ship in the npm package.

### 2. One unified IR + plugin-shaped verifiers

**Today.** Two parallel verification languages: the DSL compiles to
SQL queries against the substrate; SMT compiles to Z3. JSON for
invariants. Separate schemas per stage. Verifier logic is
hand-dispatched on binding type at the top of `resolveBindings()`.

**From scratch.** One Intent IR, one Binding IR. Verifiers and
binding kinds register via a plugin interface:

```typescript
BindingKind.register({
  type: "local",
  verify: (binding, projectRoot) => contentHashCompare(binding),
});
BindingKind.register({
  type: "graph",
  verify: (binding, projectRoot) => walkAndEvaluate(binding),
});
```

Adding a new property kind (e.g. `flow_graph` for cross-function
dataflow reachability) is a one-line registration, not a pipeline
change. Adding a new verifier (e.g. Oracle #N for some new mechanical
check) is also a registration.

**Why.** The current tagged-union for bindings is partway there but
hand-rolled — extending it requires touching `resolveBindings()`,
`hashInvariant()`, the read-time normalizer, and the type definitions.
Plugin-shaped from day 1 means each new binding kind / verifier is
self-contained.

### 3. Content-addressable substrate

**Today.** `.sugar/sugar.db` rebuilds from source via
`analyze` (mechanical, 611 files / 770k AST nodes / ~5 minutes for
Sugar). Incremental indexing is limited; the substrate is a
project-level database, not per-file.

**From scratch.** Every AST node has an ID derived from its content
hash (similar to current `nodeHash` for bindings, but DAG-shaped).
Re-indexing becomes:

```
for each changed file:
  rehash subtree
  for each node whose hash matches the previous hash: do nothing
  for each node whose hash differs: re-index
```

Unchanged files produce the same node IDs across runs, so their
bindings stay valid without any work. This is Git's content-addressable
blob model applied to the AST level.

**Why.** Today's `analyze --substrate-only` takes minutes; a
content-addressable substrate makes verify-on-every-save genuinely
cheap (sub-second on a typical codebase, because most files don't
change). The substrate becomes a Git-shaped object database for code
structure, with the same incrementality and durability properties.

### 4. CLI verb-shaped from day 1

**Today.** Eight subcommands accreted incrementally: `prove`, `verify`,
`mine-history`, `analyze`, `lint`, `invariants`, `derive`, etc. Each
has its own arg shape, its own scope, its own help text. The semantic
boundaries between them aren't clean.

**From scratch.** `prove` is the canonical verb. Other operations
are parameterizations:

```
sugar prove                        — interactive intent → full pipeline
sugar prove --retrospective <ref>  — replaces mine-history
sugar prove --no-patch             — replaces verify
sugar prove --substrate-only       — replaces analyze
sugar prove --principle-mode       — replaces lint
sugar prove --inspect              — replaces invariants list
```

Same orchestrator, different parameterizations. The CLI surface
shrinks; the semantic surface stays. The user has one verb to learn
plus a small flag set.

**Why.** The current CLI surface is the kind of accretion that comes
from incremental development. From scratch, the verb taxonomy gets
designed first.

### 5. All telemetry content-addressable

**Today.** bp's invocation IDs and revision IDs are content-addressable.
But: fix-loop log files (`.sugar/fix-loop-*.log`) are timestamp-named.
Audit trails inside FixLoopResult are in-memory arrays. Substrate
build artifacts are file-based. The audit trail across a run is
fragmented across 4+ surfaces.

**From scratch.** Every LLM invocation, every oracle outcome, every
patch attempt, every substrate rebuild — all hash-addressable nodes
in the same DAG as the corpus. Replay-by-hash works for any past
operation. The audit trail and the corpus are the same kind of thing —
just leaves with different metadata.

**Why.** This is the natural extension of bp's design to the rest of
the framework. If telemetry is content-addressable, then "show me
why this invariant was minted" is a DAG walk; "replay this fix-loop
run with a different prompt revision" is a hash lookup; "diff the
audit trail between runs" is a Merkle diff.

### 6. Intent IR layer between LLM and SMT

**Today.** C1 prompts the LLM to emit SMT formulas directly. The
prompt is taught to produce well-formed SMT-LIB output; downstream
oracles (Oracle #1, Oracle #1.5) check the SMT.

**From scratch.** The LLM produces a typed, structured **Intent IR**
— a JSON object with explicit fields for property kind, bindings,
quantifiers, etc. A separate mechanical compiler translates Intent
IR → SMT.

```typescript
type IntentIR = {
  kind: "ordering" | "presence" | "bound" | "shape" | "absence";
  bindings: BindingSpec[];
  property: PropertyExpression;
  rationale: string;
};

function compileToSmt(intent: IntentIR): SmtFormula { ... }
```

**Why three concrete benefits.**
- **Testable.** The Intent IR has a deterministic shape; tests can
  assert against it without running Z3.
- **Replaceable backend.** Today Z3 is the verification engine; if
  we ever want to add another (Vampire, CVC5, a custom symbolic
  evaluator), only the compiler changes. The prompts and the rest of
  the pipeline stay put.
- **Evolvable prompt surface.** When bp.evolve rewrites the prompt,
  the contract it must preserve is "produce valid Intent IR" — a
  much smaller, more checkable target than "produce valid SMT-LIB."

---

## The meta-observation

Most of these changes are *cleanup of incremental accretion* — the
CLI surface, the verifier dispatch, the multiple IRs, the local-only
corpus, the LLM-only-bp-coverage. The architectural fundamentals are
right; they're just buried under accretion.

The exception is **#1 (swarm-distributable corpus)**, which is a
genuine missing piece — the closing of the architectural lineage.
Sugar today applies the hash-trust primitive locally; the move to
swarm distribution is what makes it the third major application of
the BitTorrent → Bitcoin → Sugar arc, not just the third local
implementation.

---

## If one thing

If forced to pick the single highest-leverage architectural change
from this list to retrofit into the existing codebase: **#1, the
swarm-distributable corpus.** Everything else is internal cleanup
that doesn't change what Sugar *is*. #1 changes what Sugar is.

---

## What this is for

This document is a forward-design note, not a refactor plan. It
captures the architectural decisions a clean rewrite would make so
that future incremental work can pull in the direction of those
decisions even without rewriting. When a contributor adds a new
binding kind, the question becomes "is this plugin-shaped or am I
hand-rolling like the existing tagged union?" — and the answer is
informed by this doc.

It's also a pitch document. The "if one thing" framing makes the
strategic move (swarm corpus) visible separately from the cosmetic
ones, so that prioritization is honest about which improvements are
load-bearing vs. nice-to-have.
