# #1068 Real-Toolchain Ruling: Options for Architect

Audit doc, 2026-05-18. Read-only. Sir reads and rules; the ruling unblocks #1068 (Trinity exhibit runnable command + integration test, Path B of #1024) and #1081 (A13 end-to-end composition test for type-erasure + realize-plugin type translation).

Author: kit. Not a proposal. The doc enumerates the option space, marks which options are out-of-policy per A5, and surfaces a recommendation explicitly tagged as opinion.

## 1. The ruling question (verbatim)

From #1068 body:

> Does `cargo test` install + invoke real Python/Java plugins for the Trinity exhibit's federation check (assertion 4)?

Assertion 4 is `post_bind_term.cid() == post_rebind_term.cid()`. The federation property: the second `bind` against a relifted target source produces a CID-equal concept-tier IR to the first `bind` against the original source. Meaningful only if the relift step actually runs a real target-language source through a real lifter and a real binder. If any of those subprocesses is stubbed, assertion 4 collapses to "the test author wrote consistent fake JSON" (#1068 body verbatim).

## 2. Critical empirical context the architect should weigh

### 2.1 The slow-test lane is not hypothetical. It already exists, in production CI, exercising the exact pattern A5 endorsed.

- `implementations/rust/provekit-cli/Cargo.toml:55-64`: `slow-tests` feature is declared with a comment citing A5 transport policy.
- `implementations/rust/provekit-cli/tests/trinity_composition_census.rs:31`: `#![cfg(feature = "slow-tests")]` gates the entire file.
- `.github/workflows/ci.yml:920-1015`: a dedicated CI job `Trinity composition census (slow lane)` provisions Python 3.12 + JDK 21 + Rust stable, `pip install -e`s the real `provekit-lift-python-source` and `provekit-realize-python-core` packages, `cargo build`s `provekit-walk` + `provekit-realize-rust-core` + `provekit-cli`, then runs `cargo test --features slow-tests` against real subprocesses end-to-end.
- `.github/workflows/ci.yml:1017-1025`: a sibling step `python emit compile run conformance (slow lane)` runs `python_emit_compile_run_conformance.rs` in the same job under the same feature gate.

A5's policy doc at `docs/plans/2026-05-16-exhibit-transport-policy.md:19` literally names this pattern:

> CI can opt to run the exhibit in a separate slow-test lane that does not block other tests (e.g., a `cargo test --release --test trinity_exhibit --features slow-tests` invocation). The slow lane runs in CI; it does not run on every local `cargo test`. The lane is not optional; it runs on every PR.

The Trinity composition census (#1073, merged) is the empirical proof point that the lane works.

### 2.2 The `lower_kit_path_integration.rs:38-58` precedent is on-policy. It does not contradict A5.

The bash-script fake-realizer in `lower_kit_path_integration.rs` at lines 38-58 hard-codes a canned Python output via a JSON-RPC stdin/stdout protocol. At first read this looks like the "fixture-transport stub" A5 forbids. It is not, and the distinction is load-bearing for the Trinity ruling.

- A5 policy at `docs/plans/2026-05-16-exhibit-transport-policy.md:47`: the policy applies to "every future test that asserts a structural property the substrate is supposed to verify."
- A5 policy at `docs/plans/2026-05-16-exhibit-transport-policy.md:50-51`: the policy explicitly does NOT apply to "Unit tests of substrate-internal code that does not invoke kits (...) These exercise data structures and pure functions, not Kit transport."
- `lower_kit_path_integration.rs:139` (the test `lower_python_path_claim_input_cites_from_premise_to_and_loss_cids`): asserts that the LowerKit dispatch produces a claim whose `from`/`premises`/`to`/`artifacts` fields match expected CIDs given a known realizer response. The structural property under test is "LowerKit's claim-construction wires inputs to outputs correctly," not "the substrate's federation across languages is real." The fake-realizer is data-shaped fixture, not transport-shaped stub: it speaks the real PEP-1.7.0 JSON-RPC protocol and the test still exercises the real LowerKit + real `DispatchRealizeTransport` + real subprocess spawn.

The Trinity exhibit's assertion 4 is a different shape. It is the substrate's federation claim. A bash-script fake-realizer there would collapse the very property the test is supposed to verify, exactly as #1068 body warns.

So the precedent is on-policy AND insufficient for Trinity, simultaneously. Both clauses matter: the architect should not read this doc as if the precedent is wrong, and should not read it as if Trinity can copy the precedent.

## 3. The option space

### 3.1 Option 1: Real toolchain in the DEFAULT `cargo test` lane (every PR gated, no feature flag)

**Mechanism.** Move `slow-tests` out of the feature-gated lane and into the default `cargo test` run. CI provisions Python + JDK + cc on every workflow invocation; local `cargo test` requires the developer to have all three toolchains installed.

**Cost.**
- CI: every PR pays the pip-install + cargo build cost (~10-15 min currently per the trinity-composition-census job).
- Local dev: anyone running `cargo test` without Python or JDK installed sees test failures. Developers without the toolchains cannot validate their changes locally without per-feature opt-out.
- Infra: no new infra; just removes the feature gate.

**Substrate-correctness alignment (Supra omnia, rectum).** Highest. Every PR is forced to honestly run the federation check. No "we'll get to it in CI" deferment.

**Open questions / risks.**
- Significant developer-workflow regression. Sir's coordination protocol (memory: `coordination_protocol_provekit.md`) has Codex executing substrate work in isolated worktrees on remote toolchains. Forcing Python + JDK into every default test run also forces every Codex agent's worktree to provision both, which the gauntlet typically does NOT today.
- Conflicts with the lane-separation pattern A5 explicitly endorses at policy line 19. The pattern is "lane separation allowed, refusing-to-run forbidden", not "no lane separation."
- `needs-architect-clarification`: does Sir's Supra-omnia-rectum principle weigh purely on correctness OR on correctness AND practical reachability? A5's existing ruling treats lane separation as compatible with Supra omnia rectum because the slow lane is mandatory in CI, just not in `cargo test`. Re-opening that question for Trinity would re-open it for the entire `trinity_composition_census` + `python_emit_compile_run_conformance` lane.

### 3.2 Option 2: Real toolchain in the existing slow-test lane (sibling test in `trinity_composition_census` OR new lane job)

**Mechanism.** Add the Trinity exhibit integration test to `implementations/rust/provekit-cli/tests/` under `#![cfg(feature = "slow-tests")]`. CI runs it via the existing `slow-tests` feature flag in a dedicated job. Real Python + Rust subprocesses, real binary builds, real pip installs.

Two sub-mechanisms, marked as a sub-decision for the architect inside Option 2:

- **2a. Reuse the existing `Trinity composition census (slow lane)` job.** Add a new step to `.github/workflows/ci.yml:1017-1025`'s neighborhood that runs `cargo test --test trinity_citation_comments_exhibit --features slow-tests`. Shares the pip-install + cargo build steps with the existing census job (~10 min savings per PR vs separate job).
- **2b. New dedicated `Trinity exhibit (slow lane)` job.** Sibling to the existing slow-lane jobs, with its own pip-install + cargo build steps. Failure attribution is cleaner: a red Trinity-exhibit job is obviously the exhibit; a red shared job needs log grep to disambiguate.

`needs-architect-clarification`: 2a vs 2b. Kit's read on this sub-decision is in section 5; not binding.

**Cost.**
- CI: ~10-15 min on every PR if 2b; ~3-5 min incremental on every PR if 2a (the heavy Python provisioning + cargo build is amortized).
- Local dev: `cargo test` stays fast. Developers can opt into the slow lane with `cargo test --features slow-tests` locally if they have the toolchains.
- Infra: zero new infra. The lane already exists.

**Substrate-correctness alignment (Supra omnia, rectum).** High. The federation check runs honestly on every PR via CI; the slow lane is mandatory in CI per A5 line 19. The substrate's correctness claim is not weakened; the lane is just one CI invocation removed from `cargo test`.

**Open questions / risks.**
- Trinity exhibit imports `slow-tests` infrastructure. If A5's intent for #1024 specifically requires a tighter gate than the existing slow-lane jobs, this option does not satisfy it. Kit's read: A5 line 19 names this pattern verbatim and applies it to "the Trinity exhibit (#1024) when it lands" (A5 line 45). No tighter gate is on record.
- Sub-decision 2a vs 2b. Kit recommends 2b (separate job) for failure attribution; see section 5.

### 3.3 Option 3: Synthetic plugin output via the real-subprocess infrastructure

**Status: out-of-policy per A5.**

**Mechanism (for the record).** Use the existing `DispatchRealizeTransport` to spawn a bash script that emits structurally-correct JSON-RPC, similar to `lower_kit_path_integration.rs:38-58`'s fake-realizer.

**Why forbidden.** A5 policy at `docs/plans/2026-05-16-exhibit-transport-policy.md:25-29`:

> - Any test-only Kit trait impl that bypasses subprocess invocation.
> - Any `cfg(test)` branch inside production Kit code that returns canned output without invoking the transport.
> - Any `MockPythonKit` / `FakeJavaKit` / `StubCKit` registered in test setup.

A bash-script fake realizer in a Trinity exhibit test would satisfy the first and third bullets simultaneously. The lower_kit_path_integration precedent is exempt only because it is a unit test of LowerKit's RPC mechanics, not a structural-property test of substrate federation. Trinity assertion 4 IS a structural-property test of substrate federation. The distinction is the substrate's federation claim.

This option is enumerated so a future contributor cannot re-propose it without engaging the policy. It is not a live alternative.

### 3.4 Option 4: Stub one side, real the other

**Status: out-of-policy per A5.**

**Mechanism (for the record).** Real BindKit + real LiftKit; stub LowerKit + realize. The argument: assertion 4 only requires the bind output to be CID-stable across the two cycles, so the lower + relift can be canned as long as the relift produces the right source bytes.

**Why forbidden.** Same A5 lines 25-29 apply. The federation property is `post_bind_term.cid() == post_rebind_term.cid()`. If the relift's source input is canned, the rebind is computing the CID of a canned IR over a canned source, not a real federation. The "stubbed side" leaks back into the asserted property: any deviation between the canned source and what a real lower-back would produce becomes invisible to the test.

Same reasoning as Option 3. Enumerated only to close the door on future re-proposals.

## 4. Cross-references

- **A5 transport policy ruling** (#1062, doc at `docs/plans/2026-05-16-exhibit-transport-policy.md`). The load-bearing prior ruling. Lines 19, 25-29, 45-51 cited above.
- **Lower-kit precedent** (`implementations/rust/provekit-cli/tests/lower_kit_path_integration.rs:38-58`). Bash-script fake realizer that IS on-policy because the test is a wiring test, not a structural-property test. Inadequate for Trinity assertion 4 specifically.
- **Trinity exhibit fixture set** (#1159, merged 2026-05-18). 19 files at `menagerie/trinity-exhibit-fixtures/` covering 6 concept-transport categories. Pure data; ready to drive a runnable exhibit.
- **Trinity body-template completeness audit** (#1157, merged 2026-05-18, doc at `docs/audits/2026-05-18-trinity-body-template-completeness.md`). Headline: 7-concept gap in Java + Python body templates. Reconciliation: the `raise NotImplementedError("trinity lower")` text comes from a test-installed stub at `trinity_roundtrip_test.rs:106-110`, NOT from production code. Phase B1 (14 mechanical issues) closes the gap.
- **Existing slow-test infrastructure**:
  - `implementations/rust/provekit-cli/Cargo.toml:64` (`slow-tests = []`)
  - `implementations/rust/provekit-cli/tests/trinity_composition_census.rs:31` (`#![cfg(feature = "slow-tests")]`)
  - `.github/workflows/ci.yml:920-1015` (slow-lane CI job, real pip-install + real subprocesses)
- **#1081 A13** (end-to-end composition test for type-erasure + realize-plugin type translation). Per the issue body, A13 explicitly cites "real subprocess transport (per A5 transport policy doc)" in scope item 4. A13's ruling inherits from #1068's ruling: if Trinity uses slow lane, A13 does too.

## 5. Recommendation (OPINION, not fact)

Kit's read: **Option 2b** (real toolchain in a NEW dedicated `Trinity exhibit (slow lane)` CI job, gated by the existing `slow-tests` feature, separate from the existing `trinity_composition_census` job).

Reasoning, in order of weight:

1. **A5 already endorses this pattern**, verbatim, at policy line 19. The ruling for Trinity is the application of an already-locked policy, not a fresh architectural call. Sir does not need to re-derive the lane-separation discipline; he ruled on it three days ago.
2. **The infrastructure already runs in CI** at `.github/workflows/ci.yml:920-1015`, exercising real Python + Rust subprocesses via the same `slow-tests` feature. The Trinity exhibit reuses an established, tested pattern. No new infra, no new feature flag, no new precedent.
3. **2b over 2a** because failure attribution matters when Sir or a reviewer reads a red CI run. A red `Trinity exhibit (slow lane)` is unambiguous. A red shared `Trinity composition census (slow lane)` requires log inspection. The ~10 min CI savings from sharing the pip-install with the census job are real but not load-bearing on Sir's "Supra omnia rectum" principle; the marginal job duration is cheaper than the eventual debugging cost of an ambiguous attribution.
4. **Option 1 (default-lane real toolchain) would re-open A5**, not just close #1068. If Sir wants Option 1, the right move is to amend A5 first, propagate the change to the existing slow-lane jobs (Trinity composition census + python_emit_compile_run_conformance + java conformance), and then re-rule on #1068 with the new policy.
5. **Options 3 and 4 are not live**. They are out-of-policy per A5 lines 25-29 and enumerated above only so future re-proposals are stopped at the policy boundary.

This is opinion. The decision is Sir's.

## 6. Sub-decision under Option 2 (if Sir picks Option 2)

If Sir rules Option 2:

- 2a or 2b: shared job vs dedicated job. Kit's read = 2b for failure attribution. `needs-architect-clarification` if Sir disagrees.
- Test file name: `trinity_citation_comments_exhibit.rs` is the name the #1068 body uses verbatim. No clarification needed.
- Feature gate: `slow-tests` (existing). No new feature.
- A13 inheritance: A13 (#1081) lands in the same lane under the same gate. Per A13's scope item 4, this is already the planned shape.

## 7. What this doc does NOT decide

- Phase B1 body-template wiring (#1158 + 14 mechanical issues per #1157 audit §8). Independent of the toolchain ruling. Phase B1 must land before the Trinity exhibit can actually execute the federation check on the Python/Java side without `raise NotImplementedError` on certain concepts; the toolchain ruling determines WHERE the exhibit runs in CI, not WHETHER body templates are wired.
- Phase B2 Rust boundary set width (#1157 audit §8). Independent of the toolchain ruling.
- Whether `trinity_roundtrip_test.rs:106-110`'s test-installed stub upgrades to a real plugin. Per #1157 audit, this is a Phase B2 architect-call separate from #1068.
- The exact six binding assertions' order or wording. Pinned in #1068 body section "When Path B can dispatch."

## 8. Authority

Architect ruling required. This doc is preparation only. Tracked under #1068. Once Sir rules, the ruling lands in a follow-up doc at `docs/plans/2026-05-18-1068-real-toolchain-ruling.md` (date-prefixed convention per A5's sibling), and #1068 + #1081 dispatch.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
