# Bug Zoo — the bug-class taxonomy (design thinking, recorded)

> Recorded 2026-05-27 (T + Claude). The mistake we kept making was not writing
> the thinking down. This is the *why* behind which species the zoo should hold.

## Organizing principle

A bug is a **missing or false composition edge**: `post(producer) ⊬ pre(consumer)`.
So a **bug class is not a category — it is a KIND OF OBLIGATION a value carries
across a boundary**, i.e. an *axis* along which a value-at-a-composition-edge can
be constrained. Enumerate the axes and you have enumerated the classes.

Each class is **portable**: the same missing-edge shape recurs in every language.
That portability is the whole reason a *zoo* exists — the substrate claim is that
bug classes are universal, so one proof shape (the lifted contract + its edge)
covers them all.

## The obligation families (the carving that holds at the joints)

Seven families, each constraining the value-at-the-boundary along one dimension:

1. **Data** — the value's own properties (the "what is it" axis). Members:
   - Presence (optional/null vs required): `maybe_null → non_null`
   - Value domain (range/sign/enum membership): off-by-one, bounds, bad enum.
     *Includes arithmetic* (overflow = result ∉ type domain; div-by-zero = divisor ∉ {x≠0}) —
     a named instance, distinctive for machine width/precision, not a separate family.
   - Type / shape conformance (structure matches expected schema): parse, deserialize, cast, dynamic-shape.
2. **Resource** — the value is a non-copyable resource (the "who/when may hold it" axis; linear/affine). Members:
   - Aliasing & ownership (borrow/move, shared-mutable, use-after-move).
   - Lifetime (acquire/release, use-after-free, leak).
3. **Temporal** — operations over time. Members:
   - Ordering / protocol / typestate (sequential: init-before-use, lock-before-access).
   - Concurrency (parallel: data races, happens-before).
4. **Effect** — what *using* the value does (the "what happens" axis): purity vs hidden IO/throw/mutation/async (async contagion, unchecked exceptions); idempotence.
5. **Flow** — where the value may travel (taint, bidirectional): integrity (untrusted → trusted sink = injection: SQLi/XSS/path) and confidentiality (secret → untrusted sink = leak).
6. **Translation** — meaning preserved across representations (the federation axis): cross-language equivalence, encoding round-trip/canonical form (`encode∘decode = id`), version drift.
7. **Computation-property** — does the computation itself behave:
   - **Determinism / reproducibility** — same input → same output; no clock/address/iteration-order nondeterminism. *The most ProvekIt-native obligation*: content-addressing is meaningless without it; "one source lifted to two CIDs" is the canonical substrate bug.
   - Termination / progress (liveness).
   - (Authorization / capability — the caller holds the right — sits here or as a Data precondition.)

## "Ten bug classes" = the ten highest-value portable members

A curated subset across the families, biased toward what's portable and what the
substrate cares about most. Candidate ten:
1. Presence (Data)
2. Value domain incl. arithmetic (Data)
3. Type/shape conformance (Data)
4. Aliasing & ownership (Resource)
5. Resource lifetime (Resource)
6. Ordering / protocol / typestate (Temporal)
7. Concurrency / data-race (Temporal)
8. Effect & purity (Effect)
9. Information flow / taint — injection + leakage (Flow)
10. Cross-representation equivalence + round-trip (Translation)
   — with **Determinism / reproducibility (Computation-property)** as the eleventh that
   arguably outranks several above, because the whole CID-addressed substrate rests on it.

## Current coverage (5 species) and the build list

| Class | Species | Status |
|-------|---------|--------|
| Presence (Data) | BZ-SHAPE-005 null-boundary-equivalence | DONE |
| Value domain (Data) | BZ-SHAPE-006 value-scope-escape | DONE |
| Aliasing (Resource) | BZ-OWNERSHIP-001 borrowed-pages-as-scratch | DONE |
| Equivalence (Translation) | BZ-COMPOSITION-001 cross-language-equivalence | DONE |
| (polyglot-link *mechanism*) | BZ-SHAPE-007 polyglot-link-obligation | DONE (the cross-kit edge itself, instantiates Translation) |

**Uncovered (the build list — each uncovered family/member is the next species):**
- Type/shape conformance (Data) — parse/deserialize/cast
- Resource lifetime (Resource) — acquire/release, use-after-free, leak
- Ordering/protocol (Temporal) — typestate
- Concurrency (Temporal) — data race
- Effect & purity (Effect) — async contagion, unchecked throw
- Flow / taint (Flow) — injection + leakage
- **Determinism / reproducibility (Computation-property)** — nondeterministic lift/mint

The `BZ-SHAPE-001..004` numbering gap and the single-entry kingdoms are the
fossil record of a larger original design; the survivors are the five above.

## The goal: a (species × domains) matrix

The zoo's purpose is to **exhibit every portable bug-class species across ≥2 domains**
(language surfaces / kits). "More than one domain" does three jobs:

1. **Demonstration** — the same missing-edge shape recurring across languages is what
   makes it a *class* (universal), not a per-language quirk. (SHAPE-005 in Java
   `@NotNull`/Spring + TS zod/class-validator + C# DataAnnotations/LINQ.)
2. **Admission filter** — a class EARNS a species only if exhibitable across ≥2 domains.
   Single-language-only = a quirk, not a substrate-level bug class. Multi-domain is the
   criterion for being a class at all.
3. **Proof, not assertion** — the exhibits must **lift to the same obligation/edge**
   (CID-equivalent), so portability is *proven* by the lifted contracts coinciding. And
   that coincidence IS bug class #10 (Translation / cross-representation equivalence) —
   so the zoo's master claim ("bug classes are portable") is itself one of its own
   species. The zoo proves portability using the portability obligation. (Load-bearing.)

Corollary: strength is **monotonic in domains** — 2 domains is the minimum to claim
"portable"; each additional language that lifts to the *same* edge strengthens the
universality claim.

"Complete zoo" = every portable class (the taxonomy above) × ≥2 domains, each
domain-pair lifting to the byte-identical edge. The surviving 5 species each already
span ≥2 domains; the build list is the uncovered axes, each needing its ≥2-domain set.

## The canonical ten (locked) + the meta-class

Ten obligation-axis bug classes, plus ONE meta-class that is the proof mechanism
(not a competitor for a slot):

| # | Class | Family | Status |
|---|-------|--------|--------|
| 1 | Presence (optional/null → required) | Data | DONE BZ-SHAPE-005 |
| 2 | Value domain (range/sign/enum; incl. arithmetic overflow/div-zero) | Data | DONE BZ-SHAPE-006 |
| 3 | Shape / parse (dynamic structure conformance: deserialize/cast/schema) | Data | gap |
| 4 | Aliasing & ownership (borrow/move/shared-mutable) | Resource | DONE BZ-OWNERSHIP-001 |
| 5 | Resource lifetime (acquire/release, use-after-free, leak) | Resource | gap |
| 6 | Ordering / protocol / typestate (sequence, init-before-use, lock-before-access) | Temporal | gap |
| 7 | Concurrency / data-race (happens-before, interleaving) | Temporal | gap |
| 8 | Effect & purity (declared vs actual IO/throw/async/mutation; async contagion) | Effect | gap |
| 9 | Information flow / taint (injection: untrusted→trusted; leakage: secret→untrusted) | Flow | gap |
| 10 | Determinism / reproducibility (same input→same output; no clock/addr/order nondeterminism) | Computation-property | gap |

**Meta-class — Translation / equivalence.** NOT class #11. It is the obligation that
the SAME class lifts to the SAME edge across domains — the proof axis the whole matrix
runs on (encode/decode round-trip and version-drift live here too). BZ-COMPOSITION-001
demonstrates it standalone; BZ-SHAPE-007 polyglot-link is its FFI MECHANISM, not a sixth
ordinary class. Every species' cross-domain edge-CID coincidence IS this meta-class at work.

Coverage: 3/10 classes built + meta-class proven. Build list (7): shape/parse, lifetime,
ordering, concurrency, effect, taint, determinism. (Termination + authorization noted but
cut from the ten: liveness is rarer; authorization folds into Presence/Value as a capability
precondition.)

## Species template (anatomy)

A species IS a named missing edge, exhibited across >=2 domains in four states:

```
BZ-<CLASS>-NNN-<slug>/
  README.md          - the class + the edge in ProofIR (e.g. maybe_null(x) |-/- non_null(x))
  <domain-A>/        # >=2 domains required (rust/, java/, typescript/, ...)
    lab/             - host-language tooling ONLY; bug ships GREEN under ordinary checks (real + uncaught)
    exhibit/         - native contract surface added; ProvekIt lifts -> reports the MISSING edge (RED)
    fixed/           - source closes the edge; ProvekIt re-lifts -> edge DISCHARGES (GREEN)
    refused/         - malformed/over-claimed input; ProvekIt correctly REFUSES (safety/Oracle: never false-greens)
  <domain-B>/        # same four states, second language/surface
  expected/          - PINNED receipts: the lifted edge's ProofIR CID, byte-IDENTICAL across domains
```

Two load-bearing invariants:
- **The four states are the truth table of the edge:** lab (invisible to the language) ->
  exhibit (ProvekIt sees the missing edge) -> fixed (edge discharges) -> refused (won't fake
  green). lab + refused keep it honest: without lab, "ProvekIt caught it" is unfalsifiable;
  without refused, green is cheap.
- **The cross-domain edge CID must coincide** - that pinned identity in expected/ IS the
  meta-class (Translation) operating inside every species. A species is not "done" until
  >=2 domains lift to the SAME edge CID.

So the matrix is precise: ROWS = the ten classes, COLUMNS = domains, each CELL = a
{lab, exhibit, fixed, refused} quad, ROW-INVARIANT = one identical edge CID across columns.

## Build order (suggested)

Determinism (10) first — most ProvekIt-native (content-addressing rests on it; we trip its
checks constantly), then effect/purity (8), taint (9), lifetime (5), concurrency (7),
ordering (6), shape/parse (3). Each new species: name the edge, pick >=2 domains, build the
four states, pin the identical cross-domain edge CID.

## THE ADMISSION CRITERION (primary — read before everything above)

A bug earns a zoo species ONLY if it is **a bug for which a contract/unit-test was
genuinely written, the test PASSED (green), the bug was still present, and it became
visible only after lifting to ProofIR.** "A bug ProvekIt catches" is not enough — a bug
a unit test would have caught adds nothing. The zoo is *only* the bugs that hide where
tests structurally cannot look.

**Why a written test passes while the bug lives (the mechanism = the whole model):**
a unit test is **local** — it checks one unit at sample points. The bug is not in a unit;
it is in the **composition edge** `post(producer) ⊬ pre(consumer)` — the SEAM between
units. Each side passes its own test (producer produces what it produces; consumer handles
what it's given); neither can see the seam **by construction**, because the edge only
exists when you compose them, and no per-unit test composes. Lifting both contracts to
ProofIR and composing makes that edge a first-class checkable object — so the missing
implication finally shows.

This IS the value proposition, stated honestly: **ProvekIt catches the seam bugs that pass
every local test.** (cf. tonight's spine: implication is the composition operator; a bug is
a missing edge; tests cover points, lifting covers the universal edge; the bug lives in the
gap between point-coverage and the universal.)

Consequences:
- It sharpens `lab`: it must carry **passing tests**, not merely "compiles green." The
  good-faith test that misses the bug is the point — without a green test present,
  "ProvekIt found it" is unfalsifiable theater.
- It sharpens `exhibit`: it is the **lift-and-compose** that surfaces the seam edge the
  passing test could not reach.
- It is the **admission filter above the (species × domains) matrix**: a candidate that a
  unit test would catch is rejected, no matter how real the bug.

## Methodology: white-room construction (why the zoo is falsifiable)

Species are **constructed in a white room**, not harvested from real codebases: you author
BOTH the bug AND the genuine tests that fail to observe it, in a minimal self-contained
specimen. This is not a convenience — it is what makes the claim **falsifiable**.

Every real-world bug corpus (Defects4J et al.) dies on the unanswerable counterfactual:
"would a test have caught it?" White-room construction kills the ambiguity — you EXHIBIT the
actual test, green, with the bug present. The specimen IS the proof. "Tests of this class
miss this bug" stops being an assertion and becomes a thing you can run.

So a species is a **hand-built minimal counterexample to 'tests catch bugs'**: a
`(genuine test passes, bug present, lift sees the seam)` triple, in a clean room with
nothing else in it.

**The one discipline that keeps it honest — non-negotiable:** the `lab` test must be the
**IDIOMATIC** test — the one a competent engineer actually writes — NOT a strawman. If the
test is deliberately weak, the specimen proves nothing. The force comes from the test being
the *natural* one whose blindness to the seam is **structural, not contrived**. (Determinism:
`parse(serialize(x)) == x` is THE test anyone writes for a serializer, and it is blind to
byte-order by construction.) Gold standard for picking a species: choose the bug whose
idiomatic test is *architecturally incapable* of seeing the edge.
