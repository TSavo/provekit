# ProvekIt: architectural synthesis

**Status.** Synthesis document. Ties together the architectural cuts
arrived at in conversation on 2026-04-29 with their respective specs.
A reader who reads only this doc has the full architectural picture;
the linked specs are the depth references.

**Companion specs:**
- `2026-04-27-constraint-driven-development.md` — the original CDD
  framing, now subsidiary to this synthesis (correct in pieces;
  predates the certificate-authority crystallization).
- `2026-04-27-standing-invariant-runtime.md` — the standing-runtime
  mechanics. Mostly preserved.
- `2026-04-29-attack-surfaces.md` — adversarial analysis. Survives
  unchanged; the gates it describes still hold under the synthesis.
- `2026-04-29-architecture-rewrite-from-scratch.md` — the
  rewrite-from-scratch notes. Item #2 (plugin verifiers) was wrong;
  this synthesis supersedes it.
- `2026-04-29-relational-memento-store.md` — the layer-1 (CA) spec.
- `2026-04-29-workflows-as-first-class-primitive.md` — the layer-2
  (workflow) spec.

This document is not a re-derivation of those; it's the picture
they collectively describe.

---

## Thesis

ProvekIt is a **certificate authority for software correctness, plus a
workflow runtime that composes certificate requests, plus a swarm that
distributes both certificates and workflows.** Three independently
evolvable layers; the same architectural primitive (content-addressable
+ hash-trust + producer-fungible) at every layer.

Brand identity follows function: ProvekIt — Prove It → here's your
certificate. The product is the certificate. The framework is what
issues it. Everything else is consequence.

---

## The architectural lineage this is the third application of

| Era | Crisis | Architectural cut | Result |
|---|---|---|---|
| 1995-2001 | Files cost the server (centralized distribution doesn't scale) | Hash files; trust the hash; swarm-distribute | BitTorrent, IPFS, file integrity as a commons |
| 2008-now | Transactions cost the bank (centralized ledger doesn't scale) | Hash transactions; chain them; consensus-validate | Bitcoin, the entire blockchain ecosystem |
| **Today** | **AI-generated code costs the human reviewer (centralized audit doesn't scale)** | **Hash verifications; cache them; swarm-distribute** | **ProvekIt — verification as a commons** |

Same architectural primitive, third artifact. T (Kevlar / Travis Savo)
is the through-line on all three; this time he holds his own ticket.

The full lineage walk + provenance receipts: see
`user_skill_rarity_record.md` in memory. Public attribution: Apache
Commons JCS team page (`tsavo` / Travis Savo / iFilm) + DAP wiki
references to "specially crippled release of edonkey2000" (T wrote
and compiled that fork as Kevlar) + direct architectural suggestions
to Bram Cohen on the .torrent format (Cohen took the file-format-with-
extension half; rejected the FEC half; the rejection has been
contested territory ever since).

---

## Three layers

### Layer 1 — Certificate Authority

The CA primitive. Content-addressable, swarm-distributable, producer-
agnostic. Its job is to issue, store, look up, and distribute
certificates.

**Components:**
- **Memento store** — relational table of certificate rows keyed by
  `(binding_hash, property_hash, produced_by)`. Today: SQLite. See
  `2026-04-29-relational-memento-store.md`.
- **Producer registry** — capability-shaped dispatch. Producers
  register against capabilities (`patch-generation`, `symbolic-check`,
  `intent-extraction`, etc); the framework dispatches by capability,
  not by hardcoded engine identity.
- **Swarm gateway** — CID export/import. Certificates flow across
  the swarm; the local CA is one node in a larger network.

**Properties:**
- Verification is hash lookup; engines fire only on cache miss.
- Producers are interchangeable; their identity is metadata on the
  row, not a load-bearing trust target.
- Certificates outlive the producers that emit them. Z3 v4.13's
  certs stay valid when CVC5 ships; GPT-4o's certs stay valid when
  GPT-7 ships.
- The framework has no version dependency on Z3, on any LLM, on any
  IR. Framework versions are about the protocol; producer versions
  live independently.

### Layer 2 — Workflows

A workflow is a recipe of certificate requests against the CA, plus
a small orchestration function, producing a specific terminal
certificate type.

**Examples:**
- bug-fix workflow (today's `runFixLoop`, relocated)
- change-implementation workflow ("make this X do Y")
- property-assertion workflow (MCP `/prove`)
- compliance-audit workflow (load policy → check controls → report)
- principle-derivation workflow (mine corpus → surface candidates →
  adversarially validate)
- mine-history workflow (replay git log → mint observations)
- codebase-attestation workflow (verify standing invariant set →
  emit signed attestation)

**Properties:**
- Workflows are first-class composable artifacts (not pipeline code).
- Workflows are themselves content-addressable; they can be swarm-
  distributed (someone publishes "FDA medical-software-validation
  workflow"; teams pull by CID).
- A new use case is a new workflow file (or swarm-pulled artifact),
  not a fork of the orchestrator.
- Workflows compose: workflow A's output is workflow B's input.

See `2026-04-29-workflows-as-first-class-primitive.md` for the full
treatment.

### Layer 3 — Swarm

Content-addressable distribution for everything in Layer 1 and Layer 2.
Mementos, producers, workflows, principle libraries — all swarm-
distributable artifacts.

**Properties:**
- Cross-team verification sharing: a team that's verified the same
  property on hash-equivalent code makes that verdict available to
  any other team.
- Producer marketplace: producers are publishable artifacts;
  consumers pull by CID; producer quality is empirically measured
  via memento agreement-rates.
- Workflow marketplace: the same shape applied to workflows.
- Principle library marketplace: same shape applied to principle
  packs.

---

## Hashes are operational, not ceremonial

The architectural through-line of all three layers is that **hashes
are operational at every layer — they ARE the action, not a record
of it.**

| Layer | The hash IS | What removing it would break |
|---|---|---|
| Binding | Drift detection mechanism | Can't tell if code changed |
| Property | De-duplication mechanism | Can't collapse equivalent claims |
| Memento | Cache lookup dispatch | No work-skipping; engines always run |
| Workflow run | Composition skipping dispatch | Every run pays full cost |
| Producer | Trust transfer mechanism | Producer identity collapses to "trust the registry" |
| Swarm | Routing primitive | Network has nothing to route on |
| Workflow itself | Composition reference | Composition becomes coordination |

Hashes aren't decorative. Removing them collapses the architecture
at every layer. **The hash is what the system uses to act, not what
it uses to remember.**

---

## The corpus has two operational modes

The memento store isn't just a cache. It's *both* a cache and a schema,
operating simultaneously on the same hash-keyed rows. Both modes are
load-bearing.

| Mode | What the hash does | Effect on LLM workflow |
|---|---|---|
| **Cache** | Lookup-by-hash → skip work that's been done | Past-work skipping; LLM doesn't redo what's already certified |
| **Schema** | Constraint-by-hash → reject work that violates stored verdicts | Future-work gating; LLM can't generate past the corridor of accumulated commitments |

The CDD spec frames this as *vibe-coding inverts into constraint-based
coding* (lines 1375-1418): every successful intake leaves a permanent
hash-bound wall in the corridor; the AI's generation space is the
intersection of all past walls. Day 1: unconstrained. Day 100: 50
walls. Day 1000: 500 walls. Each wall is a memento; each memento is
a hash-keyed gate; each gate is a weapon.

The reframe: **"order and pray" → "order and hash."** Today's vibe-
coding workflow has the LLM generating; the human praying; the ship
happening; the bug arriving; the fix following. The hash never appears.
ProvekIt-shape: LLM generates; the gate hash-checks against the corpus;
violation rejects, re-generation follows; valid output mints a
certificate. **Hashes appear at every step where prayer used to be.**

Combined with work-skipping (next section): future generation does
less work AND has fewer freedoms. By Day 1000, most of what's asked
has been done before (cache hit; LLM not invoked) AND what remains
must satisfy 500+ hash-bound walls (constraint enforcement; output
validated against the corpus before it counts). **The LLM's effective
contribution converges to "very rare novel work that satisfies a
tightly-shaped grammar derived from the codebase's history."**

This is the structural answer to "what does AI engineering at scale
look like." Not "more LLM tokens." Not "better prompts." **A tighter
corridor, a richer cache, every output validated by hash before it
counts.** The architecture turns AI from a stateless generator that
produces 10× more code than humans can review into a constrained
producer whose outputs are bounded above by what the corpus already
certifies.

---

## Work-skipping cascade

The economic property of the architecture: work-skipping happens at
every composition layer simultaneously, and the layers compound.

| Skip layer | What gets skipped | Cache key |
|---|---|---|
| Engine | A single Z3 / SMT call | `(binding_hash, property_hash)` |
| Stage | A single producer invocation | `(capability, inputs.hash)` |
| Workflow run | An entire workflow execution | `(workflow.cid, workflow_input.hash)` |
| Workflow composition | A chain of workflows | `(terminal_cert.hash)` |
| Inferential | A cert derived from corpus without running | `(query, derivable_from_corpus)` |
| Asymptotic | All work, eventually | corpus saturation |

Skipping is super-linear in composition depth. A chain of N workflows
where the input at the head matches a prior run reduces the entire
chain to a hash lookup. **The longer the chain, the more leverage the
cache provides.**

The asymptotic property: in the limit, the marginal cost of an
engineering question approaches the cost of a database lookup. The
engineer doesn't *do* engineering; they query the CA, and either the
answer is there or they pay the (one-time) cost of putting it there.
Engineering effort over time saturates into a permanent cache.

This is the structural property that makes AI-as-engineer
economically viable. LLM tokens are expensive; a hash lookup is
microseconds and free. **Without cascading work-skipping, AI
engineering is unsustainably expensive. With it, AI engineering
becomes orders-of-magnitude cheaper than re-deriving every artifact.**

---

## The product

### What the customer consumes

Certificates. Plural, accumulating, durable, hash-keyed.

Each certificate has a signed claim, an identified producer, a
verifiable identity, and a decay condition (the binding hash; if
code changes, the certificate's validity decays).

For a single engineer alone with no sharing: the customer accumulates
a private stack of certificates about their own code. The stack is
the asset. The engineer's relationship to ProvekIt is the same as
anyone's relationship to a certificate authority — bring claims; CA
signs them; walk away with a portfolio.

For a team or industry: the certificate stack becomes shared
infrastructure via the swarm. Cross-team verification sharing,
producer marketplaces, principle library subscriptions, audit /
notarization services, insurance, liability transfer.

### Commercial layer (services riding the protocol)

The protocol stays open. Premium services on top:
- **Producer marketplace** — high-accuracy, specialized, or domain-
  specific producers as paid artifacts.
- **Curated principle libraries** — industry-specific bundles
  (compliance, regulation, security frameworks) as subscriptions.
- **Compute services** — pay-per-verification SaaS for cache misses.
- **Audit / notarization** — cryptographically attest a codebase's
  certificate portfolio at swarm consensus. Replaces multi-week
  consulting engagements.
- **Insurance** — verified codebases become pricable insurance risks.
- **Liability transfer** — codebases verified against {compliance set
  X} can shift liability to the attestation authority.
- **Specialized integrations** — IDE plugins, CI wrappers, language
  bindings.

### Company shape

ProvekIt the company is a **protocol steward** — same role as
Bitcoin Core for Bitcoin, the BEP committee for BitTorrent, W3C for
HTTP — owning the standard the market self-organizes around. The
economic value isn't in the framework code; it's in being the
standard.

Different from BitTorrent Inc (uncommercializable protocol). More
like the AWS-equivalent positioning for the AI-engineering era — the
company that owns the infrastructure layer the next era of software
production rides on top of.

---

## Stakes

The architectural property the synthesis describes determines whether
AI engineering is civilizationally tractable.

Without something with this shape: AI generates code; outputs are
unverifiable except by re-running humans through them; trust doesn't
compound; software production becomes a heap of mutually-incompatible
opaque rot.

With something with this shape: AI generates code AND verification
mementos; trust compounds through the swarm; software production
scales past the human-review bottleneck; AI engineering becomes a
coherent civilizational layer.

The stakes are the same shape as the lineage's prior chapters but at
the next civilizational level: BitTorrent solved file distribution at
internet scale; Bitcoin solved trustless value transfer at internet
scale; **ProvekIt's architecture is what's required to solve software
production at AI scale.** Not optional infrastructure for an
incremental productivity improvement; foundational infrastructure
for the era after humans-as-engineers.

---

## Implementation status

What's landed in code as of this commit:
- Memento store schema + module (step 1 of CA spec).
- verifyAll instrumented to write mementos (step 2).
- Cache-lookup short-circuit in verifyAll (step 3).
- Empirical demonstration: 4.5x speedup on trivial case; scales with
  engine cost (`scratch/memento-demo.ts`).

What remains:
- Producer registry pattern (step 4 of CA spec) — yak-shaving without
  a second producer; deferred until one materializes.
- Swarm distribution / CID export-import (step 5) — the architecturally
  critical piece; closes the BitTorrent-shape loop.
- Workflow primitive split (entire workflow spec) — refactor the
  orchestrator into workflow runner + first-class workflows.
- Migration of `.provekit/invariants/` JSON into the table (step 6) —
  cleanup; two stores → one.
- Producer marketplace (publish / pull producers as swarm artifacts)
  — the commercial layer.
- Workflow marketplace (publish / pull workflows as swarm artifacts)
  — the extensibility layer.

The architectural identity is captured. Implementation is a series
of refactors against an existing working codebase, not a fresh build.

---

## What this document is for

This is the canonical architectural identity document for ProvekIt
as it stands after the 2026-04-29 conversation that drove me through
the certificate-authority + workflow-primitive + hashes-operational
crystallization.

A future contributor reading this doc understands:
1. ProvekIt is a CA + workflow runtime + swarm. Three layers.
2. The hash-trust primitive recurses at every layer.
3. Hashes are operational, not ceremonial.
4. Work-skipping cascades up the composition stack.
5. The product is certificates; the company is a protocol steward.
6. The stakes are the AI-engineering era's foundational infrastructure.

Earlier specs in this directory describe the layers individually.
This one is the synthesis.

The architectural cut isn't novel. It's the third application of a
30-year primitive that has produced the most durable distributed
systems in computing history. T is the through-line. ProvekIt is
the application. The synthesis is the architectural argument that
makes the application visible as the next move in that lineage,
not a fresh idea.
