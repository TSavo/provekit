# Trinity completion checklist: what being real actually requires

The substrate's Trinity claim is: source code in language A, lifted to the concept-tier hub via the substrate's algebra, transported through a registered realize kit to language B, lifted back, and the cycle closes byte-identical at the hub CID. The federation property holds across N>=2 languages. The proof chain is auditable end-to-end. The substrate's first principle (above all, correctness) is empirically verifiable, not just claimed.

This document enumerates what is required for that claim to be real. Each item is concrete and checkable. Order is dependency-aware; some items can land in parallel. No cycle-count estimate is given; the list closes when every item is checked.

## Substrate primitives (verifier-side)

- **A4 merged** (PR #1061 / SHA a48c6f45). `Term::walk()` iterator + `TermNode { op_cid, term_position: Vec<usize> }` slot-path semantics on main. 7 unit tests green.
- **A6 merged** (PR #1063 / SHA 431b2c19). `Catalog::contains(&Cid) -> bool` trait method with default impl `self.get(cid).is_some()` + HashMapCatalog override. Doc-comment names the fs-backed override as a future optimization for when an fs-backed impl is minted.
- **`walk_premises_to_root` landed** in `libsugar::core::walks` (PR #1067 / SHA 3218f42c, Path A). Four `ChainBreak` variants (CycleDetected, PremiseNotInCatalog, OriginUnreachable, DeserializationFailed) + DFS-coloring algorithm (visited + on_path + reached_origin three-set discrimination) + eight unit tests (4 ChainBreak variants + 3 dedup discrimination + 1 happy). Algorithm corrects A2's single-HashSet bug that collapsed within-claim-dup / diamond-DAG / true-cycle to the same error.
- **`assert_concept_tier` landed** in `libsugar::core::walks` (PR #1067 / SHA 3218f42c, Path A). Consumes `Term::walk()` + `Catalog::contains()` + `HubMissingNode { node_op_cid, term_position }` failure variant. Two unit tests (fail + happy).

## Substrate primitives (executor + chain)

- **A1 merged** (PR #1064 / SHA 8627786b). `PathExecutionChain` with `terminal_claim`, `claim_at_step`, `source_at_step`, `term_at_step` accessors. `execute_path` returns the chain. 11 call sites migrated. Three executor unit tests green.
- **A2 merged** (PR #1066 / SHA 9ecd9730). `ProveKit` registered with `KitRegistry` under name `"prove"` with `ConformanceDeclaration::NonCarrier`. `ProveKit::prove(claim)` runs `walk_premises_to_root` (inlined helper in `libsugar/src/core/prove_kit.rs`) and returns `Verdict::Proved` + `Witness::ChainIntegrity(ChainIntegrityWitness)` on success; `Verdict::Refuted` + `Witness::ChainIntegrityFailure(ChainIntegrityFailureWitness)` on any `ChainBreak` (architect amendment: refutation is positive evidence, not the absence of a witness). 8 prove_kit tests green.
- **A3 merged** (PR #1065 / SHA a79214cb). BindKit's payload is `Term::Op { op_cid: concept:bind-result, args: [original_term, named_term_as_op_tree] }`. `concept:bind-result` op-CID minted in the concept-shapes catalog. Round-trip walkability test asserts every node's op_cid resolves via catalog lookup. Wire-format backward-compat preserved via `parse_named_or_bind_payload` helper.
- **#1049 lands.** Producer-side premise dedup in `execute_path`. Two-layer defense composes with `walk_premises_to_root`'s `HashSet` visited tracking.

## Capstone exhibit

- **Path A merged** (PR #1067 / SHA 3218f42c). The substrate primitives `walk_premises_to_root` (with DFS-coloring fix) + `assert_concept_tier` are on main in `libsugar::core::walks`. The two helpers are ready for consumption by the exhibit.
- **Path B in flight as follow-up.** The runnable exhibit + integration test (single `execute_path()` call constructing a Path with seven steps lift → bind → lower → relift → rebind → lower-back → prove, six binding assertions, real lift/bind/lower/relift through registered kits) is split off as a separate follow-up issue. Path B requires architect ruling on real Python/Java toolchain in cargo test (assertion 4 demands real plugin invocation; existing precedent stubs target output). Open question: real-toolchain-in-cargo-test acceptable + slow-test lane shape + minimum fidelity for the federation check.
- **The six binding assertions, when Path B lands:**
  1. Source-CID equality at cycle close (`terminal_source.cid() == original_source.cid()`).
  2. Hub-tier check on forward leg (`assert_concept_tier(&post_bind_term, &catalog).is_ok()`).
  3. Hub-tier check on relift leg (`assert_concept_tier(&post_rebind_term, &catalog).is_ok()`).
  4. Hub-tier CID equality across legs (`post_bind_term.cid() == post_rebind_term.cid()`).
  5. Terminal `Verdict::Proved` with `terminal_claim.witness.is_some()`.
  6. `walk_premises_to_root(&terminal_claim, &origin_cid, &catalog, false).is_ok()`.
- Exhibit runs against a real `libsugar::core` function (not a toy fixture-only example). Smallest function that exercises the algebra non-trivially: at least one arithmetic op, one conditional, one return.

## Per-kit emit-compile-run conformance (#1039)

- **Python fixtures land.** N>=5 fixtures including hello_world, recursive function, arithmetic, control-flow, transported-op-via-concept-citation-comment. CI invokes `python3 -m py_compile` then `python3 <emitted>.py` and asserts behavior equivalence to the original source. Refusal on compile failure emits `CompositionRefusalMemento { failure_kind: target-compile-failure }`. Refusal on behavior divergence emits `failure_kind: target-behavior-divergence`.
- **Java fixtures land.** Same five fixtures shape, JUnit invokes `javac` then `java` on emitted source.
- **C fixtures land.** Same five fixtures shape, Makefile invokes `cc -Wall -Wextra -Werror` then `./out` on emitted source.
- **CI gates merge** on per-kit conformance test failure. A carrier PR that breaks emit-compile-run cannot merge.
- **Registry meta-test enforces** that any kit registered with `Carrier { fixtures_path, .. }` has a `fixtures_path` resolving to a directory containing at least the minimum fixture set.

## Federation evidence

- **Cross-language byte-identity asserted in CI.** At least one fixture lifted in both Rust and Python (via their respective lifters) producing byte-identical concept-tier CIDs at the hub. This is the colimit argument's empirical instance.
- **Same fixture cycled through Rust → Python → Java → C → Rust** (or some N>=3 permutation) producing byte-identical CIDs at the hub at every cycle point. The federation property's empirical proof.
- A README or test-doc captures which cross-language pairs are CI-verified and which are not. Honest about scope.

## Recovery and cleanup (drift repair)

- **#1048 merged.** Deletion-rule violation in `cmd_lower.rs` from #1043 cleaned up. `grep -rnE 'fn (dispatch_lower|missing_template_receipt|suggested_body_template_file)' implementations/rust/` returns zero matches. LowerKit's `lower_plugin.rs` does not internally call any of the deleted helpers.
- **The #1044 silent CI failure** documented. What was the actual gap (CI didn't exercise the verb field; tests use `..Default::default()`; field's `#[serde(default)]` made it tolerant)? Filed as a CI-coverage issue if not already.
- **The Dialect newtype-String refactor lands** (drafted but not yet filed; see `draft_issue_dialect_cid_into_catalog.md`). `Dialect::Other(String)` smuggling removed. Every callsite migrated. `ConformanceDeclaration::Carrier` drops the now-redundant `target_language: Dialect` field.

## Architectural rulings codified durably

- **A5 merged** (PR #1062 / SHA f629aa43). Exhibit transport policy doc in `docs/plans/2026-05-16-exhibit-transport-policy.md` capturing: real subprocesses are legitimate; fixture stubs forbidden; `#[ignore]` markers on structural-property tests forbidden. Cited from #1024 and #1039.
- **Pre-merge ritual** captured in a repo-level CONTRIBUTING or REVIEWING doc. The ritual: read full issue comment thread, grep-verify deletion rules, check architect tightening timestamp vs merge attempt, refuse merge if ritual incomplete. Currently in agent memory only; needs to be in the repo where human contributors can read it without an agent's memory store.
- **Build-on-existing-kits clause** captured as a repo-level discipline doc. Currently posted per-issue as a comment; should be a once-and-for-all clause that issues cite by reference.
- **Deletion rule** captured in the same discipline doc. Currently named per-PR; should be the canonical reference.

## Bitcoin-anchored lineage update

- **v2 attestation directory created** at `sugar-warnings/provenance/v2/` post-Trinity. New `sign-ceremony.py` invocation captures the Trinity-proven substrate state (Trinity exhibit merged at SHA X, on Bitcoin block Y).
- **OTS stamp** on v2 `attestation.json`.
- **OP_RETURN broadcast** of v2 attestation CID, txid saved to `v2/anchor-bitcoin-txid.txt`.
- **v1's pending OP_RETURN** also completed so the lineage is unbroken: v1 anchors the pre-Trinity substrate, v2 anchors the Trinity-proven substrate, both signed by T Savo, both on chain.

## Documentation and citation

- **Paper or section** in the repo's `papers/` directory or top-level README naming Trinity as the substrate's first empirical federation proof. The architectural claim is currently distributed across paper 7, paper 16, the substrate-trinity discussion, and conversation; needs to be consolidated into one citable artifact.
- **Trinity exhibit's binding assertions** documented as the canonical specification of what Trinity proves. Anyone reading the substrate's claim should be able to find this and check it against the exhibit's source.
- **The architect-rulings** locked in the 2026-05-15 / 2026-05-16 sessions (deletion rule, build-on-existing-kits, pre-merge ritual, codex-inline-brief, model defaults, architect-only-when-dispatcher-split) all captured durably in the repo, not just in agent memory.

## Closing gate

The list is complete when:

- Every item above resolves to either "merged" or "explicitly out of scope with a filed follow-up."
- The Trinity exhibit's six binding assertions pass on every PR (CI gate).
- The cross-language federation evidence runs on every PR.
- The v2 Bitcoin attestation is signed and on-chain.
- The architectural rulings are findable by a contributor who has never read an agent's memory.

When all of that is true, the substrate's Trinity claim is empirically real, not just structurally complete.

The list is NOT closed when:

- Any prereq is "in flight" instead of "merged."
- Any conformance fixture is `#[ignore]`'d or stubbed.
- The federation evidence runs only locally, not in CI.
- The architectural rulings live only in agent memory.

The list closes when every item resolves. The list does not close because "we shipped a lot." The substrate's first principle pays for the discipline of waiting for the actual close.

## Operational discipline going forward

When an architect agent is asked "how many cycles," the honest answer is "open this file, count the unchecked items, that is the answer." Estimates based on the visible front are the lying shape; the answer based on the complete list is the honest one.

When new gaps are discovered (a reviewer refuses to fabricate API, an executor dispatch surfaces a dependency we did not see, a CI test reveals a transport assumption), add them to this list. The list is the substrate's first principle applied to its own planning: above all, correctness about what is actually required.

## Status snapshot (2026-05-16, post-A1 merge)

**Merged and counted:** Python carrier (#1034), Java carrier (#1041), C carrier (#1042), keystone executor + KitRegistry + LiftKit (#1036), PathDocument closure (#1035), LowerKit (#1043), verb-selector + `Kit::prove` default (#1044), BindKit (#1047), `ConformanceDeclaration` substrate (#1046), cmd_lower deletion-rule cleanup (#1048), A4 Term::walk + slot-path TermNode (#1055 / PR #1061), A5 exhibit transport policy (#1060 / PR #1062), A6 Catalog::contains (#1056 / PR #1063), A1 PathExecutionChain (#1058 / PR #1064), A3 BindKit op-tree + concept:bind-result mint (#1057 / PR #1065), A2 ProveKit chain-integrity (#1059 / PR #1066), #1024 Path A: core::walks module + DFS-coloring fix + assert_concept_tier helper (PR #1067 / SHA 3218f42c), **A7 LowerKit::claim_spec_value Term::Op { concept:bind-result } descent + shared spec builder (#1069 / PR #1071 / SHA 1fab1a55)**, **A8 verdict-propagation policy lock + antibody doc-comment + regression test (#1070 / PR #1072 / SHA aa847755)**.

**All six prereqs plus the substrate half of #1024 (Path A) are on main.** Only the runnable exhibit (Path B) remains.

**Two-attempt orientation surfaced:** the dispatched Opus agent twice correctly refused to compose the exhibit without architect rulings. First refusal surfaced the 6-vs-7-step algebra ambiguity (resolved: 7 steps, `[lift, bind, lower, relift, rebind, lower-back, prove]`) and the verbs.rs-vs-walks namespace question (resolved: new `core::walks` module). Second refusal surfaced a real algorithm bug in A2's `walk_premises_to_root` (single-HashSet collapses three outcomes to `CycleDetected`) which Path A corrects, AND a substantive architect question about real Python/Java toolchain in cargo tests (Path B blocker).

**Census seams 2, 5, and 7 closed.** Seam 2 (bind to lower forward leg): closed by A7's claim_spec_value descent + shared spec builder. Seam 5 (rebind to lower-back, same producer-consumer pair): closed by same A7 work. Seam 7 (verdict-propagation): locked by A8 as the lenient policy with regression test antibody.

**Composition test census merged** (PR #1074 / SHA 032cd1f5, closes #1073). 9 tests covering 4 seams (1, 3, 4, 6) against real subprocess toolchains in the slow-test lane (`cargo test ... --features sugar-cli/slow-tests`). CI workflow adds a `trinity-composition-census` job. Three `#[should_panic]` markers document the surfaced gaps; when those gaps close, the markers come off and the tests go fully green. Parallel-surfacing payoff realized: three integration gaps visible in one PR instead of discovered serially.

**Three gaps filed and all three closed in parallel:**

- **A9 #1075 / PR #1078 / SHA 2c3695b6: MERGED.** Federation type-erasure at hub (architect option a locked: lifters strip types; realize kits reconstruct via two-layer canonical-token + per-plugin-map). Independent Opus retry caught Kit's locality-bias error (Kit-as-fallback reviewer initially posted MERGE-WITH-NITS on a non-existent deviation; fresh-context Opus found that `sugar-realize-rust-core::map_source_type(\"int\") -> \"i64\"` and `sugar-realize-python-core::map_source_type(\"int\") -> \"int\"` empirically implement the architect's "each realize plugin owns its language's type reconstruction" specification).
- **A10 #1076 / PR #1079 / SHA d37cb8d3: MERGED.** Python source lifter operator-atom preservation. 13-operator op-CID table at bind-IR boundary; add-vs-sub distinctness asserted; census seam 4 discrimination antibody promoted from `#[should_panic]` to `assert_ne`.
- **A11 #1077 / PR #1080 / SHA 16a973bc: MERGED.** Python realize body templates for 5 named concepts (concept:conditional, concept:eq, concept:decl, concept:lt, concept:mul). Per-template render+compile+execute verification. Decl-in-expression-position refusal pattern. Seam 3 positive antibody now refuses only on 2 UNNAMED entries (which A10 resolved to named ops; A11 follow-up flip pending).

**Filed, ready to re-dispatch:** #1068 Path B. Architect ruling locked: Option 2 (slow-test lane in CI; real Python/Java toolchain; no fixture stubs; no `#[ignore]`). Pre-decided BindKit determinism sentinel passes empirically on Path B's earlier probe. A7 + A8 both merged; A3-introduced lower-side gap is closed; β policy is locked. Path B's brief re-dispatches verbatim once the composition test census green-rate stabilizes (after A9/A10/A11 land OR with the `#[should_panic]` markers serving as documented-gap discipline until they do).

**Open follow-ups not blocking Trinity:** #1049 premise-dedup (Opus's non-blocking concern from #1047 review).

## Status snapshot (2026-05-16, post-antibody-flip)

**Census antibody flip merged** (PR #1082, 2026-05-16T22:29:06Z). The structural antibodies are now CI-load-bearing positive assertions; 9/9 census slow lane tests pass with `assert_eq!`/`assert_ne!` instead of `#[should_panic]` wrappers.

**Empirically validated at the algebra layer:**
- Seam 4 federation byte-identity (`add(x: i64, y: i64) -> i64 { x + y }` lifts from both Rust and Python to byte-identical bind CIDs). Paper 16's colimit argument applied to Sugar has its first empirical receipt.
- Seam 3 positive lower-relift round-trip (structural assertion: relift produces >=1 IR entry; lower step's body synthesis emits parseable Python).
- All other seam positive + discrimination tests pass.

**Substrate work landed in service of the antibody flip's empirical green:**
- γ canonical-form ruling (`cb5387532`): term_shape = `{concept_name, op_cid, args:[<sort markers>]}`; surface syntax stripped; deferred polymorphic-extension noted.
- Platform-semantics-via-LossRecord ruling (`e15cd3fd2`): per-platform semantic behavior declared at kit registration; cross-platform behavioral divergence captured via existing LossRecord mechanism; A18 reframed from URGENT BLOCKING to feature-gap.
- A14 γ (Rust lift, PR #1089), A15 γ (Python lift, PR #1087), A16 (Python realize body templates audit, PR #1088).
- Cascade fixes: deserializer optional fields (`610abaf5f`), file-key omission (`a70a13d55`), A21 source-location residuals (PR #1090).
- A19+A22 Rust envelope strip + bitnot disambiguation (PR #1091).
- A19+A20 Python envelope strip + statement-concept coverage (PR #1092).
- Seam 3 routing fix: Option A synthesis-from-term_shape with function-name placeholder fallback (PR #1094).

**Honest qualification on the seam 4 federation milestone:** the byte-identity holds for the no-contract pure-expression algebra. Federation across functions with contracts (attr_pre/post extracted differently per language), loops/sequences/function-calls (Python emits γ statement concepts post-A20 matching Rust's coverage), and bitwise-not on integers (A22 disambiguation) ALL fall under the same federation claim and pass once the per-axis fixes land (most have already landed).

**Behavioral-correctness follow-ups filed (not blocking algebra-layer federation claim):**
- A23 #1093: libsugar-side bind-payload hashes function-name field (federation residual at one layer deeper than A19 stripped).
- A24 #1095: operand-binding-from-context derivation in body synthesis. γ's bare-`{}` operand slots leave operand identity to be derived from context; current synthesis uses positional fallback. Emitted Python parses but operand bindings are semantically wrong for fixtures with let-bindings or literals.
- A25 #1096: function-name non-hashed sidecar channel. Current architecture uses `_sugar_synth` placeholder; production federation needs the real channel flowing from lift to realize without entering the bind CID hash.

A24 and A25 are both architectural-judgment-required to spec but mechanical-once-architected to implement; share a sidecar-channel architecture pattern. Likely worth implementing together in one PR.

## Status snapshot (2026-05-16, post-A24+A25 behavioral-correctness landing)

**Behavioral-correctness layer operational** (PR #1097 A23 merged 22:39:33Z, PR #1098 A24+A25 combined merged 23:58:09Z).

**A23 #1093 merged (PR #1097):** libsugar's bind-payload hash no longer carries `function`/`fn_name` envelope fields. Federation byte-identity now holds across languages with diverging function naming conventions (camelCase vs snake_case). Companion in-place fallback in `realize_function_name` falls back to `term.name` when `term.function` is empty (bridge until A25's sidecar arrived).

**A24+A25 #1095+#1096 combined merged (PR #1098):** the operand-binding sidecar channel + function-name sidecar channel both flow through libsugar's lower request via non-hashed metadata fields per the locked schema at `docs/plans/2026-05-16-operand-binding-sidecar-schema.md` (β with integer-array paths).

- Both lift kits (Rust walk_rpc.rs + Python bind_lifter.py) emit `operand_bindings` as a flat list of `{position: [int...], symbol: str}` tuples walked from term_shape root, plus optional `source_function_name`.
- Python realize plugin pre-processes `operand_bindings` into a path-to-symbol map and synthesizes function bodies from term_shape using the map, with a completeness gate (Supra omnia rectum) that asserts every term_shape leaf has a binding entry and refuses synthesis on misalignment.
- Federation byte-identity at the algebra layer (bind CID) is preserved regardless of operand-naming or function-naming differences. The colimit claim is now robust, not coincidental.

**Empirical signals on the combined merge:**
- Trinity census slow lane: 9/9 GREEN with CORRECT operand bindings (no longer positional fallback).
- Python realize + lift tests: 39/39 each.
- Bridgeworks targeted smoke: passes (incidental closure of task #70 pre-existing red via `.sugar/lower/<surface>` RPC manifest dispatch fix landed alongside A24+A25).

**The substrate's behavioral-correctness layer is fully operational:**
- Structural federation at algebra layer (γ canonical form + bind CID byte-identity): CI-gated.
- Body synthesis from term_shape via the non-hashed sidecar channel with operand-binding-from-context + function-name flow: CI-gated.
- Completeness-gate refusal pattern preserves the trichotomy (exact / loudly-bounded-lossy / refuse) at the realize boundary.

**Architect rulings codified durably:**
- γ canonical-form ruling: `docs/plans/2026-05-16-canonical-term-shape-form.md` on main.
- Platform-semantics-via-LossRecord ruling: `docs/plans/2026-05-16-platform-semantics-via-loss-records.md` on main.
- Operand-binding sidecar schema ruling: `docs/plans/2026-05-16-operand-binding-sidecar-schema.md` on main.
- PlatformSemanticTag schema ruling: `docs/plans/2026-05-16-platform-semantic-tag-schema-ruling.md` on main.
- γ post-merge audit: `docs/plans/2026-05-16-gamma-postmerge-audit.md` on main.

## Status snapshot (2026-05-17, post-#1039 + post-PlatformSemanticTag ruling)

**Per-kit emit-compile-run conformance (#1039) shipped across three languages:**
- Python (PR #1099): 5 fixtures (hello_world, recursive factorial, arithmetic, control flow, transported concept citation comment) + CI slow lane invoking LiftKit → BindKit → LowerKit → py_compile → python3 + behavior assertion; refusal paths for target-compile-failure and target-behavior-divergence both verified.
- Java (PR #1100): 5 fixtures + JavaConformanceFixtureTest invoking javac/java + refusal coverage; Rust registry meta-test for lower-java carrier fixture registration; Java canonical body templates with refreshed CID pin.
- C (PR #1101): 6 fixtures + emit-compile-run harness invoking cc -Wall -Wextra -Werror + binary behavior comparison; refusal probes for both failure_kinds; C body templates with refreshed plugin CID; test-c wired into test-all.

Carrier PRs that break the realize layer can no longer merge silently across any of the three languages.

**PlatformSemanticTag schema ruling locked** (`78fcb45f0` + em-dash fix `ec2f329c5`). Independent Opus authored after Sir caught Kit's substrate-enumerates framing as the same mistake the γ canonical-form ruling rejected at the term-shape layer. Schema: `PlatformSemanticTag { dimensions: BTreeMap<String, Cid> }` with kit-minted DimensionValueMemento carrying compare_to IrFormula. Substrate enumerates nothing; kits mint dimension names AND value mementos. Composition: pairwise intersection on key sets; CID-equality is equivalence; non-identical produces LossRecord entry with DivergenceBetween IrFormula. Refusal: asymmetric dimension keys → RefusalMemento with reason "uncharacterizable_platform_divergence". Zero new memento families; composes with existing LossRecord + RefusalMemento.

**Implementation work surface for the PlatformSemanticTag ruling (Stream A Stages 2-5) unblocked:**
- Stage 2: mint PlatformSemanticTag + DimensionValueMemento types in sugar-ir-types + extend kit registration with PlatformSemanticsDeclaration carrier field. Substrate-only; no kit changes yet.
- Stage 3 (per-kit, parallel-safe): each kit on main declares its per-op platform semantics by minting its own dimension names + value mementos at registration time.
- Stage 4: semantic-comparison step in execute_path; LossRecord emission on dimension intersection divergence; RefusalMemento emission on key asymmetry.
- Stage 5: CI gate asserting cross-platform composition emits the right artifact per the trichotomy.

Stages 2 → 3 → 4 → 5 are dependency-ordered; Stage 3's per-kit dispatches are parallel-safe once Stage 2 substrate types land.

**Architect rulings still in agent memory only (need durable repo capture):**
Deletion rule, build-on-existing-kits clause, pre-merge ritual, codex-inline-brief, exhibit transport policy, model defaults, framing-audit discipline. All available in agent memory; human contributors can't find them without an agent's memory store.

**Not yet captured durably in repo:** the older architect rulings from the 2026-05-15 / earlier-2026-05-16 sessions (deletion rule, build-on-existing-kits clause, pre-merge ritual, codex-inline-brief, exhibit transport policy, model defaults). Each lives in agent memory but not in a repo location a human contributor can find.

**Not yet on chain:** v2 attestation (post-Trinity snapshot). v1's OP_RETURN broadcast also still pending; OTS upgrade completed.

The list is the work. The work is the substrate. The substrate is the proof.
