# Open-issue triage after 2026-05-17/18 substrate landings

Date: 2026-05-18
Scope: open issues on TSavo/provekit, issue numbers 194-1148
Classifier: Kit (Opus), read-only audit
Source PRs (today): #1115-#1117 (platform semantics), #1118 (R1-R13 ruling),
#1127-#1135 (realization tag-kinds migration arc), #1138, #1142-#1144,
#1149 (#1141 C-lift fix), #1150 (Java floor #1137), #1151 (Rust floor #1145),
#1152 (Python floor #1146).

## Reading guide

- **OBSOLETE** = work completed by today's landings, recommend close
- **PARTIALLY-OBSOLETE** = some aspect solved, residual smaller than the issue
- **STILL-OPEN** = work remains, in scope, no landing touched it
- **BLOCKED** = depends on a thing not yet landed (named dep)
- **IN-FLIGHT** = active worktree per task description
- **needs-architect-call** = classification ambiguous

Umbrella note: GitHub does not auto-close umbrellas when sub-issues close. An
umbrella is OBSOLETE only when every sub-step in its body has landed; an
umbrella with one open sub-step is STILL-OPEN.

R1-R13 reshape note: the realization tag-kinds + marketplace ruling reframes
several pre-existing umbrellas (#880 contract-observation, #884 log-emit, #889
sugar-selection-policy, #755 runtime-mode emission sugars). They are NOT
retired by the ruling; they survive under the new framing. They remain
STILL-OPEN unless explicitly addressed by a PR.

PR #1144 reverted cmd_bind to emit Term::Op payload. No open issue in the
700-1148 range references the pre-revert shape as load-bearing; the revert
does not re-open any closed scope.

---

## Section A: OBSOLETE (work landed today, recommend close)

### #1119 -- Realization tag-kinds migration umbrella

OBSOLETE. All seven dispatch steps in the issue body landed:
1. RealizationMemento tagged enum -> #1120 closed by PR #1128
2. Classify concept-shapes entries -> #1121 closed by PR #1130
3. Concept API tagging primitives per language -> #1122 closed by PR #1131
4. Boundary-contract catalog seed -> #1123 closed by PR #1127
5. Exam manifest schema v1.1 -> #1124 closed by PR #1132
6. Regenerate v1.1 manifest -> #1125 closed by PR #1133
7. Re-dispatch #1106 against v1.1 -> #1126 closed by PR #1134

Recommend close with a "phases 1-7 all landed; ratified ruling #1118" comment.

### #1126 -- Realization migration #7: re-dispatch #1106 against v1.1 manifest

OBSOLETE. PR #1134 wired exam_question_cid + exam_manifest_cid into
TransportGapMemento, into the cluster path (smoke-test driver), the bind path
(libprovekit + cmd_bind), and the three mature lifters (Rust/Python/Java).
Spec §1.4 appendix added to the transport-gap protocol spec.

### #1106 -- Exam #C: wire cluster + bind to cite exam-question CIDs

OBSOLETE. PR #1134 diff includes `menagerie/smoke-test-e2e/driver/src/cluster.rs`,
`implementations/rust/libprovekit/src/core/bind.rs`,
`implementations/rust/provekit-cli/src/cmd_bind.rs`, and the
TransportGapMemento citation fields. Both cluster and bind paths cite the
v1.1 exam-question CIDs in refusal records. Discrimination tests per refusal
variant landed.

### #1136 -- Trinity floor-completion: drive absent count to zero across Rust+Java+Python

OBSOLETE. The three sub-issues landed today:
- Java (#1137) closed by PR #1150 -- 14 morphism mints + 2 sugar-carrier paths
- Rust (#1145) closed by PR #1151 -- 30 morphism mints + 8 sugar-carrier paths
- Python (#1146) closed by PR #1152 -- 34 morphism mints + Compare desugar + 8 sugar-carrier paths

GitHub did not auto-close the umbrella because the sub-issues did not list it
under `closes:`. Recommend close with a "Trinity floor at zero absent rows;
sub-issues 1137 + 1145 + 1146 all merged" comment.

### #1119 sub-issue chain note (already closed but worth confirming)

The closed-by-PR linkage from `recent_landed_prs`:

- #1120 closed by PR #1128
- #1121 closed by PR #1130
- #1122 closed by PR #1131
- #1123 closed by PR #1127
- #1124 closed by PR #1132
- #1125 closed by PR #1133
- #1141 closed by PR #1149
- #1137 closed by PR #1150
- #1145 closed by PR #1151
- #1146 closed by PR #1152

These appear closed already; if any remain open due to gh state lag they are
OBSOLETE.

---

## Section B: PARTIALLY-OBSOLETE

### #1103 -- Mint exam v1 manifest: name catalog questions as a first-class substrate artifact

PARTIALLY-OBSOLETE. The v1 manifest minted (PR #1111 closes #1105) and was
then superseded by the v1.1 manifest minted (PR #1133 closes #1125) under the
R1-R13 ruling that narrowed the schema to structural-only questions. The
umbrella's Phase C citation-wiring also landed (PR #1134). What remains
under the original framing is sub-phases D/E/F (exam-as-driver loop, drift
detection, exam-as-paper) which were never fully spelled out as numbered
issues. Recommend EITHER (a) close as the umbrella was superseded by #1119
under R1-R13, OR (b) tighten body to enumerate residual D/E/F work as fresh
sub-issues. needs-architect-call between (a) and (b).

### #1096 -- A25: function-name non-hashed sidecar channel

PARTIALLY-OBSOLETE / IN-FLIGHT. Two worktrees per task description are
actively addressing the function-name issue: Option C fn_name_sugar
annotation on NamedTerm (sonnet, pk-fix-lower-fn-name) and the R14 §2.5
appendix (sonnet, r14-fn-names-appendix). PR #1129's bind.rs fn_name
preservation also touches this surface. Hold for those worktrees to land
before re-classifying. STILL-OPEN until the sidecar channel formally lands.

---

## Section C: IN-FLIGHT (active worktree per task description; do not classify)

### #1023 -- Recursive concept composition over structured term/path inputs

IN-FLIGHT. codex, worktree pk-1023-recursive-composition. Hold.

---

## Section D: BLOCKED

### #1147 -- Stream A Stage 4: per-callsite migration point-query

BLOCKED on Stage 3 platform-semantics declarations. PRs #1115/#1116/#1117
landed Stage 3.1 platform-semantics declarations for Rust, Java, C kits. The
Stage 4 point-query mechanism builds on `effect_propagation::propagate_effects`
from earlier work. Ready to dispatch; awaiting executor. STILL-OPEN.

### #1148 -- Stream A Stage 5: CI gate asserting target-platform-dependent receipts

BLOCKED on #1147. Cannot land the CI gate until Stage 4 produces the
per-callsite point-query receipts. STILL-OPEN.

### #1068 -- #1024 Path B: Trinity exhibit runnable command + integration test

BLOCKED on architect ruling per issue body: "Does cargo test install + invoke
real Python/Java plugins for federation check (assertion 4)?" The Path A
substrate primitives merged at PR #1067. Path B awaits the real-toolchain
ruling. STILL-OPEN.

### #1081 -- A13: end-to-end composition test for type-erasure + realize-plugin

BLOCKED on Path B (#1068). The composition test requires the Trinity exhibit
runnable command from Path B. STILL-OPEN.

---

## Section E: STILL-OPEN (work remains, no landing touched)

### Bootstrap arc (the ProvekIt-self-hosted-in-CCL push)

Today's landings did not touch this arc. All STILL-OPEN.

- #893 -- CCL umbrella (the substrate is a programming language)
- #894 -- Bootstrap Phase 1: meta-layer primitives (op-definition + sorts)
- #895 -- Bootstrap Phase 2: round-trip tooling (proofir-ccl bridge)
- #896 -- Bootstrap Phase 3: op catalog lift
- #897 -- Bootstrap Phase 4: libprovekit (Rust) bootstrap
- #898 -- Bootstrap Phase 5: realize kits (per-(language, library))
- #899 -- Bootstrap Phase 6: lifters bootstrap
- #900 -- Bootstrap Phase 7: provekit-cli as primitive command adapters
- #901 -- Bootstrap Phase 8: spec catalog as CCL
- #902 -- Bootstrap Phase 9: CDDL in CCL
- #903 -- Bootstrap Phase 10: build infrastructure as CCL
- #904 -- Bootstrap Phase 11: tests + fixtures
- #905 -- Bootstrap Phase 12: sugars + sugar-selection
- #906 -- Bootstrap Phase 13: mint scripts
- #907 -- Bootstrap Phase 14: proofchain + algebraic effects in CCL
- #908 -- Bootstrap Phase 15: bootstrap completion
- #909 -- Bootstrap Phase 16: federation receipts
- #910 -- Bootstrap Phase 17: paper 26 placeholder
- #919 -- Bootstrap C2: Catalog-as-CCL signed memento
- #920 -- Bootstrap C3: Dual-form consistency CI gate
- #922 -- ProvekIt self-hosted in CCL: the bootstrap arc (top umbrella)
- #943 -- Bootstrap D7: lift libprovekit per-crate
- #961 -- D7-debt: retire procedural-macro loss class
- #962 -- D7-debt: retire trait-path-truncated
- #963 -- D7-debt: retire return-type-user-defined
- #964 -- D7-debt: retire ffi-call-unresolved-effect
- #965 -- D7-debt: mint concept:comment

These chain BLOCKED on prior phases per the bootstrap dependency tree;
classify each as STILL-OPEN and dispatch in phase order. The Trinity floor
landings did not feed any of these.

### PEP 1.7.0 / 1.8.0 migration arc

- #748 -- Trinity k(k(k(I)))=t under pep/1.7.0 -- Trinity floor work touched
  the morphism census, not the round-trip integration test. STILL-OPEN.
- #743 -- Rust CLI emit/consume pep/1.7.0 -- STILL-OPEN
- #744 -- Per-language kit emit/consume (12 kits, parallel) -- STILL-OPEN.
  Trinity floor work landed Rust+Java+Python (3 of 12) realization tagging;
  the per-kit pep/1.7.0 emit/consume migration is a different cross-cut.
- #745 -- Bug zoo regen on pep/1.7.0 -- STILL-OPEN
- #746 -- Bridgework + cross-language conformance harness -- STILL-OPEN
- #747 -- Final conformance sweep -- STILL-OPEN
- #750 -- concept:try / catch / except hub extension -- STILL-OPEN, queued PEP 1.8.0
- #751 -- concept:assert hub extension -- STILL-OPEN, queued PEP 1.8.0
- #752 -- concept:alloc hub extension -- STILL-OPEN, queued PEP 1.8.0
- #753 -- mint.sh cids.tsv absolute-path bug -- STILL-OPEN, small fix
- #754 -- WitnessMemento canonical form -- STILL-OPEN
- #755 -- Runtime-mode emission sugars: witness/monitor/emitter/gate -- STILL-OPEN, reframed by R1-R13 not retired
- #756 -- Tag carriers: source-visible CID audit trails -- STILL-OPEN
- #760 -- Trinity body emission redo through provekit-walk + Java kit -- STILL-OPEN
- #762 -- BridgeV14RoundtripTest pre-existing broken test -- STILL-OPEN

### Cross-X catalog and cross-library trinity (paper 21/22 arc)

- #844 -- Cross-X Catalog tracker -- STILL-OPEN
- #845 -- Cross-library axis umbrella (API-tier concept tagging + HTTP trinity receipt) -- STILL-OPEN
- #848 -- Bridge C: trinity HTTP sugar cells for C, Java, Python -- STILL-OPEN
- #849 -- Bridge D: HTTP payload in trinity round-trip -- STILL-OPEN
- #852 -- Bridge E: realize target identifier becomes (language, library) -- STILL-OPEN
- #853 -- Bridge F: permutation driver + truth-table aggregator -- STILL-OPEN
- #854 -- Bridge G: paper 22 combinatorial receipt -- STILL-OPEN
- #855 -- Bridge migrate-CLI: provekit migrate --library-from X --library-to Y -- STILL-OPEN
- #856 -- Substrate honesty pass (rename concept-shape fields by proof-strength gradient) -- STILL-OPEN
- #880 -- Umbrella: concept:contract-observation hub + four-mode composition -- STILL-OPEN, reframed by R1-R13 not retired
- #882 -- Legacy retirement: migrate all realize kits to four-mode contract-observation -- STILL-OPEN
- #884 -- Catalog cell: concept:log-emit + body-template citation -- STILL-OPEN, reframed by R1-R13 not retired
- #885 -- Catalog growth long-tail axes (crypto, ml, graphics, networking-protocol, file-io, process) -- STILL-OPEN
- #886 -- Vibe coding integration (paper 24 placeholder + spec) -- STILL-OPEN
- #887 -- Per-language witness runtime helpers -- STILL-OPEN
- #888 -- Bidirectional cross-language editing (paper 21 §5 demo) -- STILL-OPEN
- #889 -- Sugar-selection-policy memento -- STILL-OPEN, reframed by R1-R13 not retired
- #890 -- Stage 5: self-witnessing inlined emission -- STILL-OPEN
- #932 -- Catalog: contract-library sugars as loss-bearing cells -- STILL-OPEN

### Concept-shapes / wp-as-formula / gap-memento arc

- #642 -- wp-as-formula PR3: c11 + rust lifters emit wp_rule -- STILL-OPEN
- #643 -- wp-as-formula PR4: other 8 lifters -- STILL-OPEN
- #644 -- wp-as-formula PR5: ∀Q.⊑ z3 discharge (keystone) -- STILL-OPEN
- #645 -- wp-as-formula PR6: desugar wp-preservation onto evaluator -- STILL-OPEN
- #646 -- hub-shrink round 3: re-spec only-c11+1 ops -- STILL-OPEN
- #647 -- gap-memento PR2: z3 partial/lossy discharge -- STILL-OPEN
- #648 -- gap-memento PR3: CLI gap-refusal + --accept-loss -- STILL-OPEN
- #649 -- gap-memento PR4: PartialComposition + LossyDesugaring -- STILL-OPEN
- #650 -- Spec: Content Addressing Protocol (CAP) -- STILL-OPEN
- #651 -- Spec: Shadow Channels at compile time -- STILL-OPEN
- #652 -- Spec patch: role:abstraction-lift addendum -- STILL-OPEN
- #654 -- Paper 18: After Static Analysis -- STILL-OPEN
- #655 -- Catalog: macro-able C cells umbrella -- STILL-OPEN
- #656 -- Catalog: structural-rewrite C cells umbrella -- STILL-OPEN
- #657 -- Catalog: other-7-language abstraction-tier matrix fill -- STILL-OPEN
- #658 -- Python lifter: op_throw / op_raise / op_new -- STILL-OPEN
- #659 -- 21 false-refusal cells -- STILL-OPEN
- #660 -- Per-language realize-side compilers umbrella -- STILL-OPEN
- #661 -- Statement-hoisting impl chain -- STILL-OPEN
- #662 -- Dynamic-lang type-inferring lifters -- STILL-OPEN
- #664 -- LSP substrate overlay -- STILL-OPEN
- #666 -- Wrap every catalog cell as a signed .proof envelope -- STILL-OPEN
- #667 -- CI as content-addressed receipt lookup -- STILL-OPEN
- #678 -- Gate invariant: branch morphism set ⊇ main morphism set -- STILL-OPEN
- #686 -- discharge.py wipes ruby:source-unit on every mint -- STILL-OPEN (bug)
- #689 -- Spec: WitnessMemento empirical contracts -- STILL-OPEN
- #732 -- Plugin protocol umbrella -- STILL-OPEN
- #733 -- Spec: plugin-protocol + sugar-dict-memento + loss-function-memento -- STILL-OPEN
- #735 -- Sugar #1: Spring (Java) JSON file -- STILL-OPEN
- #736 -- Sugar #2: comment floor JSON per target language -- STILL-OPEN
- #737 -- Sugar #3: JUnit5 (Java, inverse of JUnit lifter) -- STILL-OPEN
- #738 -- Loss-fn #1: default lexicographic-preorder -- STILL-OPEN
- #739 -- Demo: rust unit test -> java with mixed sugars -- STILL-OPEN

### Walk + lifter coverage arc

- #371 -- walk Epic: extensions, IR enrichment, language coverage -- STILL-OPEN
- #373 -- walk: definition equations + quantifier mementos -- STILL-OPEN
- #374 -- walk: implication types from rustc -- STILL-OPEN
- #375 -- walk: implication types from execution -- STILL-OPEN
- #378 -- walk: generics + lifetimes + closure captures -- STILL-OPEN
- #379 -- walk: async + macros + const eval + unsafe -- STILL-OPEN
- #380 -- walk: C kit via libclang -- STILL-OPEN. Note: PR #1149 fixed the
  clang-18 RecoveryExpr issue, which is a prerequisite. Does not close.
- #381 -- walk: loop invariant inference -- STILL-OPEN
- #414 -- walk: C.9 integration test for Outlives composition -- STILL-OPEN

### LSP forward-propagation arc

- #308 -- LSP forward-propagation loop epic -- STILL-OPEN
- #313 -- rust LSP forward-propagation loop -- STILL-OPEN
- #314 -- go LSP forward-propagation loop -- STILL-OPEN
- #324 -- php LSP forward-propagation loop -- STILL-OPEN

### Bridge / kit completeness

- #246 -- zig kit dedup -- STILL-OPEN
- #248 -- verify_proof consistency-only -- STILL-OPEN
- #274 -- PHP conformance integration -- STILL-OPEN
- #286 -- spec: sort polymorphism cross-kit consensus -- STILL-OPEN
- #302 -- mint_bridge is actually mint_implication_attestation -- STILL-OPEN
- #476 -- v1.6.2 kit completeness matrix tracker -- STILL-OPEN
- #479 -- v1.6.2 bridge tagged-union migration cleanup -- STILL-OPEN

### Bind / realization plan / dialect / CLI spine

- #805 -- Bind evidence vNext: carry evidence entries -- STILL-OPEN
- #806 -- Realization plan consumer: contracts/loss/policy -- STILL-OPEN
- #820 -- bind-ir/1.0.0: document `witnesses` field as OPTIONAL additive -- STILL-OPEN
- #821 -- ir_term_to_text Const formatting doc-comment -- STILL-OPEN
- #841 -- Spec: GenerativeCompletionProtocol -- STILL-OPEN
- #869 -- Git filter driver + pre-commit hook -- STILL-OPEN
- #870 -- Async tree shaker: effect-propagation engine + PromotionDecisionMemento spec -- STILL-OPEN
- #871 -- Concept-shape post-condition: wp_rule + pending sentinel -- STILL-OPEN
- #975 -- Extend bind to emit cluster cardinality view -- STILL-OPEN
- #976 -- Paper 26 draft: After Concepts -- STILL-OPEN
- #977 -- Phase-5-Py: libprovekit-py self-host (n=1 cycle invariance) -- STILL-OPEN
- #978 -- Phase-6-Trinity: empirical Rust -> Python -> Rust cycle invariance -- STILL-OPEN
- #979 -- Close the name-lifecycle loop: bind -> edit comment -> relift -- STILL-OPEN
- #982 -- Generalize effect-propagation from migrate to every bind invocation -- STILL-OPEN
- #983 -- Vendor .proof publish + platform consume workflow -- STILL-OPEN
- #984 -- Generalize migrate-trigger to vendor-version-upgrade -- STILL-OPEN
- #992 -- Operator workflow: absorb vendor library via bind + LLM + relift -- STILL-OPEN
- #993 -- Adoption viral loop: PR operator-generated vendor .proof upstream -- STILL-OPEN
- #1025 -- CLI primitive-routing spine -- STILL-OPEN
- #1049 -- execute_path premise dedup at executor boundary -- STILL-OPEN
- #1051 -- Dialect: newtype String, remove closed enum + Other(String) smuggling -- STILL-OPEN
- #966 -- Transported-op primitive: CID-backed citation comments for language gaps -- STILL-OPEN

### Supply Chain Rails

- #498 -- npm inspector as complete package semantic model -- STILL-OPEN
- #499 -- promote JS lowerer from exhibit-specific ORP to general engine -- STILL-OPEN
- #1140 -- authenticated-betrayal preserved-contract witness missing /lowerResult/output/status -- STILL-OPEN. Standalone bug; not touched by today.

### Docs

- #194 -- Tier D doc gaps -- STILL-OPEN
- #195 -- future paper topics backlog -- STILL-OPEN

---

## Section F: needs-architect-call

### #1103 -- close as superseded vs split into D/E/F sub-issues

See Section B. Both paths reasonable.

### #889 / #880 / #884 / #755 -- R1-R13 reframe scope

These pre-date R1-R13. Their original framing assumed a different
RealizationMemento shape and different sugar/policy mechanics. They survive
the ruling but their bodies reference superseded structures. Recommend
architect-call on whether to rewrite the bodies under the R1-R13 framing
or leave as-is.

### #748 -- Trinity k(k(k(I)))=t under pep/1.7.0 vs Trinity floor-completion

Today's Trinity floor work (#1137/#1145/#1146) completed the morphism census
across the three Trinity languages. The pep/1.7.0 trinity round-trip
integration test (`trinity_roundtrip_test.rs`) is a different cross-cut
(content-payload byte-stability under the identifier flip). Worth confirming
that today's floor-completion did not silently green the round-trip test, in
which case #748 may be PARTIALLY-OBSOLETE.

---

## Section G: Skipped (issues < 700) per task scope

The following older open issues were not classified:

#194, #195, #246, #248, #274, #286, #302, #308, #313, #314, #324, #371,
#373, #374, #375, #378, #379, #380, #381, #414, #476, #479, #498, #499,
#642, #643, #644, #645, #646, #647, #648, #649, #650, #651, #652, #654,
#655, #656, #657, #658, #659, #660, #661, #662, #664, #666, #667, #678,
#686, #689.

(Several appear in Section E above for completeness because they remain
within active arcs. None were closed or reframed by today's landings.)

---

## Summary counts

- OBSOLETE: 4 (#1106, #1119, #1126, #1136)
- PARTIALLY-OBSOLETE: 2 (#1103, #1096)
- IN-FLIGHT: 1 (#1023)
- BLOCKED: 4 (#1068, #1081, #1147, #1148)
- STILL-OPEN: ~140 (bootstrap arc + PEP migration + cross-X catalog +
  walk arc + LSP arc + kit completeness + bind/realization spine +
  supply-chain-rails + docs)
- needs-architect-call: 3 scope items (#1103 path, R1-R13 reframe, #748)

## Recommended dispatch order after this audit

1. Close the four OBSOLETE umbrellas with summary comments citing the closing PRs.
2. Architect-call on #1103: close as superseded by #1119 OR refile residual sub-issues for D/E/F.
3. Hold #1096 + #1023 until in-flight worktrees land.
4. Dispatch #1147 (Stage 4) -- prereqs in place via #1115/#1116/#1117.
5. Architect ruling on #1068's real-toolchain question unblocks #1068 + #1081.
6. Bootstrap arc (#922 phases) remains the load-bearing queue; dispatch in phase order.
