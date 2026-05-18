# Bootstrap arc #922: next-phase readiness audit

Date: 2026-05-18
Method: gh issue traversal + dependency map against today's landings + the libprovekit lift+lower inventory (PR #1164)
Mode: read-only. Only file written is this audit.

## 0. Framing notes

### 0.1 Naming discipline (the umbrella body still says "CCL")

Per `coordination_protocol_provekit` (Sir, 2026-05-14): the bootstrap arc's
operational handle is "ProofIR-final" / "substrate self-hosting", not "CCL".
CCL the human-authored language is a separate later arc. The umbrella #922
body and several child titles still say "CCL"; the work is what it is
regardless of label. This audit uses "ProofIR-terminus" or "substrate
self-hosting" in its own prose and treats existing "CCL" issue titles as
historical labels for the same work.

### 0.2 What "today" means in this audit

The task asks readiness "given everything landed today (2026-05-17/18)".
Phase 1 leaves (#911-#914), Phase 2 leaves (#915-#917), Phase 3 C1 (#918),
and Phase 4 D1-D6 + D7-v0..v7 landed earlier and are taken as baseline.
The today-list below is the marginal-readiness delta on top of that
baseline.

## 1. Today's landings that change readiness

- Trinity floor-completion across Rust+Java+Python (#1136 / #1137 / #1145 /
  #1146 closed by PRs #1150 / #1151 / #1152). 14 + 30 + 34 morphism mints
  across the three Trinity kits; absent count drops to zero.
- R14 ruling (#1135) and R14.5 appendix (#1154) ratified the function-names-
  as-sugar discipline.
- Option C `fn_name_sugar` annotation on NamedTerm (PR #1153) threads
  `fn_name` through wire citations as sugar (closes #1148-the-sub, not the
  Stream A Stage 5 #1148).
- Citation wiring: `exam_question_cid` + `exam_manifest_cid` into
  TransportGapMemento (#1126 closed by PR #1134), via the v1.1 exam manifest
  arc (#1119 closed by PRs #1127-#1135).
- R5 four-shapes amendment (PR #1163): Monitor / Witness / Emitter / Gate.
- Trinity body-template completeness audit (PR #1157,
  `docs/audits/2026-05-18-trinity-body-template-completeness.md`):
  identifies the 14 declared-but-not-wired body-template slots that #1158
  Phase B1 mechanizes.
- libprovekit lift+lower inventory (PR #1164,
  `docs/audits/2026-05-18-libprovekit-lift-lower-inventory.md`): the
  foundational audit for libprovekit-self-bootstrap. Proposes 44 sub-issues
  across 7 stages (Stage 1 = Rust boundary widening x 5, Stage 2 = boundary
  contracts x 9, Stage 3 = realization seeding x 14, Stage 4 = body-template
  wiring x 6, Stage 5 = module-level emission x 5, Stage 6 = Rust idioms x 4,
  Stage 7 = end-to-end validation gate x 1).
- #1068 real-toolchain ruling options audit (PR #1161,
  `docs/audits/2026-05-18-1068-real-toolchain-ruling-options.md`):
  enumerates the option space Sir reads to unblock #1068 Path B.
- R14 reframe scope audit (PR #1160, on R14's effect on #880 / #884 / #889 /
  #755): the four umbrellas survive the ruling.
- Trinity exhibit fixture set covering 6 concept-transport categories
  (#1159, #1068 prep).
- Stream A Stage 4 per-callsite migration receipt (in-flight as task per
  triage doc; PR #1155 named in #1162 body).
- Recursive concept composition (#1023, in-flight per triage doc).
- SugarSelectionPolicyMemento mint (per task-brief; not located by issue
  number in this audit, no readiness impact for #922 children).
- Trinity exhibit Path B 2b dedicated CI job (in flight per task brief).

The marginal effect on #922 readiness: the Trinity floor closes the
"absent realization rows" failure mode that blocked any cross-language
emission claim, and the lift+lower inventory (PR #1164) turns Phase 4
post-D7 + Phase 5 from "stubs without filed leaves" into "44 concrete
dispatch units with a topological order".

## 2. #922 child inventory

GitHub does not auto-close umbrellas when sub-issues close. An umbrella is
classified by aggregating its children, not by its own state.

Phase 1-3 leaves landed weeks ago. The current frontier is at the boundary
of Phase 4 (D7 + D7-debt + the lift+lower 44-issue arc the inventory
implies) and Phase 6 (the Trinity cycle #978 stack).

### 2.1 Phase umbrellas (#894-#910 + lateral #977 / #978)

| issue | title                                  | aggregate status | residual blockers          |
|-------|----------------------------------------|------------------|----------------------------|
| #894  | Phase 1: meta-layer primitives         | Children closed (#911-#914). Umbrella STILL-OPEN per GH state, OBSOLETE in effect. | None. Recommend close.    |
| #895  | Phase 2: round-trip tooling            | Children closed (#915-#917). Same shape. | None. Recommend close.   |
| #896  | Phase 3: op catalog lift               | C1 (#918) closed. C2 (#919) + C3 (#920) READY-TO-DISPATCH (no upstream blocker). | C2 + C3 dispatch. |
| #897  | Phase 4: libprovekit (Rust) bootstrap  | D1-D6 closed. D7 (#943) OPEN with v0-v7 modules merged; one module terminus reached. D7-debt #961-#965 OPEN. | D7 next-module dispatch + 5 D7-debt leaves. |
| #898  | Phase 5: realize kits                  | Stub umbrella, leaves never filed. PR #1164 inventory provides the dispatch plan (Stage 3 + Stage 4 + Stage 5). | File leaves per #1164 sections. |
| #899  | Phase 6: lifters bootstrap             | Stub umbrella. Lateral #978 (Trinity n=2 cycle) holds the operational dispatch stack; 6 of 8 dispatch-stack children closed. | Wait on #1023 (in flight) + #1068 (architect ruling). |
| #900  | Phase 7: provekit-cli adapters         | Three named leaves #1025/#1026/#1027 closed; G1-G7 not filed. | File G-series after #1023 lands. |
| #901  | Phase 8: spec catalog as CCL           | Stub. Depends on Phase 2 (closed). Leaves H1-H6 not filed. | File leaves; not blocked. |
| #902  | Phase 9: CDDL in CCL                   | Stub. Depends on Phase 2 (closed). Leaves I1-I4 not filed. | File leaves; not blocked. |
| #903  | Phase 10: build infrastructure         | Stub. Depends on Phase 5. | Phase 5 first. |
| #904  | Phase 11: tests + fixtures             | Stub. Depends on Phase 5. | Phase 5 first. |
| #905  | Phase 12: sugars + sugar-selection     | Stub. Depends on Phase 5, refs #889. | Phase 5 first. |
| #906  | Phase 13: mint scripts                 | Stub. Depends on Phase 6. | Phase 6 first. |
| #907  | Phase 14: proofchain + algebraic effects | Stub. Depends on Phase 4. | Phase 4 close-out. |
| #908  | Phase 15: bootstrap completion         | Stub. Depends on Phases 4-14. | Late-arc capstone. |
| #909  | Phase 16: federation receipts          | Stub. Depends on Phase 15. | Late-arc capstone. |
| #910  | Phase 17: paper 26 placeholder         | Stub. Depends on Phase 16. | Final. |
| #977  | Phase-5-Py umbrella                    | Lateral umbrella; n=1 Python diagnostic at #995 CHARACTERIZED_DIFF. | Driven by D7-debt closure + python body-template completeness. |
| #978  | Phase-6-Trinity umbrella               | Dispatch stack 1-6 closed (#1020/#1021/#1022/#1026/#1027/Java-kit-sibling); step 7 = #1023 IN-FLIGHT; step 8 = #1024 Path A merged at PR #1067, Path B = #1068 BLOCKED-ON-ARCHITECT. | #1023 land + #1068 ruling. |

### 2.2 Phase-4 leaves (D-series + D7-debt + boundary-mechanism children)

| issue  | title                                                  | status                        | prereqs landed? | blocks-on                       |
|--------|--------------------------------------------------------|-------------------------------|-----------------|---------------------------------|
| #919   | C2: Catalog-as-CCL signed memento                      | READY-TO-DISPATCH             | Yes (#918)      | none                            |
| #920   | C3: Dual-form consistency CI gate                      | READY (after #919)            | Yes (#918)      | #919                            |
| #943   | D7: Lift libprovekit per-crate (first end-to-end)      | IN-PROGRESS (cluster work)    | v7 = MODULE TERMINUS on provekit-canonicalizer::value | next-module-next-crate dispatch unit |
| #961   | D7-debt: retire procedural-macro loss class            | READY-TO-DISPATCH             | Yes             | none (concept-mint, no cross-dep) |
| #962   | D7-debt: retire trait-path-truncated                   | READY-TO-DISPATCH             | Yes             | none                            |
| #963   | D7-debt: retire return-type-user-defined               | READY-TO-DISPATCH             | Yes (concept:sort exists per A4) | none                |
| #964   | D7-debt: retire ffi-call-unresolved-effect             | READY-TO-DISPATCH             | Yes (admissibility-spine landed) | none           |
| #965   | D7-debt: mint concept:comment                          | READY-TO-DISPATCH             | Yes             | none                            |
| #966   | Transported-op primitive umbrella                      | OBSOLETE-ish                  | #1020 / #1022 closed            | Recommend close as superseded |
| #975   | Extend bind to emit cluster cardinality view           | READY-TO-DISPATCH             | Yes             | none (single-file mechanical)   |
| #979   | Close the name-lifecycle loop: integration test + doc  | READY-TO-DISPATCH             | Yes (#944 + #1019 shipped pieces) | none (integration-test leaf) |
| #982   | Generalize effect-propagation to every bind invocation | READY-TO-DISPATCH             | Yes (engine + demo ship)        | none (generalization leaf)     |
| #983   | Vendor .proof publish + platform consume               | READY (after #981/#982 land)  | #981 closed; #982 open          | #982                            |
| #984   | Generalize migrate-trigger to vendor-version-upgrade   | READY (mostly)                | migrate + RefusalMemento exist  | nominal; recommend defer to wave 2 |
| #992   | Operator workflow: absorb-any-vendor                   | READY (operator-workflow leaf) | bind + LLM auto-namer (#980) closed | needs end-to-end script wiring |
| #993   | Adoption viral loop: PR vendor .proof upstream         | BLOCKED on #983 + #992        | --              | #983, #992                      |

### 2.3 Phase-5/6 leaves (Trinity + boundary mechanism + post-floor)

| issue  | title                                                  | status                        | prereqs landed?           | blocks-on              |
|--------|--------------------------------------------------------|-------------------------------|---------------------------|------------------------|
| #1023  | Recursive concept composition over structured term/path inputs | IN-FLIGHT (codex worktree per triage doc) | #1020 closed | --       |
| #1068  | #1024 Path B: Trinity exhibit runnable command         | BLOCKED-ON-ARCHITECT          | Path A merged (#1067)     | Sir's ruling on PR #1161 toolchain options |
| #1081  | A13: end-to-end composition test for type-erasure + realize-plugin | BLOCKED on #1068      | --                        | #1068                  |
| #1147  | Stream A Stage 4: per-callsite migration point-query   | IN-FLIGHT (per triage doc; PR #1155 cited in #1162 body) | Stage 3.1 platform-semantics declarations all landed | -- |
| #1148  | Stream A Stage 5: CI gate                              | BLOCKED on #1147              | --                        | #1147                  |
| #1158  | Trinity body-template completeness (B1 + B2)           | READY-TO-DISPATCH (B1 mechanical); B2 architect-call | Floor-completion landed today | B1 dispatchable; B2 needs ruling |
| #1162  | Stage 3.1 extension: SQLite-API divergence dimensions  | BLOCKED on #1147              | --                        | #1147                  |

## 3. READY-TO-DISPATCH (dispatch order)

The set splits into three independent waves. Each item below cites the
empirical fact that its prereqs are landed.

### Wave 1: independent leaves with no cross-wave dependency

1. **#919 C2: Catalog-as-CCL signed memento.** Phase 3 close-out. C1 (#918)
   closed: 84 ops lifted to op-definition citations under PR #958. Mint a
   `CatalogAsCCLMemento` citing every op-definition CID; sign under
   provenance key. One PR. Mechanical. Codex (gpt-5.5 xhigh). Mints a single
   memento.
2. **#965 D7-debt: mint concept:comment + lift trivia comments.** No cross
   issue dependency. Mechanical: extend `provekit-walk` to capture trivia
   comments; mint `concept:comment(text, position)`; emit through
   realize kit. Codex.
3. **#963 D7-debt: retire return-type-user-defined.** Extends
   `provekit-walk::type_decl` to lower `A<B>` to
   `concept:sort(name="A", args=[CID(B)])`. The A4 concept:sort exists per
   #914. Mechanical. Codex.
4. **#962 D7-debt: retire trait-path-truncated.** Resolve `Arc::new` via
   rustc name resolution from `provekit-walk`. Mechanical. Codex.
5. **#964 D7-debt: retire ffi-call-unresolved-effect.** Route ffi-call
   sites through the existing `concept:effect-occurrence` primitive
   (admissibility-spine work, 2026-05-13). Mechanical. Codex.
6. **#961 D7-debt: retire procedural-macro loss class.** Mint
   `concept:proc-macro-invocation` and route derives through it.
   254 rows of loss. Mechanical but larger. Codex.
7. **#975 Extend bind to emit cluster cardinality view.** Single-file
   addition to `cmd_bind.rs`. Mechanical. Sonnet or codex.

Wave-1 issues #919, #961, #962, #963, #964, #965, #975 can dispatch in
parallel: zero cross-issue conflicts, each touches a different region of
`provekit-walk` or `cmd_bind.rs` or a memento mint script.

### Wave 2: ordered close-outs after wave 1

8. **#920 C3: Dual-form consistency CI gate.** Depends on #919. CI gate
   asserting legacy op_*.spec.json and op-definition citations describe the
   same ops. Mechanical. Codex.
9. **#979 Close the name-lifecycle loop: integration test + doc.** Doc +
   integration test only; pieces ship (#944 + #1019). Sonnet sufficient.
10. **#982 Generalize effect-propagation to every bind invocation.**
    Engine + demo already ship; this generalizes invocation. Codex.
11. **#1158 Phase B1: wire 14 declared-but-not-wired boundary realizations.**
    Mechanical body-template wiring per the Trinity completeness audit.
    7 concepts x 2 languages = 14 dispatch slots. Three-tests-per-variant
    discrimination discipline per `feedback_discrimination_tests_per_variant`.
    Sonnet or codex; codex preferred for the test bulk.

### Wave 3: lift+lower inventory's 7-stage plan (#1164's proposal)

The lift+lower inventory in PR #1164 proposes 44 sub-issues. These are
NOT yet filed as GitHub issues. The meta-decision is whether to file them
now (so they can dispatch) or wait. Recommend: file Stage 1, Stage 2,
Stage 6 immediately because they parallelize and unblock Stages 3-5.

Per #1164 §7 dispatch order:

12. **File Stage 1 leaves (5 issues, Rust boundary widening).** Each issue
    promotes one Rust sugar-carrier to boundary (concept:dynamic-dispatch,
    concept:closure, concept:iterator, concept:generic-instantiation,
    concept:reference). Architect-call per #1164 §8.2: each issue must
    include a "two-vote justification" why the concept is observable at
    the libprovekit substrate level. Codex with Opus review.
13. **File Stage 2 leaves (9 issues, boundary contracts).** Mechanical
    boundary-contract minting per #1164 §3 (blake3-512, ed25519-rfc-8032,
    jcs-rfc-8785, json-rfc-8259, base64-rfc-4648-stdpad,
    filesystem-posix-read, subprocess-posix-spawn, json-rpc-2.0,
    c-abi-platform). Codex.
14. **File Stage 6 leaves (4 issues, Rust-specific concept hub gaps).**
    concept:lifetime-bound, concept:pattern-match-destructure verify,
    macro-expansion-as-pre-substrate doc, deferred thread-safety. Codex
    and architect ratification.

## 4. BLOCKED-ON-ARCHITECT (needs Sir's call)

### 4.1 #1068 Trinity exhibit Path B: real-toolchain ruling

PR #1161's audit `docs/audits/2026-05-18-1068-real-toolchain-ruling-options.md`
enumerates four options. Per that audit, **Option 2 (slow-test lane)** is
already in production CI: `implementations/rust/provekit-cli/Cargo.toml:55-64`
declares the `slow-tests` feature; `.github/workflows/ci.yml:920-1015`
provisions Python 3.12 + JDK 21 + Rust stable + real plugins in the
`Trinity composition census (slow lane)` job. The bash-script fake-realizer
precedent (`lower_kit_path_integration.rs:38-58`) is on-policy under A5 for
unit tests of substrate-internal code but explicitly insufficient for the
Trinity federation assertion 4.

The ruling unblocks #1068, which in turn unblocks #1081. Recommend Sir
read PR #1161 and rule Option 2 unless his read of A5 differs.

### 4.2 #1158 Phase B2: Rust boundary set width

Per #1158 body: "Should Rust declare exception / closure / iterator /
reference as boundaries? Currently narrower than Java/Python."

PR #1164 §4 frames this independently and recommends promoting 5 of 7
Rust sugar-carriers (dynamic-dispatch, closure, iterator,
generic-instantiation, reference) to boundary. The architect-call cited
in #1158 maps directly onto #1164's Stage-1 dispatch. Recommend Sir
ratify #1164 §4's recommendation; the ratification IS the unblock for
#1158 Phase B2 AND for #1164 Stage 1.

### 4.3 trinity_roundtrip's fake plugin (sub-call within #1158)

Per #1158 B2: "trinity_roundtrip's fake plugin: upgrade the test stub at
`trinity_roundtrip_test.rs:106-110` to invoke real Python kit emission?
Or keep as fake for fast unit-test discipline? (Related: #1068 real-
toolchain ruling.)" Bundle this with the #1068 ruling; same trichotomy.

### 4.4 File-then-dispatch decision for #1164's 44 sub-issues

Meta-question: do the 44 inventoried sub-issues get filed as GitHub
leaves now, or wait? Recommend file immediately under the appropriate
phase umbrellas (#897, #898, mostly). The architect-call is purely
"yes/no on filing"; the inventory has already done the typing work.

## 5. BLOCKED-ON-IMPLEMENTATION

- **#1148 Stream A Stage 5: CI gate**, blocked on **#1147 Stream A Stage 4**
  (in flight per triage doc; PR #1155 cited in #1162 body as the landed
  vehicle, but issue #1147 still OPEN as of this audit).
- **#1162 Stage 3.1 SQLite-API extension**, blocked on **#1147**. Body
  explicitly cites "PR #1155 (Stage 4: per-callsite migration point-query)
  added the substrate-driven divergence walk" but #1147 is still OPEN;
  reconcile before classifying #1162.
- **#1081 A13 composition test**, blocked on **#1068** (Trinity exhibit
  Path B runnable command).
- **#983 Vendor .proof publish + consume**, blocked on **#982** (#982 is
  ready in Wave 2).
- **#993 Adoption viral loop**, blocked on **#983 + #992**.
- **#1023 Recursive concept composition**, IN-FLIGHT (codex worktree per
  triage doc). Not blocked; just not classified as READY because there
  IS an active worktree.

## 6. DEFERRED (out of scope for current cycle)

- **#984 Generalize migrate-trigger to vendor-version-upgrade.** The
  primitives exist; the work is "uses existing migrate + RefusalMemento."
  Defer until at least one vendor .proof use case forces it (a deferred
  pull, not a planned push).
- **Phase 8 / 9 (#901 / #902, spec catalog + CDDL in CCL).** Both depend
  only on Phase 2 (closed) so technically dispatchable now, but the
  effort/value ratio favors landing Phase 4 close-out + Phase 5/6 first.
  File leaves H1-H6 + I1-I4 in a later wave.
- **Phases 10-17 (#903-#910).** All depend on later phases. Wait.

## 7. Recommended next dispatch wave

Concrete top-5 to dispatch NOW. Justifications cite specific empirical
facts:

1. **#919 + #920 (Phase 3 close-out).** Phase 3 has been one issue away
   from closure since C1 #918 merged. Mint + CI-gate are mechanical.
   Cheap close, satisfying for the integrator: Phase 3 goes green and
   the umbrella can close. Codex parallel. Estimated 1 PR each.
2. **#1158 Phase B1 (Trinity body-template wiring).** The floor-completion
   audit names exactly 14 dispatch slots, all mechanical, all parallel.
   Three-tests-per-variant discipline per
   `feedback_discrimination_tests_per_variant`. Single-issue close-out
   for the most-recently-completed audit. Codex.
3. **#961-#965 (5 D7-debt leaves, parallel).** All five are mechanical
   concept-mint or lift-extension work with no cross-issue dependency.
   These narrow D7's residual loss surface and feed both #943's per-crate
   ascent AND the lift+lower 44-issue arc. Codex parallel.
4. **Architect ruling on #1068 (PR #1161 read-and-decide).** This single
   ruling unblocks #1068 + #1081 + the trinity_roundtrip B2 sub-call.
   The audit recommends Option 2 (slow-test lane); the lane already runs
   in CI. Estimated 15 min of architect read time.
5. **File Stage 1 + Stage 2 + Stage 6 leaves from PR #1164 (18 issues
   total: 5 + 9 + 4).** Once filed, these dispatch in parallel and each
   one is mechanical or architect-ratifiable (Stage 1 needs the architect
   call from item 4's adjacent ruling on #1158 B2 = Rust boundary widening
   = same five concepts as #1164 Stage 1). Kit (issue-write) only; codex
   dispatch follows after architect ratifies Stage 1.

The integrator-level signal: items 1-3 are pure-codex no-architect work
that closes Phase 3 + Phase B1 + most of D7-debt in a single wave. Items
4-5 are architect-gated but cheap. The combined wave converts roughly
20 open issues into closed-or-actively-running within a single coordination
cycle, and sets up the 44-issue lift+lower arc for the following cycle.

## 8. Summary

Phase 1-3 are effectively complete (#911-#918 closed; #919/#920 are the
two-issue residual). Phase 4 is mid-flight at D7 with five D7-debt leaves
all dispatchable in parallel. Phase 5-6 readiness is dominated by PR
#1164's 44-issue inventory (proposal; not yet filed as GitHub leaves)
plus the Trinity completion stack #978 (six of eight stack children
closed; #1023 in flight, #1068 architect-gated). Phases 7-17 stay queued
behind their phase prereqs.

The single biggest leverage point: the architect ruling on PR #1161
(toolchain options for #1068) AND the file-the-44-leaves decision for
#1164. Both are short-cycle, low-effort, and unblock the bulk of the
remaining bootstrap surface.

Supra omnia, rectum. The lift+lower inventory's Stage 7 (the JCS-port
byte-identity test) is the load-bearing correctness gate for the whole
arc; everything before it is plumbing. The plumbing is mostly mechanical
and parallel-dispatchable today.
