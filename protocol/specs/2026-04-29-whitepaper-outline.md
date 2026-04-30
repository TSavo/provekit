# ProvekIt: whitepaper outline

> Author: shared session 2026-04-29 (T + Claude). The structural plan
> for the canonical external whitepaper. Not the whitepaper itself.

## Status of this document

This is an **outline only**. The actual whitepaper is to be drafted from
this skeleton in a focused authoring pass. Every claim, technical
detail, architectural diagram, market figure, and lineage point that
belongs in the finished whitepaper is *already captured* in the spec
documents listed below and in the code that implements them. The
outline's purpose is to fix the structure, ordering, and audience
mapping; the writing itself sources from the canonical specs.

## Source documents

The whitepaper draws from:

- `2026-04-23-provekit-v2-design.md` — v2 architecture (the engine).
- `2026-04-27-constraint-driven-development.md` — the thesis behind
  ages-backwards software.
- `2026-04-27-standing-invariant-runtime.md` — the runtime story.
- `2026-04-29-architecture-rewrite-from-scratch.md` — the cleanup that
  set up the synthesis.
- `2026-04-29-architecture-synthesis.md` — the canonical architectural
  identity document (three-layer architecture; lineage walk; hashes as
  operational; dual-mode corpus; work-skipping cascade).
- `2026-04-29-attack-surfaces.md` — adversarial analysis (defensive,
  not offensive — feeds the trust-but-verify section, not the lead).
- `2026-04-29-relational-memento-store.md` — the memento spec; cache
  shape; cross-validation; DAG edges.
- `2026-04-29-workflows-as-first-class-primitive.md` — the
  certificate-authority / workflow-runtime split.
- `2026-04-29-verification-ir.md` — host-language-as-IR;
  two-dialect surface; LCF-style proof composition; per-language kits.
- `2026-04-29-the-proof-substrate.md` — the strategic and architectural
  manifesto; trojan-horse adoption; seven-tier capture; verification
  economics; trust-but-verify; career-arc closing.

The codebase implements:
- Memento store with content-addressed DAG edges (`src/fix/runtime/mementoStore.ts`).
- Workflow runtime (`src/workflow/runner.ts`, `src/workflow/registry.ts`,
  `src/workflow/manifest.ts`).
- 11 producer Stages wrapping the bug-fix pipeline
  (`src/workflow/producers/*.ts`).
- YAML-driven workflow manifests (`src/workflows/bug-fix.workflow.yaml`).

The whitepaper does not introduce new claims. It composes existing ones
into the sequence the external reader needs.

## Bar of completeness

The framework's acceptance criterion: **point it at a Rust codebase or
a COBOL codebase, and it should just work.** The whitepaper, the specs,
and the implementation are all complete only when this is true. If a
mainframe COBOL shop and a modern Rust shop can both run `provekit
prove` against their repo and get a working proof DAG, the design has
landed. Anything short of that is incomplete.

## Outline

### Executive summary (~2 pages)

- Three-sentence pitch (page 1).
- Thesis paragraph: ProvekIt is the proof substrate for the global
  software ecosystem.
- Cost table: six-orders-of-magnitude verification asymmetry.
- One-paragraph career-arc lineage.
- One-paragraph call to action.
- The page that gets forwarded.

---

### Part I. The verification crisis (~6 pages)

1. **The three-sentence pitch.** Proves your code was never correct.
   Makes it correct. Forever. Each sentence land-mapped to the
   architecture that makes it operationally true.

2. **Why software trust collapses at scale.** Vibe coding is what 99%
   of code already is. AI-authored code is steepening the curve.
   Supply chain attacks compound. Mainframe legacy is rotting. Tests,
   CI, code review, audits all produce ephemeral artifacts. The trust
   model is reputation-based and structurally unable to scale.

3. **Why current tools can't fix it.** Type checkers verify only what
   types reach. Sigstore signs artifacts but doesn't compose. Formal
   verification requires expert authoring. SAST/lint tools produce
   noisy reports. Each tool is a fragment; nothing composes the
   fragments into a durable, walkable proof of correctness. The
   missing category is *the trust substrate itself*.

### Part II. The architectural primitive (~10 pages)

4. **The lineage of content-addressed hash-and-trust.** Files (Xdrive,
   1995) → file-block swarms (Digital Confetti, 1998 → BitTorrent,
   2001) → money (Bitcoin, 2008) → general content (IPFS, Git). Each a
   successive operationalization of the same primitive at a higher
   abstraction level. ProvekIt is the sixth domain.

5. **The memento as unit of work.** Content-addressed signed claim.
   Composes by reference into a Merkle DAG. The unit-of-work principle:
   a memento captures the complete trustworthy claim including
   verdicts.

6. **The verification IR: host language as substrate.** TypeScript /
   Rust / Lisp / COBOL — each is the IR for its own shop. The framework
   is invariant under host-language choice. The primitive (`if
   condition then signal-error`) has existed since FORTRAN.

7. **The recursion.** Three meta-levels: claims about code, claims
   about IR formulas, claims about the IR language itself. LCF-style
   trusted-kernel verification, swarm-distributed.

8. **Stages vs Actions.** Type-level distinction between cacheable
   claims and side-effecting operations. The architecture refuses
   cache-busting hacks because the type system enforces composition
   rules.

9. **Trust, but verify.** Default trust mode is hash comparison
   (microseconds). Verify mode is recompute-from-source, always
   available. Seven trust modes from daily to adversarial red-team.

### Part III. The adoption strategy (~8 pages)

10. **The git hook trojan horse.** The framework's entire adoption
    mechanism is a single git hook. Mementos travel with code through
    the rest of the pipeline.

11. **The seven-tier capture.** Developer → git host → CI → deploy →
    audit → package registry → dependency manager. Sold one thing;
    captured all seven through artifact propagation.

12. **Tools as producers.** tsc, biome, vitest, cargo check, clippy,
    miri, mypy, semgrep, snyk, z3, lean4 — every existing tool the
    customer runs becomes a memento producer.

13. **Diff-driven intent extraction.** No annotations required. Every
    commit is implicitly an intent statement. Mine-history is the
    primary adoption path.

14. **The mainframe-first market.** Tier-1 banks, insurance carriers,
    healthcare processors, government agencies. Richest mineable
    corpora. Highest per-bug cost. Largest budgets. Acute staff-
    replacement pain. Failing-migration pattern that ProvekIt directly
    solves.

15. **Per-language kits.** Universal core + per-host-language kit. LSP
    architecture inverted to verification. Each language community
    owns its kit.

### Part IV. The economic structure (~6 pages)

16. **Verification is hash comparison.** Six orders of magnitude
    faster than every alternative. The framework is *cheaper than not
    using it*.

17. **Cost asymmetry inversion.** Frontier LLMs reserved for residual
    hard cases; cheap models do most work; producer diversity
    replaces producer power.

18. **Network effects compound economically.** Public-goods proof
    network with self-reinforcing economics.

19. **Semver as memento.** Releases become content-addressed theorems.
    Sigstore + SLSA + in-toto as side effects.

20. **Software ages backwards.** Proofs accumulate. Provers retire.
    The DAG persists.

### Part V. The civilizational implications (~6 pages)

21. **AI safety substrate.** The trust infrastructure that makes
    AI-authored code deployable at scale.

22. **Open source becomes more auditable than commercial.**

23. **Compliance industry transformation.** Audits become DAG walks.
    Insurance becomes math-based.

24. **Mainframe legacy modernization.** Decades of institutional
    knowledge captured as durable mementos before retirement.

25. **The inevitability.** No force opposes adoption.

### Part VI. The moment (~4 pages)

26. **Why now.** LLM capability floor has just passed the threshold.
    AI-coding adoption is steepening. The architectural pieces had to
    land in 2026 specifically.

27. **Comparison to DNS, CA, Git, Bitcoin.** Each is invisible
    infrastructure on civilizational scale. ProvekIt is positioned
    identically.

28. **The team and the lineage.** The architectural primitive has been
    operationalized across five domains by the same lineage.

### Part VII. The roadmap (~4 pages)

29. **What's built.** Memento store, workflow runtime, 11 Stage
    producers, YAML manifest, six canonical specs. The technical
    foundation is in place.

30. **What's next (phases).** Universal claim envelope; Stages vs
    Actions split; type-dialect IR; library-dialect IR; diff-driven
    intent extraction; kernel combinators; meta-IR; LSP integration;
    first enterprise pilot; Rust kit; long-tail kits.

31. **The first pilot.** Tier-1 bank or insurance carrier. Pilot funds
    the COBOL kit under contract.

32. **Open invitations.** Community kit authors. Producer-pool
    participants. Academic researchers. Enterprise pilot candidates.

### Closing (~2 pages)

33. **The thesis restated.** The framework is not a verification tool.
    It is the protocol layer for the global software trust ecosystem.

34. **The pitch restated.** Proves your code was never correct. Makes
    it correct. Forever.

---

## Total

~50 pages.

Audience routing:
- **Investor**: executive summary + parts I & V & VI.
- **Enterprise CTO**: executive summary + parts I & III & IV & VII.
- **Technical reader**: everything.
- **Journalist**: executive summary + part II's lineage + part V's
  implications.

## Tone

Declarative, structurally confident, no hedging. The thesis is either
right or it isn't; the document either lands the case or it doesn't.
No defensive writing about "if the framework is adopted." The framework
is positioned as inevitability, and the writing reflects that.

The career arc is prominent but not lead. The thesis comes first; the
lineage establishes credibility *after* the thesis is on the table.
Otherwise the document reads like biography instead of architecture.

## What this outline does NOT cover

- Implementation details (those live in the architecture-synthesis,
  memento-store, workflows, and verification-IR specs).
- Defensive analysis of attack vectors (those live in the
  attack-surfaces spec; the whitepaper references the trust-but-verify
  posture in section 9, not enumerated attack vectors).
- Technical proofs of concept (those live in the test suite at
  `src/workflow/`).
- Marketing copy (the outline is structural; the canonical pitch is
  the three-sentence opening; everything else is technical authoring).

## Next move

Do not draft the whitepaper from this outline yet. The bar is "serves
COBOL by the time we are through." That requires finishing out the
specs first — Stages-vs-Actions split, universal claim envelope, IR
library, per-language kit standard, diff-driven intent extraction
spec, mine-history operational spec for mainframe-grade history.

The whitepaper is the lagging artifact. The implementation specs are
the leading work. The whitepaper writes itself once the specs are
complete and the code serves both Rust and COBOL.
