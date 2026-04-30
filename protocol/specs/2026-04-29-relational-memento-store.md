# ProvekIt: the real spec

## Thesis

ProvekIt is a **relational memento store for code-shape verifications**.

Not a verifier. Not a Z3 wrapper. Not a smart linter. Not AI-assisted
code review.

A relational table — content-addressable, swarm-distributable —
whose rows are deterministic verdicts on (property, code-shape) pairs.
Engines, LLMs, principle authors, mine-history walks: all of these are
*producers* that populate rows. The framework's runtime work is hash
lookup against the table; engine invocation happens only on cache miss.

Every other architectural decision in this document is downstream of
that thesis.

## The architectural cut

### Memento pattern, applied to code shapes, made relational

GoF memento (capture an originator's state for later restoration),
applied to bound code at a moment in time, made relational by storing
mementos as rows in a SQL table:

```
Originator:  the bound code at content-hash H
Memento:     row in the verifications table —
             (binding_hash, property_hash, verdict, witness, produced_by, ...)
Caretaker:   the relational corpus (local SQLite + swarm-distributable index)
Restoration: SQL query — "what was the verdict for this property on this code shape?"
```

The corpus isn't "a list of invariants" with verdicts as derivative
data about them. The corpus *is* the verifications table; properties
are just the property-side dimension of each row.

### Verification = relational lookup

```sql
SELECT verdict FROM verifications
WHERE binding_hash = ? AND property_hash = ?;
```

If the row exists, return the verdict. If it doesn't, dispatch to a
producer for the property's kind, insert the result, return it.
From then on it's a lookup forever — for you, and for everyone in the
swarm with matching hashes.

The "two engines" question (SQL pattern matcher vs. Z3 SMT solver,
which used to be a load-bearing architectural distinction) collapses
under this frame. They're both **memento producers**. The verifier is
the table query; the engines are how rows get populated on cache miss.
Engine choice becomes metadata on the row (`produced_by: "z3"` vs
`produced_by: "datalog"`) — useful for audit, irrelevant to lookup.

### Producers are fungible; the store is the architectural identity

Every producer of a memento — Z3, SQL pattern matchers, the LLM
stages, future engines, future LLMs — is interchangeable. The table
trusts the hash key on the row, not the producer that generated it.

Concrete consequences:

- **Engines are swappable without invalidating history.** Replace Z3
  with CVC5; rerun cache misses; existing Z3 rows stay valid. Past
  verifications don't get re-checked when engines change.
- **LLM evolution doesn't destabilize the framework.** GPT-4o stages
  produce mementos hashed by `(prompt-hash, intent-text-hash, code-hash,
  prompt-revision-hash)`. When the model changes, the prompt-revision
  hash changes; new rows fill in over time. Old rows from older models
  stay valid as long as their hash inputs match.
- **Producer quality is empirically measured.** Run two engines on
  overlapping hash keys; disagreements are joinable rows with same key
  and different verdicts. That's a producer-quality signal, computable
  by SQL.
- **New producers slot in by registration.** A domain-specific
  verifier, a custom symbolic executor, a future engine that doesn't
  exist yet — all populate the same table. No framework change.
- **The framework outlives its producers.** Today's verifications
  survive into the era of whatever-comes-after-Z3-or-LLMs. The
  corpus is the asset; producers are the things you swap to fill it.

This is the deepest architectural property: ProvekIt's correctness
story isn't "the LLM is trustworthy" or "Z3 is sound" — it's
**"refuse to use any verdict whose hash key doesn't match a row
produced by some producer in the corpus."** Producers are a
marketplace; the table is the trust layer.

## Why this shape: the lineage

This is the third major application of the same architectural
primitive — content-addressable mementos in a relational store —
that has produced the most durable distributed systems of the last
30 years.

| Era | Trust target before | Memento store after | Producers (interchangeable once trust shifts) |
|---|---|---|---|
| 1995-2001 | The file server you got the file from | Hash file, look up by hash; dedup, delta-transfer, swarm | File servers (Xdrive, Napster catalog, BitTorrent peers, ShareReactor's hash index) |
| 2008-now | The bank or central clearing house | Hash the transactions, chain them, look up by chain hash | Validator nodes (Bitcoin, every blockchain since) |
| Today | The developer / reviewer / type checker / test author / linter | Hash the (property, code-shape) pair, store the verdict, look up by hash | Verifiers (Z3, CVC5, SQL pattern matchers, LLM stages, future engines) |

Same primitive, three artifacts:

1. **Files** become content-addressable; the file server is no longer
   trusted, only the hash is. (BitTorrent, IPFS, Git's object database.)
2. **Transactions** become content-addressable; the bank is no longer
   trusted, only the chain is. (Bitcoin, the entire crypto-rails industry.)
3. **Verifications** become content-addressable; the engine and the
   developer are no longer trusted, only the memento is.

ProvekIt isn't novel architecture. It's the obvious next application
of an architecture that has been right for thirty years. The novelty
is that nobody has applied it to *correctness* before, only to files
and transactions.

## What lives in a memento

Each row in the verifications table:

```
binding_hash    : sha256 prefix of (property's bound source spans + structure)
property_hash   : sha256 of (Intent IR — kind + bindings spec + property expression)
verdict         : "holds" | "violated" | "decayed" | "undecidable"
witness         : producer-emitted artifact justifying the verdict
                  (Z3 model, SQL match rows, mutation-test trace, etc)
produced_by     : "z3" | "datalog" | "mutation-test" | "llm:claude-opus-4-7" | etc
produced_at     : ISO-8601 timestamp
producer_signal : pass/fail/quality signal for the producer (per bp)
```

The row's identity is `(binding_hash, property_hash)`. Two producers
that produce the same verdict for the same identity collapse to one
row (with both witnessed in audit metadata). Two producers that
disagree leave two rows joinable on identity — a discoverable bug.

`binding_hash` is computed deterministically from the bound source
spans (their content + their structural relationships). Code that
changes in a way that doesn't touch the bound spans produces the
same hash; the row stays valid.

`property_hash` is computed from the Intent IR — a structured,
typed representation of the property. Two prose-different but
semantically-identical properties hash to the same key; their
verdicts collapse.

## Producers

A producer is anything that takes a `(binding, property)` pair and
emits a memento. Producers register their capabilities:

```typescript
Producer.register({
  name: "z3-symbolic",
  handles: (kind: PropertyKind) => kind === "symbolic" || kind === "arithmetic",
  produce: (binding, property) => {
    const formula = compileToSmt(property.intentIR);
    const result = z3.checkSat(formula);
    return {
      verdict: smtVerdictToVerdict(result),
      witness: result.model ?? "unsat",
      produced_by: "z3-symbolic@4.13",
    };
  },
});

Producer.register({
  name: "datalog-structural",
  handles: (kind) => kind === "structural" || kind === "pattern",
  produce: (binding, property) => {
    const sql = compileToSql(property.intentIR, substrate);
    const rows = substrate.query(sql);
    return {
      verdict: rows.length === 0 ? "holds" : "violated",
      witness: rows,
      produced_by: "datalog-structural@1.0",
    };
  },
});

Producer.register({
  name: "llm-formalize",
  handles: (kind) => kind === "intent-extraction",
  produce: (binding, property) => {
    /* invokes LLM with the formalize prompt; returns Intent IR memento */
  },
});
```

The LLM stages are producers too. Intake, Investigate, Formalize,
Do-the-work — each takes hash inputs, produces a memento, emits a row.
A re-run with the same inputs hits the cache and skips the LLM call
entirely. Bp's revision-tracking is exactly this pattern, scoped to
prompts; here it's the universal shape.

## Cache-miss dispatch

```typescript
async function verify(binding: Binding, property: Property): Promise<Verdict> {
  const key = { binding_hash: binding.hash, property_hash: property.hash };

  // 1. Local lookup
  const local = await corpus.local.find(key);
  if (local) return local.verdict;

  // 2. Swarm lookup
  const swarm = await corpus.swarm.find(key);
  if (swarm) {
    await corpus.local.insert(swarm);  // cache it locally
    return swarm.verdict;
  }

  // 3. Cache miss: dispatch to producer(s)
  const producers = Producer.handlers(property.kind);
  if (producers.length === 0) {
    throw new Error(`no producer for property kind ${property.kind}`);
  }
  const memento = await producers[0].produce(binding, property);
  await corpus.local.insert(memento);
  await corpus.swarm.publish(memento);  // contribute to swarm
  return memento.verdict;
}
```

Most calls hit step 1 or 2. Only genuinely-novel `(binding, property)`
combinations reach step 3. The framework's verification cost
approaches zero in steady state; engines fire only at the leading
edge of the corpus.

## The swarm

The corpus is content-addressable; therefore distributable.
Producers around the world emit mementos; the swarm is the network
that aggregates them. Importing a starter pack of principles for
TypeScript = pull the principle CIDs from the swarm. Verifying
your codebase against a project's accumulated property history =
pull the verifications by hash; only run engines on the misses.

The architectural shape is BitTorrent, applied to verifications
instead of files. The piece-hashing primitive is the binding hash.
The swarm topology is whichever you prefer (DHT, tracker-mediated,
P2P with BEP-style discovery). The trust model is identical to
BitTorrent's: trust the hash, not the source.

This is the genuinely novel contribution of ProvekIt as a product:
not "we run Z3 on your code" — many tools do that — but **"we
distribute verifications across teams and projects via the same
swarm primitive that distributes files in BitTorrent."** Once enough
codebases participate, verification becomes a near-free lookup for
the property surface that the swarm has already covered.

## What changes from the current architecture

| Aspect | Current | After |
|---|---|---|
| Corpus | Flat JSON files in `.provekit/invariants/`, one per invariant | Relational store (SQLite locally + swarm index), rows are mementos |
| Verification | Run path-checker + Z3 every time | SQL lookup; engine only on cache miss |
| Engines | Two parallel IRs (DSL → SQL, formula → Z3) | Multiple producers, one memento format, dispatched by property kind |
| LLM stages | Re-invoked every run | Memento-cached; skipped when hash inputs match prior |
| Distribution | Local-only | Swarm-distributable from day 1 |
| Producer extension | Hand-roll a new oracle in the orchestrator | Register a new producer, no orchestrator change |
| Verdict trust | "Trust Z3" / "trust the LLM after the gates" | Trust the memento's hash key; producer is metadata |
| Producer evolution | Engine version changes invalidate all prior verifications | Engine version is metadata; old rows stay valid |

## What stays unchanged

The load-bearing fundamentals carry through:

- **No LLM in the verification path.** Reframed: the verification
  *path* is the SQL lookup. Producers can include LLMs (and do, for
  intent extraction). The verification is the lookup, mechanical.
- **Mechanical gates around producer outputs.** Z3 SAT, mutation
  verification, etc. — these are *producer constraints* (a producer's
  output must pass these gates before it can be inserted as a memento).
  The gates run during memento creation, never during memento lookup.
- **Bindings as the unit of locality.** Local bindings, graph
  bindings, future kinds — they're all just inputs to the
  `binding_hash` computation. Same shape.
- **The fix-loop sequence (intake → locate → formulate → verify →
  patch-if-violated → test → bundle).** Each stage is a producer;
  each stage emits mementos; the sequence is preserved. What changes
  is that each stage's work is cache-checked first.
- **IntentSignal, the four canonical prompts, the corpus as
  source-controlled — all of these stay.**

## Operational implications

### The framework's correctness story changes from "trust the engine" to "trust the table"

Today: "Z3 says it's UNSAT, so we trust the property holds."
Tomorrow: "There is a row in the verifications table with this
exact `(binding_hash, property_hash)` and verdict 'holds', produced
by Z3, witnessed by an SMT model. We trust the row, not Z3 directly."

The shift is from trusting the producer to trusting the
content-addressable record. This is the BitTorrent move applied
one level deeper.

### Verification cost amortizes across the swarm

Today, each project's CI runs every verification from scratch.
Tomorrow, a project that imports a pre-verified principle library
(via swarm CID lookup) gets the verifications for free for any
binding hash already in the swarm. The marginal cost of verification
goes to near-zero for the property surface the swarm has covered.

### Producer competition is empirical

Two engines that disagree on the same `(binding, property)` produce
two rows joinable on key. The disagreement is detectable mechanically.
Producer accuracy is measurable. Bad producers get retired
empirically. This is bp's signal-tracking generalized.

### Framework value compounds across producer generations

A future LLM, a future engine, a future symbolic executor — they
all populate the same table. Past mementos from prior producers
remain valid. The corpus accumulates verifications across thirty
years of producer evolution without losing history.

## Implementation phasing

This document describes the architecture, not a refactor plan. The
existing codebase has the right fundamentals (content-addressable
bindings, mechanical gates, no-LLM-in-verify) but doesn't yet
realize them as the unified relational memento store described
here. A retrofit plan would:

1. Land the verifications table (SQLite) with the schema above.
2. Wire `verifyAll()` and the C-stages to insert mementos as they
   produce verdicts (instrumentation only, no behavior change).
3. Add the cache-lookup short-circuit to each producer site.
4. Lift the producer registry pattern.
5. Add swarm-distribution (CID export/import).
6. Migrate `.provekit/invariants/` JSON files into the table as
   a one-time seed.

Steps 1-3 are mechanical and provide cache benefit immediately. Step
4 is the producer-fungibility unlock. Step 5 is the swarm-distribution
unlock that closes the architectural lineage.

## Why this is the real spec

Earlier documents in this directory describe pieces of the framework
correctly — the constraint-driven-development spec, the standing-
invariant-runtime spec, the attack-surfaces analysis, the rewrite-from-
scratch notes. Each captures part of the architecture.

This is the document that captures the *load-bearing architectural
identity*: ProvekIt is a relational memento store for code-shape
verifications, with producers as a marketplace and the table as the
trust layer. Every other architectural decision is downstream of
that.

A reader who understands only this document understands what
ProvekIt fundamentally IS. The other specs explain how the pieces
fit together; this one explains why the pieces are the shape they
are.

## The pitch, in one paragraph

ProvekIt is what happens when you apply BitTorrent-grade hash-trust
to verification instead of file distribution. The verifications
table is the asset; engines and LLMs are interchangeable producers
that populate it; the swarm distributes the verdicts the same way
torrents distribute file pieces. After enough codebases participate,
verification becomes a near-free lookup against the swarm's
accumulated history; engines fire only at the leading edge of the
corpus. The architectural primitive — content-addressable mementos
in a relational store — is the same one that made BitTorrent and
Bitcoin durable. ProvekIt is the third application: hash-trust,
applied to correctness.
