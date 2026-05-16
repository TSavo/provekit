# Trinity composition test census: every producer-consumer seam in the 7-step algebra

**Date:** 2026-05-16.
**Parent:** #1024 Trinity exhibit, #1068 Path B.
**Status:** Architect census, awaiting dispatch as a single comprehensive composition-test PR.

## Why this exists

The pattern this session has produced, in plain English: every time the substrate work specs a producer's output shape, the producer ships individually-correct with unit tests, then composition surfaces a consumer-side integration gap, then the gap becomes a new prereq. Same shape as A7 (`LowerKit::claim_spec_value` not descending through A3's `Term::Op` wrapper), and #1043 (LowerKit shipped without parallel-path deletion), and #1044 (verb-selector shipped without verb-field updates in PathAlgebra consumers).

The meta-correction: spec discipline must require composition tests across every producer-consumer seam, not just unit tests of the producer. The producer's acceptance criteria must include "every consumer of this producer's output continues to work end-to-end."

This census is the upfront walk of the 7-step Trinity algebra. For each seam (the boundary where one step's output is consumed by the next step's input), it names the producer, the consumer, the composition assertion, and whether the seam has a known gap, an open architectural question, or is empirically known to compose.

The expected outcome: this census surfaces all remaining integration gaps in parallel rather than one PR at a time. Each gap becomes a small prereq, but they are visible upfront, not discovered serially through repeated dispatches.

## The 7-step Trinity algebra

```
[lift, bind, lower, relift, rebind, lower-back, prove]
```

1. **lift** — Rust source bytes → `Term` + `DomainClaim`. LiftKit, registered as `lift-rust`.
2. **bind** — `Input::Term` → `DomainClaim` whose payload is `Term::Op { op_cid: concept:bind-result, args: [original_term, named_form_binding] }`. BindKit, registered as `bind-default`.
3. **lower** — `Input::Claim` (bind's output claim) → Python source + `DomainClaim`. LowerKit invoking the Python realize plugin via subprocess transport.
4. **relift** — Python source bytes → `Term` + `DomainClaim`, recovering concept-citation comments as concept-tier nodes. The Python source LiftKit.
5. **rebind** — `Input::Term` (relift's output Term) → `DomainClaim` whose payload is `Term::Op { op_cid: concept:bind-result, args: [...] }`. BindKit again.
6. **lower-back** — `Input::Claim` (rebind's output claim) → Rust source + `DomainClaim`. LowerKit invoking the Rust realize plugin via subprocess transport.
7. **prove** — `Input::Claim` (lower-back's output claim) → `DomainClaim` with `Verdict::Proved` + `ChainIntegrityWitness`. ProveKit invokes `walk_premises_to_root` and discharges chain-integrity.

## The seams

| Seam | Producer | Consumer | Status |
|---|---|---|---|
| 1 | LiftKit output `Term` | BindKit `transform(Input::Term)` | UNTESTED |
| 2 | BindKit output `Term::Op { concept:bind-result }` | LowerKit `claim_spec_value` | **CLOSED by A7 (#1069)** |
| 3 | LowerKit output Python source | Python source LiftKit `transform(Input::Source)` | UNTESTED |
| 4 | Python LiftKit output `Term` (with relift concept-citation recovery) | BindKit `transform(Input::Term)` | **OPEN GAP suspected** |
| 5 | BindKit output `Term::Op { concept:bind-result }` (second invocation) | LowerKit `claim_spec_value` (Rust target this time) | **CLOSED by A7 (#1069)** (same producer-consumer pair as seam 2) |
| 6 | LowerKit output Rust source | (relift to Term, then) ProveKit input | UNTESTED |
| 7 | Lower-back output `DomainClaim` | ProveKit `prove(claim)` running `walk_premises_to_root` | **ARCHITECTURAL QUESTION** |

### Seam 1 (lift → bind)

- Producer: LiftKit. Output: `Term` representing the Rust function's algebra.
- Consumer: BindKit. Expects `Input::Term`.
- Composition assertion: BindKit::transform accepts the exact Term shape LiftKit produces. No type or structure mismatch.
- Status: UNTESTED. The Path B blocker probe confirmed lift + bind succeed on real Rust source, so this seam is empirically holding for at least the probe's example. Add to the test partition: a fixture with non-trivial control flow (conditional + arithmetic + recursion) lifted then bound, with assertion that bind succeeds and the resulting Term::Op wrapper has the expected arg arity and op_cid.
- Discrimination test: a deliberately-malformed Term (e.g., missing required field) handed to BindKit refuses cleanly with a typed error. Not a panic.

### Seam 2 (bind → lower, forward leg)

- Producer: BindKit output `Term::Op { concept:bind-result, args: [original_term, named_form_binding] }`.
- Consumer: LowerKit's `claim_spec_value` in `lower_plugin.rs:210-228`.
- Status: **GAP CLOSED BY A7 (#1069).** The Path B blocker probe surfaced this as the first integration gap. A7's new match arm descends through the wrapper and synthesizes the same RealizeRequest that `cmd_lower` builds from `--named-terms-json` today. When A7 lands, this seam composes.
- Composition assertion: lower step on a real BindKit output produces valid Python source that the Python realize plugin renders without refusal. The synthesized RealizeRequest has the correct function name (extracted from the wrapped NamedTermDocument, not the synthetic `bind::default::bind-result-op-tree`).
- Discrimination test (already in A7's partition): a `Term::Op` with a DIFFERENT op_cid (not `concept:bind-result`) takes the existing fallback path, not the new descent.

### Seam 3 (lower → relift)

- Producer: LowerKit output Python source. The source is a real `.py` file the realize plugin emitted.
- Consumer: Python source LiftKit (`implementations/python/provekit-lift-python-source`). Expects `Input::Source { dialect, bytes }`.
- Composition assertion: the Python source LowerKit emits is parseable by the Python LiftKit. The relift produces a Term that:
  - has the same operations (concept ops) as the original Rust-lifted Term, modulo language-specific renaming;
  - recovers concept-citation comments correctly (per #1022's carrier work) for operations that had no native Python body;
  - is byte-deterministic on repeated relifts of the same Python source.
- Status: UNTESTED at the composition level. The Python carrier work (#1022 / #1034) tested its OWN round-trip (lower-to-Python then relift-Python recovers `concept_cid`). But the composition test "LowerKit's output is consumable by the Python LiftKit on real lifted Rust source" is not on record. Likely composes, but should be exercised.
- Discrimination test: emit Python source with a deliberately-broken concept-citation comment payload (malformed JSON, wrong schema version, mismatched CID). Python LiftKit refuses the relift with the correct `CompositionRefusalMemento.failure_kind` from #1021 / #1022.

### Seam 4 (relift → rebind)

- Producer: Python source LiftKit output `Term` (post-relift, with concept-citation-recovered NamedTerm nodes).
- Consumer: BindKit `transform(Input::Term)`. Same BindKit as in seam 1, second invocation.
- Composition assertion: BindKit::transform accepts the Term shape that the Python LiftKit produces. The recovered concept-citation comments produce NamedTerm nodes that look structurally similar enough to the original Rust-lifted Term's NamedTerm nodes that BindKit doesn't refuse on shape mismatch.
- Status: **OPEN GAP SUSPECTED.** The Rust LiftKit's Term shape and the Python LiftKit's Term shape may not be byte-identical because they encode the same algebra in their own canonicalized forms. BindKit might accept both shapes, OR might refuse one. Unknown until tested.
- Discrimination test: a Term lifted from Rust and the same algebra lifted from Python (assuming both lift the same source) handed to BindKit produces byte-identical bind output. If they produce different bind output, the federation property fails at this seam, not at the source-CID level.

### Seam 5 (rebind → lower-back)

- Producer: BindKit output `Term::Op { concept:bind-result, ... }` (second invocation).
- Consumer: LowerKit's `claim_spec_value` (third invocation, this time targeting Rust).
- Status: **GAP CLOSED BY A7 (#1069).** Same producer-consumer pair as seam 2; A7's match arm covers both invocations.
- Composition assertion: same as seam 2 but with target=`rust` instead of `python`. The Rust realize plugin renders the synthesized RealizeRequest without refusal.

### Seam 6 (lower-back → terminal claim → prove)

- Producer: LowerKit output Rust source (the final, "round-tripped" Rust).
- Consumer: ProveKit `prove(claim)`. ProveKit takes `Input::Claim` and walks the premise chain.
- Composition assertion: the terminal claim from lower-back has a complete premise chain back to the origin `Input::Source` CID. ProveKit's `walk_premises_to_root` succeeds with `Ok(())`.
- Status: UNTESTED. Probably depends on the verdict-propagation architectural question below.
- Discrimination test: deliberately break one premise CID in an intermediate claim (e.g., reference a non-existent CID). ProveKit refuses with `Verdict::Refuted` + the correct `ChainBreak::PremiseNotInCatalog`.

### Seam 7 (the verdict-propagation question)

This is the architectural question this census exists to surface, not the gap of A7-shape.

`walk_premises_to_root` per Path A (#1067) has a `NotProved { claim_cid, verdict }` variant: "a claim along the chain has verdict != Proved." Per A2 (#1066), only ProveKit emits `Verdict::Proved`; the other kits (lift, bind, lower, relift, rebind, lower-back) return claim unchanged from their default `prove()` impls, which means their claims have whatever default verdict the substrate emits at transform time (likely `Verdict::Pending` or `Verdict::Inconclusive`).

**The question:** does `walk_premises_to_root` require Proved at EVERY claim in the chain, or only at the TERMINAL claim?

- If the strict interpretation shipped: `walk_premises_to_root` fires `NotProved` on the lift claim (verdict != Proved). Chain-walk always fails on a 7-step path. ProveKit can never emit Verdict::Proved because the chain-walk inside its prove() invocation always returns Err.
- If the lenient interpretation shipped (only terminal must be Proved): chain-walk traverses intermediate claims regardless of their verdict; only the terminal claim (the prove step's input) must be Proved before ProveKit returns its own Proved verdict. But that's circular: ProveKit's prove() runs walk_premises_to_root on its OWN input, which is the claim BEFORE it has set Verdict::Proved.

Three architectures that resolve this:

- **(α) Strict + intermediate kits emit Proved.** Every kit's `transform()` produces a `DomainClaim` with `verdict: Verdict::Proved` for its own transform output. The kit treats "I successfully ran" as a proof of its own contract. Chain-walk requires Proved at every step. Simple but conflates "transform succeeded" with "semantic correctness." Probably wrong under Supra omnia rectum because the substrate's first principle says we don't claim more than we can prove, and "transform succeeded" is not a proof of correctness.
- **(β) Lenient verdict policy.** Chain-walk's `NotProved` variant fires only on the terminal claim (the one being proved), not on intermediate claims. Intermediate claims have whatever verdict; chain-walk only checks structural integrity (signatures, premise CID resolution, no cycles, no orphan from CIDs). Honest about what the chain-walk proves: the chain is well-formed, signed, and complete; whether intermediate steps are semantically correct is a different question that a richer prove kit could answer later.
- **(γ) New intermediate verdict.** Introduce `Verdict::Transformed` (or similar) for intermediate transform outputs. Chain-walk accepts `Transformed` at intermediate positions and requires `Proved` only at the terminal. Three-state verdict gives the substrate richer vocabulary for proof state.

**My architect recommendation:** option (β). The chain-walk's job at the prove step is to verify that THIS prove invocation's premise chain is structurally complete and signature-verifiable, not that every prior kit's transform was somehow "proven." The substrate's first principle is upheld by being precise about what chain-walk proves: it proves structural chain integrity, not semantic correctness of intermediate kits. The intermediate kits don't claim Proved because they didn't prove anything; they transformed. The prove step's claim IS Proved because it ran the chain-walk and it succeeded.

The change required if (β) is the call: relax `walk_premises_to_root`'s `NotProved` variant. Either remove it entirely (no verdict check on visited claims), or scope it to "the root claim being verified must be Proved" (which is a tautology that resolves to: ProveKit's prove() returns Proved iff its OWN claim's chain walks). Or rename `NotProved` to `NotProvable` or `ChainWalkRefused` and apply only to the terminal.

**Filed as the verdict-propagation question.** This is a separate prereq from A7. Should be filed and dispatched alongside A7 since both block #1068.

## Composition test plan

The census becomes the single comprehensive composition-test issue. Title: `Composition test census for the 7-step Trinity algebra: every producer-consumer seam exercised end-to-end against real toolchains`.

The PR ships:
- A new test file (e.g., `provekit-cli/tests/trinity_composition_census.rs`) in the slow-test lane per A5's policy.
- One test per seam (seams 1, 3, 4, 6, 7 are the ones not yet covered; seams 2 and 5 are covered by A7's tests). Each test exercises the producer-consumer composition against real subprocess transports (no fixture stubs per A5).
- Discrimination tests per seam (positive case + at least one negative case where composition should refuse cleanly).
- Reuses the test fixtures from #1039's per-kit conformance work where possible. Composition tests are the federation-level analogue to #1039's per-kit conformance tests.
- Runs in the slow-test lane on every PR.

The composition tests are NOT the Trinity exhibit. The exhibit (#1068) is the single execute_path call asserting the six binding assertions. The composition tests are the per-seam exercise that the exhibit composes. If any composition test fails, the exhibit cannot succeed; if every composition test passes, the exhibit becomes a thin smoke test that verifies the six assertions on top of an already-proven composition.

The architectural payoff: the composition tests are findable and runnable independently. If Trinity fails in CI, the composition tests tell you WHICH seam broke. The exhibit is the binding gate; the composition tests are the diagnostic layer underneath it.

## Path forward

The honest queue, in dispatch order:

1. **A7 (#1069) lands.** Closes seam 2 / seam 5.
2. **Verdict-propagation question gets an architect ruling** and a filed prereq (call it A8). Closes the architectural ambiguity in seam 7.
3. **Composition test census PR dispatches.** Files the composition tests for seams 1, 3, 4, 6, 7 against real toolchains in the slow-test lane. SURFACES any remaining gaps in parallel.
4. **Any gaps surfaced by the census** become small prereqs (A9, A10, etc. if needed). Each is a producer-consumer-seam-shaped fix, but now they're visible upfront rather than serially.
5. **#1068 Path B re-dispatches** against a clean composing main. The exhibit becomes a thin smoke test on top of empirically-proven seams.
6. **#1024 ships** when the exhibit's six binding assertions pass green on every PR.

This is more work than "one more PR." It's also more honest about what work is actually required. The pattern of `N keeps growing` resolves when the work is surfaced in parallel instead of discovered serially.

## What this changes about how I draft specs going forward

Every future producer spec must include, in its acceptance criteria, a composition test for every named consumer of the producer's output. The composition test is part of the producer's PR, not a follow-up. The producer ships when both its unit tests AND its composition tests pass.

The discriminator-tests-are-spec discipline from A2's bug becomes part of the producer's spec template. Each producer change includes:

- Unit tests on the producer's output shape.
- Composition tests on every consumer that depends on the producer's output.
- Discrimination tests: each output variant has a positive case (matches the variant) and at least one negative case (does not match the variant, must take the fallback path).
- Regression tests: the existing output shape's consumers still work after the producer change.

This is the spec discipline that was missing from A3. Locking it in now closes the failure mode that has produced #1043, #1044, A7, and (probably) at least one more A-shaped gap in the remaining unknown seams.

## Status snapshot (2026-05-16, this census)

**Merged and counted (from the closing-list):** carriers, keystone executor, PathDocument, LowerKit, BindKit, verb-selector, ConformanceDeclaration substrate, LiftKit, cmd_lower cleanup, A1, A2, A3, A4, A5, A6, Path A walks module.

**In flight:** A7 (#1069).

**Surfaced by this census:** verdict-propagation question (A8 to be filed), composition test census itself (this document becomes a dispatchable PR).

**Suspected open gaps not yet filed:** seam 4 (Python relift Term → BindKit). May surface a new prereq when the composition test runs.

**Unknown until tested:** seam 1, seam 3, seam 6.

**Blocked on prereqs:** #1068 Path B (blocks on A7, A8, composition tests), #1024 (blocks on #1068).

The list is the work. The work is the substrate. The substrate is the proof.
