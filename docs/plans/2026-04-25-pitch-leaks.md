# ProveKit Pitch Leaks: Solutions

The pitch has six legitimate cracks. Five are engineering, one is wording. Each below has a concrete path.

---

## Leak 1: "Garbage invariant, garbage proof"

**The crack.** Z3 proves the patch satisfies an invariant the LLM wrote from prose. The oracles catch unsatisfiable invariants, not WRONG ones. For arithmetic the prose-to-invariant gap is small. For "the auth flow has a race" it's a loaded gun.

**Solution: oracle #1.5 — invariant fidelity check.** Three independent verifiers, all of them mechanical, run before C1 returns the InvariantClaim:

1. **Cross-LLM derivation agreement.** A second LLM (different tier — sonnet if proposer was opus) reads the SAME prose bug report and derives its OWN invariant from scratch. Compare the two invariants for semantic equivalence:
   - Z3 implication check: `assert (=> proposer_invariant adversary_invariant)`. Both directions. If unsat (one entails the other), they agree on what's a violation. If sat in either direction, they disagree → reject the invariant; ask the proposer to refine.
   - This catches: cases where the proposer LLM hallucinates additional constraints or misses essential ones. Two LLMs from different tiers diverging is the signal that the prose has multiple plausible readings.

2. **Prose-to-clause traceability.** The proposer must annotate each clause of the SMT with the source-text sentence(s) that justify it. A verifier LLM re-reads the bug report and the annotated SMT and confirms each clause is grounded in cited prose. Ungrounded clauses (LLM speculating beyond the report) → reject; ask for citation.

3. **Adversarial-fixture pre-validation.** The proposer LLM also generates 5 positive fixtures (code that DOES exhibit the bug) and 5 negative fixtures (clean code that should NOT match). The principle's compiled match query runs against each fixture. If positives don't match or negatives do match, the invariant is wrong → reject. Same shape as oracle #6 but FIRES BEFORE C2 (not after C6) so the rest of the loop never runs against a bad invariant.

**Files:** new `src/fix/invariantFidelity.ts`. Modify `src/fix/stages/formulateInvariant.ts` to invoke the three checks before returning. Add tests in `src/fix/invariantFidelity.test.ts`.

**Cost:** roughly doubles C1 time (extra LLM calls + Z3 implication checks). Worth it for non-trivial invariants. Could be conditional on confidence: simple arithmetic invariants skip steps 1+2, complex semantic invariants run all three.

---

## Leak 2: "The demos are easy mode"

**The crack.** Division-by-zero and empty-catch are syntactic. The 7 open gaps need data-flow, which the substrate has but only in single-hop form (data_flow_transitive is bipartite; we documented).

**Solution: solve one hard bug end-to-end and ship the substrate it requires.** Pick the canonical hard case: **shell-injection**. The A8 memo named what's needed:

1. New capability `string_composition` — `{node_id, kind: 'template'|'concat'|'literal', has_interpolation: bool, interpolated_node_ids: text}`
2. New relation `data_flow_reaches(from_node, to_node)` — true if value of `from_node` can flow to `to_node` via 0+ hops. Requires fixing the bipartite-graph limitation in `data_flow_transitive` (already documented in `src/sast/dataFlow.ts`'s header).

**The plan:**

a. **Fix the bipartite-graph limitation.** Redesign data_flow edges so chains form. Two viable approaches per the existing memo:
   - Edge-shape redesign: emit `def → binding_node → use` triples so transitive closure produces real chains.
   - Bridge through node_binding: at query time, join data_flow + node_binding via shared variable name to compute reachability.
   Pick whichever is simpler; the populate-time approach is probably cleaner.

b. **Add `string_composition` capability via the substrate-extension path.** This is the dogfood test for hard-bug substrate proposals:
   - Write a fixture: `function exec(input) { return execSync(\`ls ${input}\`); }`
   - Write a bug report describing the shell-injection
   - Run `provekit fix` with real LLM, autoApply
   - Watch C6 propose `string_composition` capability + `data_flow_reaches` relation
   - All five substrate oracles fire on the proposal; if they pass, the bundle applies; the principle library now catches shell-injection forever
   
c. **Repeat for at least 2 more hard cases** (loop-accumulator-overflow, variable-staleness) before claiming the substrate handles real bugs.

**This is the existence proof.** Until ProveKit closes a hard bug whose principle requires multi-hop data flow + a non-trivial new capability, the easy-mode critique stands. After it does, the architectural pattern is proven across a meaningful range.

**Files:** see existing `docs/plans/2026-04-23-fix-loop/capability-gaps.md` for the per-gap specs.

---

## Leak 3: "Generalizes structure, not semantics"

**The crack.** The DSL matches AST shapes. Real bugs of the same class wear different syntactic clothes. `a/b` and `Math.floor(a/b)` and `a*Math.pow(b,-1)` are all the same bug; only the first matches a naive arithmetic-op principle.

**Solution: semantic equivalence at three layers.**

1. **Per-bug-class, multiple syntactic principles.** The C6 LLM is asked at principle-generation time: "now generate 2-3 ALTERNATIVE syntactic shapes this same bug class can take in real code." Each shape becomes its own principle in the same bundle, all with the same `bug_class_id`. Generation cost: ~3x more LLM calls at C6, but principles are cached forever; the cost is one-time per bug class.

2. **Semantic-equivalence relations as substrate primitives.** Add to the relation registry:
   - `same_call_target(a, b)` — both are calls whose callee resolves to the same function (cross-references via captures + binding tables). Already mostly available; needs wiring.
   - `same_arithmetic_value(a, b)` — both nodes evaluate to the same value under standard JS semantics. Computed via a small symbolic-execution pass over the SAST node and its data-flow ancestors. Bounded depth (e.g., 5 hops) for tractability.
   - `via_known_alias(a, b)` — `a` is known to be an alias of `b` (e.g., `const x = obj; x.f()` → `same_call_target(x.f, obj.f)`). Requires alias analysis, currently disclaimed in v1.

3. **Confidence-tiered matches.** Each principle match emits a confidence score:
   - Exact syntactic match: 1.0
   - Match through one semantic-equivalence relation: 0.8
   - Match through multiple semantic-equivalence relations: 0.6
   - Below a threshold, the bundle's `principle_candidate` artifact is flagged for human review instead of auto-applied. The library still grows but the auto-apply gate is more conservative for low-confidence matches.

**Cost:** layer 1 is cheap (LLM does the work). Layer 2 is one new capability (`alias_chain`?) plus a few relations — a substrate-extension. Layer 3 is wiring through bundle assembly.

**Honesty:** the substrate becoming truly semantic, not just structural, is multi-quarter work. The path is incremental. After layer 1 alone, the principle library handles 3-5x more real-world variants per bug class. Layer 2 expands further. Layer 3 makes the auto-apply boundary safer.

---

## Leak 4: "Five integration gaps in one run"

**The crack.** First real-LLM dogfood surfaced five gaps. That's a system that works for ONE bug, not a system hardened across hundreds. The seams aren't tight.

**Solution: deliberate fuzzing of the loop, with finding-rate as the readiness metric.**

1. **Bug report fuzzer.** Generate 100 synthetic bug reports across known classes (division, null deref, off-by-one, race condition described in prose, etc.) plus an examples corpus of 10-20 small TS projects each containing planted bugs. Run `provekit fix` against each, autoApply mode, with a stub LLM that mimics realistic Claude output (or use real Claude if budget permits — opus at 100 runs costs real money but produces meaningful data).

2. **Track per-stage failure rates.** For each run record: which stage failed, why, was the failure an integration gap or a real "the principle doesn't apply" rejection. Build a dashboard: failure-rate-by-stage over the corpus.

3. **Fix the top finding from each run, repeat.** After a sweep, the most-frequent gap is the next thing to fix. After 5-10 sweeps the system is hardened against the realistic bug surface.

4. **Property-based tests of the loop.** Beyond the corpus run: add invariants to the loop itself — "if D2 succeeds, ALL coherence flags are true," "if oracle #9 passes, the test file's content was non-empty," "if oracle #2 verdict is unsat, then path-condition extraction returned at least one assertion." Vitest fast-check generates inputs to verify these.

5. **Readiness threshold.** Define publish-ready as "across 100 runs in the corpus, integration gaps surface in <2% of runs and 95% of bundles successfully apply." Lower threshold for experimental access; higher for general availability.

**Cost:** fuzzer scaffolding (~3 task-agents), corpus curation (~2), 5-10 fix-sweep iterations. Probably 2-3 weeks of focused work for one engineer. Genuinely tightens the seams.

---

## Leak 5: "LLM is fungible — aspirational"

**Reword, not solve.** The accurate framing:

**Old:** "LLM is fungible. Pipeline assumes the LLM produces correct outputs."

**New:** "LLM tier is calibrated per stage. Intake parsing tolerates haiku. Classification tolerates sonnet. Invariant formulation requires opus. Lower-tier models on load-bearing stages degrade silently — the oracles catch unsatisfiable invariants, not vague ones. The pipeline assumes the LLM was competent for its assigned stage; the architecture's contribution is bounding what 'competent' has to mean (small structured output per stage, not 'understand the whole codebase')."

This is honest about what the LLM provides (small structured proposals) and what the pipeline provides (mechanical verification of those proposals). "Fungibility" was overclaim; "tier-calibrated with mechanical gates around competence" is accurate.

**Files:** Update README hero, ARCHITECTURE thesis paragraph, RETROSPECTIVE.

---

## Leak 6: "TS only, 2–8 min/fix on Opus"

**The crack.** Doesn't fit "every PR" yet. Multi-language is P7 (Tier 3); per-fix speed needs to drop.

**Solution: three independent speed wins, tested in order.**

1. **Tiered model selection (P4 in production-readiness, but specifically for speed).** Haiku for intake/classify (currently opus default; ~10s vs ~25s per call), sonnet for fix-generation, opus only for invariant formulation. Cuts wall time ~3x on a typical bundle without sacrificing the load-bearing decisions. Direct UX win.

2. **Principle-library short-circuit at C1.** If the bug shape matches an existing principle in the library, skip C1's LLM call entirely; use the principle's stored SMT template + the locus's bindings. Pure SMT instantiation, no LLM, oracle #1 still verifies. For division-by-zero (which is migrated), the C1 stage drops from ~15s to ~50ms. The compounding-library asset doubles as a speed asset.

3. **Speculative parallelism.** While C1 runs (opus, ~15s), C2 (overlay creation, ~600ms) can already be kicked off — they don't depend on each other beyond the locus, which is computed in B2. Same for C5 starting test generation in parallel with C4's complementary discovery. The orchestrator currently runs strictly sequentially; making independent stages parallel saves another 10-30% wall time.

**Combined target:** typical bundle from the current ~5 minutes to ~60-90 seconds. Multi-language remains its own large effort (P7 in the production-readiness plan), addressed separately.

**Files:** `src/fix/modelTiers.ts` (P4), `src/fix/stages/formulateInvariant.ts` (principle short-circuit), `src/fix/orchestrator.ts` (speculative parallelism wiring).

---

## Sequencing

The leaks are ranked by urgency:

1. **Leak 1 (invariant fidelity)** — most existential. Without it, the "verified correctness" claim is conditional. Fix first.
2. **Leak 4 (seams via fuzzing)** — every other leak benefits from a corpus-driven hardening pass. Fix second.
3. **Leak 2 (hard-bug existence proof)** — shell-injection or loop-accumulator-overflow. Demonstrates the substrate-extension path works on non-trivial cases.
4. **Leak 6 (per-fix speed)** — tiered models + principle short-circuit. Shifts the UX from "occasional batch use" to "per-PR realistic."
5. **Leak 3 (semantic generalization)** — multi-quarter incremental work. Layer 1 (alternative-shape principles) is cheap; layers 2 and 3 are larger.
6. **Leak 5 (rewording)** — cheap, do alongside any of the above.

None of this is research. All six paths are concrete engineering with named files and named approaches. The architecture's claim survives each one if executed; the marketing claim only survives some of them, and the gap between architecture and marketing IS the work above.
