# Trinity completion checklist: what being real actually requires

The substrate's Trinity claim is: source code in language A, lifted to the concept-tier hub via the substrate's algebra, transported through a registered realize kit to language B, lifted back, and the cycle closes byte-identical at the hub CID. The federation property holds across N>=2 languages. The proof chain is auditable end-to-end. The substrate's first principle (above all, correctness) is empirically verifiable, not just claimed.

This document enumerates what is required for that claim to be real. Each item is concrete and checkable. Order is dependency-aware; some items can land in parallel. No cycle-count estimate is given; the list closes when every item is checked.

## Substrate primitives (verifier-side)

- **A4 merged** (PR #1061 / SHA a48c6f45). `Term::walk()` iterator + `TermNode { op_cid, term_position: Vec<usize> }` slot-path semantics on main. 7 unit tests green.
- **A6 merged** (PR #1063 / SHA 431b2c19). `Catalog::contains(&Cid) -> bool` trait method with default impl `self.get(cid).is_some()` + HashMapCatalog override. Doc-comment names the fs-backed override as a future optimization for when an fs-backed impl is minted.
- **`walk_premises_to_root` landed** in `libprovekit::core::walks` (PR #1067 / SHA 3218f42c, Path A). Four `ChainBreak` variants (CycleDetected, PremiseNotInCatalog, OriginUnreachable, DeserializationFailed) + DFS-coloring algorithm (visited + on_path + reached_origin three-set discrimination) + eight unit tests (4 ChainBreak variants + 3 dedup discrimination + 1 happy). Algorithm corrects A2's single-HashSet bug that collapsed within-claim-dup / diamond-DAG / true-cycle to the same error.
- **`assert_concept_tier` landed** in `libprovekit::core::walks` (PR #1067 / SHA 3218f42c, Path A). Consumes `Term::walk()` + `Catalog::contains()` + `HubMissingNode { node_op_cid, term_position }` failure variant. Two unit tests (fail + happy).

## Substrate primitives (executor + chain)

- **A1 merged** (PR #1064 / SHA 8627786b). `PathExecutionChain` with `terminal_claim`, `claim_at_step`, `source_at_step`, `term_at_step` accessors. `execute_path` returns the chain. 11 call sites migrated. Three executor unit tests green.
- **A2 merged** (PR #1066 / SHA 9ecd9730). `ProveKit` registered with `KitRegistry` under name `"prove"` with `ConformanceDeclaration::NonCarrier`. `ProveKit::prove(claim)` runs `walk_premises_to_root` (inlined helper in `libprovekit/src/core/prove_kit.rs`) and returns `Verdict::Proved` + `Witness::ChainIntegrity(ChainIntegrityWitness)` on success; `Verdict::Refuted` + `Witness::ChainIntegrityFailure(ChainIntegrityFailureWitness)` on any `ChainBreak` (architect amendment: refutation is positive evidence, not the absence of a witness). 8 prove_kit tests green.
- **A3 merged** (PR #1065 / SHA a79214cb). BindKit's payload is `Term::Op { op_cid: concept:bind-result, args: [original_term, named_term_as_op_tree] }`. `concept:bind-result` op-CID minted in the concept-shapes catalog. Round-trip walkability test asserts every node's op_cid resolves via catalog lookup. Wire-format backward-compat preserved via `parse_named_or_bind_payload` helper.
- **#1049 lands.** Producer-side premise dedup in `execute_path`. Two-layer defense composes with `walk_premises_to_root`'s `HashSet` visited tracking.

## Capstone exhibit

- **Path A merged** (PR #1067 / SHA 3218f42c). The substrate primitives `walk_premises_to_root` (with DFS-coloring fix) + `assert_concept_tier` are on main in `libprovekit::core::walks`. The two helpers are ready for consumption by the exhibit.
- **Path B in flight as follow-up.** The runnable exhibit + integration test (single `execute_path()` call constructing a Path with seven steps lift → bind → lower → relift → rebind → lower-back → prove, six binding assertions, real lift/bind/lower/relift through registered kits) is split off as a separate follow-up issue. Path B requires architect ruling on real Python/Java toolchain in cargo test (assertion 4 demands real plugin invocation; existing precedent stubs target output). Open question: real-toolchain-in-cargo-test acceptable + slow-test lane shape + minimum fidelity for the federation check.
- **The six binding assertions, when Path B lands:**
  1. Source-CID equality at cycle close (`terminal_source.cid() == original_source.cid()`).
  2. Hub-tier check on forward leg (`assert_concept_tier(&post_bind_term, &catalog).is_ok()`).
  3. Hub-tier check on relift leg (`assert_concept_tier(&post_rebind_term, &catalog).is_ok()`).
  4. Hub-tier CID equality across legs (`post_bind_term.cid() == post_rebind_term.cid()`).
  5. Terminal `Verdict::Proved` with `terminal_claim.witness.is_some()`.
  6. `walk_premises_to_root(&terminal_claim, &origin_cid, &catalog, false).is_ok()`.
- Exhibit runs against a real `libprovekit::core` function (not a toy fixture-only example). Smallest function that exercises the algebra non-trivially: at least one arithmetic op, one conditional, one return.

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

- **v2 attestation directory created** at `provekit-warnings/provenance/v2/` post-Trinity. New `sign-ceremony.py` invocation captures the Trinity-proven substrate state (Trinity exhibit merged at SHA X, on Bitcoin block Y).
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

**Merged and counted:** Python carrier (#1034), Java carrier (#1041), C carrier (#1042), keystone executor + KitRegistry + LiftKit (#1036), PathDocument closure (#1035), LowerKit (#1043), verb-selector + `Kit::prove` default (#1044), BindKit (#1047), `ConformanceDeclaration` substrate (#1046), cmd_lower deletion-rule cleanup (#1048), A4 Term::walk + slot-path TermNode (#1055 / PR #1061), A5 exhibit transport policy (#1060 / PR #1062), A6 Catalog::contains (#1056 / PR #1063), A1 PathExecutionChain (#1058 / PR #1064), A3 BindKit op-tree + concept:bind-result mint (#1057 / PR #1065), A2 ProveKit chain-integrity (#1059 / PR #1066), **#1024 Path A: core::walks module + DFS-coloring fix + assert_concept_tier helper (PR #1067 / SHA 3218f42c)**.

**All six prereqs plus the substrate half of #1024 (Path A) are on main.** Only the runnable exhibit (Path B) remains.

**Two-attempt orientation surfaced:** the dispatched Opus agent twice correctly refused to compose the exhibit without architect rulings. First refusal surfaced the 6-vs-7-step algebra ambiguity (resolved: 7 steps, `[lift, bind, lower, relift, rebind, lower-back, prove]`) and the verbs.rs-vs-walks namespace question (resolved: new `core::walks` module). Second refusal surfaced a real algorithm bug in A2's `walk_premises_to_root` (single-HashSet collapses three outcomes to `CycleDetected`) which Path A corrects, AND a substantive architect question about real Python/Java toolchain in cargo tests (Path B blocker).

**Filed, in flight:** A7 #1069 (codex dispatched, worktree pk-1069-a7-claim-spec-value-decompose). LowerKit::claim_spec_value descends through A3's Term::Op { concept:bind-result } wrapper. Surfaced by Path B's empirical probe 2026-05-16: A3 changed BindKit's output shape but did not update LowerKit's input decomposition; bind to lower composition fails with `missing body-template entry` for synthetic operation_kind `bind::default::bind-result-op-tree`. Test partition follows the test-partition-is-spec discipline locked post-A2: positive + regression + discrimination + refusal + cross-target. Estimated ~30-60 LOC + 5 tests. The architectural lesson (any producer-output-shape change must include "consumer descends correctly" as a discrimination test) is the durable practice surfacing from this gap.

**Filed, blocked on A7:** #1068 Path B (Trinity exhibit runnable command + integration test). Architect ruling 2026-05-16 locked: Option 2 (slow-test lane in CI; real Python/Java toolchain; no fixture stubs; no `#[ignore]`). Lane shape: separate `[[test]]` entry or `cfg(feature = "slow-tests")`, runner image must have python3 + javac + cc + Werror, fails loudly on missing toolchain. Pre-decided architectural sentinel: BindKit invocation-determinism test must run BEFORE assertion 4 (separates "BindKit is deterministic" from "federation property holds"). Determinism sentinel ALREADY PASSED EMPIRICALLY on Path B's probe (real BindKit::transform x2 on same lifted Term produced byte-identical CIDs). Net Path B brief locked. Re-dispatches verbatim once A7 lands.

**Open follow-ups not blocking Trinity:** #1049 premise-dedup (Opus's non-blocking concern from #1047 review).

**Not yet captured durably in repo:** the architect rulings from the 2026-05-15 / 2026-05-16 sessions (deletion rule, build-on-existing-kits clause, pre-merge ritual, codex-inline-brief, exhibit transport policy, model defaults). Each lives in agent memory but not in a repo location a human contributor can find.

**Not yet on chain:** v2 attestation (post-Trinity snapshot). v1's OP_RETURN broadcast also still pending; OTS upgrade completed.

The list is the work. The work is the substrate. The substrate is the proof.
