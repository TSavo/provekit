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

1. **lift** : Rust source bytes → `Term` + `DomainClaim`. LiftKit, registered as `lift-rust`.
2. **bind** : `Input::Term` → `DomainClaim` whose payload is `Term::Op { op_cid: concept:bind-result, args: [original_term, named_form_binding] }`. BindKit, registered as `bind-default`.
3. **lower** : `Input::Claim` (bind's output claim) → Python source + `DomainClaim`. LowerKit invoking the Python realize plugin via subprocess transport.
4. **relift** : Python source bytes → `Term` + `DomainClaim`, recovering concept-citation comments as concept-tier nodes. The Python source LiftKit.
5. **rebind** : `Input::Term` (relift's output Term) → `DomainClaim` whose payload is `Term::Op { op_cid: concept:bind-result, args: [...] }`. BindKit again.
6. **lower-back** : `Input::Claim` (rebind's output claim) → Rust source + `DomainClaim`. LowerKit invoking the Rust realize plugin via subprocess transport.
7. **prove** : `Input::Claim` (lower-back's output claim) → `DomainClaim` with `Verdict::Proved` + `ChainIntegrityWitness`. ProveKit invokes `walk_premises_to_root` and discharges chain-integrity.

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
- Consumer: Python source LiftKit (`implementations/python/sugar-lift-python-source`). Expects `Input::Source { dialect, bytes }`.
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

### Seam 7 (the verdict-propagation question, resolved)

**Resolved option β. The shipped behavior is already correct. A8 is documentation-only.**

The original draft of this section described a `NotProved { claim_cid, verdict }` variant as if it had shipped in `walk_premises_to_root`. That was incorrect. Path A (#1067) shipped four `ChainBreak` variants: `CycleDetected`, `PremiseNotInCatalog`, `OriginUnreachable`, `DeserializationFailed`. There is no `NotProved` variant. ProveKit's `prove()` at `prove_kit.rs:74-89` calls `walk_premises_to_root_with_failure_steps` and stamps `Verdict::Proved` on `Ok`, `Verdict::Refuted` on `Err`. No verdict check on intermediate claims happens anywhere in shipped code.

The current behavior IS already option β: chain-walk proves structural chain integrity (signatures, premise CID resolution, no cycles, no orphan-from CIDs, origin reachability), not semantic correctness of intermediate kits. Intermediate claims have whatever verdict their producers set; chain-walk does not inspect their verdicts.

The architect ruling locks this:

- **(α) is the substrate's lying-shape applied to verdicts.** LiftKit stamping `Proved` on "I successfully transformed source bytes" claims something LiftKit did not prove. Refused on first principles.
- **(β) is the shipped behavior and the architecturally correct frame.** Chain-walk's job is to verify structural integrity, not to assert that every prior kit was somehow "proven." Intermediate kits transform; they do not prove. The prove step's claim IS Proved because it ran the chain-walk and it succeeded.
- **(γ) adds a `Verdict::Transformed` substrate API surface for a problem β already resolves with documentation.** Premature commitment.

The risk that remains is drift: a future contributor could add a `NotProved` variant or a verdict check on intermediate claims under the well-intentioned-but-wrong belief that "chain integrity" should include "every step's verdict." That drift would move shipped behavior from β toward α. A8 exists to lock the language and prevent that drift.

**A8 scope (documentation-only):**

1. Doc-comment on `walks.rs::walk_premises_to_root` explicitly stating the lenient policy. Locked language: "structural chain integrity" and "verdict semantics on intermediate claims are out of scope."
2. Module-level note preventing future contributors from adding a `NotProved` / verdict-check variant to `ChainBreak`. The doc-comment is the antibody.
3. One regression test: `walk_premises_to_root` over a chain where intermediate claims have `Verdict::Pending` / `Verdict::Inconclusive`. Assertion: returns `Ok`. Catches future α-shape drift.

Scope: ~15-30 LOC + 1 test. Smaller than A7. Does NOT block #1068; can ship in parallel with A7 or after Path B if optimizing for throughput.

**Filed as A8 (issue number TBD).** Downgraded from "blocking prereq" to "documentation lock that should land before the composition test census so future census readers do not re-litigate the question."

## Composition test plan

The census becomes the single comprehensive composition-test issue. Title: `Composition test census for the 7-step Trinity algebra: every producer-consumer seam exercised end-to-end against real toolchains`.

The PR ships:
- A new test file (e.g., `sugar-cli/tests/trinity_composition_census.rs`) in the slow-test lane per A5's policy.
- One test per seam (seams 1, 3, 4, 6, 7 are the ones not yet covered; seams 2 and 5 are covered by A7's tests). Each test exercises the producer-consumer composition against real subprocess transports (no fixture stubs per A5).
- Discrimination tests per seam (positive case + at least one negative case where composition should refuse cleanly).
- Reuses the test fixtures from #1039's per-kit conformance work where possible. Composition tests are the federation-level analogue to #1039's per-kit conformance tests.
- Runs in the slow-test lane on every PR.

The composition tests are NOT the Trinity exhibit. The exhibit (#1068) is the single execute_path call asserting the six binding assertions. The composition tests are the per-seam exercise that the exhibit composes. If any composition test fails, the exhibit cannot succeed; if every composition test passes, the exhibit becomes a thin smoke test that verifies the six assertions on top of an already-proven composition.

The architectural payoff: the composition tests are findable and runnable independently. If Trinity fails in CI, the composition tests tell you WHICH seam broke. The exhibit is the binding gate; the composition tests are the diagnostic layer underneath it.

## Path forward

The honest queue, in dispatch order:

1. **A7 (#1069) lands.** Closes seam 2 / seam 5.
2. **A8 lands** (documentation-only lock per the seam 7 resolution). Ships in parallel with A7 or after Path B; does NOT block #1068.
3. **Composition test census PR dispatches.** Files the composition tests for seams 1, 3, 4, 6 against real toolchains in the slow-test lane. SURFACES any remaining gaps in parallel.
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

**Draft-must-grep-verify discipline.** Adjacent failure mode caught while writing this census: the original draft of seam 7 named a `NotProved` ChainBreak variant as if it had shipped, when only four variants actually shipped on main. Every architect text that names shipped behavior must verify the shipped behavior at write time, not at commit time. Same shape as the deletion-rule grep, the em-dash grep, the test-partition-is-spec discipline. Add to the architectural rulings memory work alongside the other disciplines.

## Status snapshot (2026-05-16, this census)

**Merged and counted (from the closing-list):** carriers, keystone executor, PathDocument, LowerKit, BindKit, verb-selector, ConformanceDeclaration substrate, LiftKit, cmd_lower cleanup, A1, A2, A3, A4, A5, A6, Path A walks module.

**In flight:** A7 (#1069).

**Surfaced by this census:** verdict-propagation question (A8 to be filed), composition test census itself (this document becomes a dispatchable PR).

**Suspected open gaps not yet filed:** seam 4 (Python relift Term → BindKit). May surface a new prereq when the composition test runs.

**Unknown until tested:** seam 1, seam 3, seam 6.

**Blocked on prereqs:** #1068 Path B (blocks on A7, A8, composition tests), #1024 (blocks on #1068).

The list is the work. The work is the substrate. The substrate is the proof.
