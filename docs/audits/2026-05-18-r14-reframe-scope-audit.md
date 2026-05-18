# R14 reframe scope audit on #880 / #884 / #889 / #755

Date: 2026-05-18
Triage source: PR #1156, `docs/audits/2026-05-18-open-issue-triage.md`
Author: T Savo (Kit, on behalf of architect)

## Method

For each of the four umbrellas I:

1. Read the current issue body via `gh issue view`.
2. Cross-checked each scope claim against the R1-R13 + R14 + R14.5 rulings in:
   - `docs/plans/2026-05-17-realization-tag-kinds-and-marketplace-ruling.md`
   - `docs/plans/2026-05-17-r14-floor-ceiling-registration-tiers.md`
3. Cross-checked against the four ratified specs that R1-R13 names as already-in-main primitives:
   - `protocol/specs/2026-05-12-sugar-dict-memento.md` (R4)
   - `protocol/specs/2026-05-15-concept-citation-comment-sugar.md` (R3, fourth tag location)
   - `protocol/specs/2026-05-14-policy-profile-memento.md` (R6)
   - `protocol/specs/2026-05-13-policy-memento.md` (sibling)
4. Compared each umbrella's "Acceptance" and "Scope" sections to the primitives that R14 ratified or introduced (RealizationMemento tagged enum, four tag locations, exam manifest v1.1, R14.5 `fn_name_sugar`).
5. Where the read remains ambiguous, marked `needs-additional-architect-call` rather than guessing.

The classification language (SURVIVES / REFRAME / PARTIAL-OBSOLETE / OBSOLETE) follows PR #1156's reading guide.

## Per-issue audit

### #880 contract-observation

**Original scope**: Umbrella for `concept:contract-observation(callsite_cid, contract_cid, mode)` as the hub concept for runtime observation, with FOUR modes (`witness`, `monitor`, `emitter`, `gate`) and a composition-point taxonomy (`before`, `after-return`, `after-throw`). Acceptance requires the hub term minted CID-pinned, Java + Python emitting a callsite with operation body plus observation wrapper, result-preserving `after-return`, and the four-mode CLI/CID-pinning surface.

**R14-era overlap**:
- R5 ratifies a THREE-shape observation model: **Monitor / Witness / Gate** with explicit memento families (`MonitorMemento`, `WitnessMemento`, `GateMemento`). R5 does NOT name `emitter` as a distinct fourth shape.
- R12 names RealizationMemento as a tagged enum with four VARIANTS (FirstClass, Composition, Boundary, SugarCarrier). The observation tier (Monitor/Witness/Gate) is orthogonal to the tag-kinds tier; this issue does not conflict.
- R3 + R4 reframe how a wrapper is realized: the wrapper concept itself becomes a `concept:` term whose target-surface choice is made by sugar-dict + policy-profile, not by hardcoded body templates.
- `sugar-dict-memento.md` §2.3 still lists FOUR modes (Witness, Monitor, Emitter, Gate) as observation-mode applicability filters. This is a substrate-internal inconsistency with R5.

**Residual**: The hub concept term and the four (or three) mode-tagged composition points are NOT directly covered by any R14-era PR. Composition-point semantics (before / after-return / after-throw) remain unspecified at the substrate level. Result-preserving wrapper semantics remain unspecified.

**Recommendation**: **REFRAME**.

The umbrella survives but with three concrete changes:

1. Reconcile the mode vocabulary. R5's three shapes (Monitor/Witness/Gate) is the architectural ruling; `emitter` as a separate mode is not ratified. The three plausible mappings for `emitter` are: (a) unsigned Monitor that happens to dispatch to a runtime sink (most plausible), (b) a sugar-dict realization of `concept:log-emit` cited from inside a Monitor body, (c) a fourth shape R5 missed. **This needs an architect-call**: marked `needs-additional-architect-call` on the mode-vocabulary reconciliation. The fact that `sugar-dict-memento.md` §2.3 still lists `Emitter` as a fourth mode means whichever way it resolves, that spec must update.
2. Re-anchor the wrapper-realization mechanism on R3 + R4: when a kit has no first-class or composition tag for `concept:contract-observation`, the round-trip carrier is concept-citation-comment-sugar; concrete library wrappers (slf4j, JUnit, Bean Validation) come in as vendor sugar-dict plugins per R4, not as substrate body templates.
3. Acceptance criteria #880's "Java and Python emit one ordinary callsite with operation body plus observation wrapper" remains valid but must be re-stated in terms of R12 tag-kinds: the wrapper realization for each (mode, language) pair is itself a RealizationMemento::Boundary (for library-backed wrappers like slf4j) or RealizationMemento::SugarCarrier (for comment-floor) or RealizationMemento::Composition (for stdlib-only wrappers).

Concrete next step: post an issue comment on #880 noting the R5 reconciliation requirement and pointing at this audit. Do not close.

### #884 log-emit

**Original scope**: Mint `concept:log-emit(level, message, structured_fields)` as a normal concept. Add per-(language, library) realization cells (Java + TypeScript + Python at minimum). Define loss dimensions for level semantics, structured vs formatted fields, sink buffering, context propagation. Recursive body-template substitution engine is explicitly out of scope (delegated to #1023).

**R14-era overlap**:
- R3 ratifies concept-citation-comment-sugar as the fourth tag location for round-trip preservation of concepts that lack native realization.
- R4 ratifies sugar-dict-memento v1.0.0 as the marketplace mechanism: per-library sugar dicts are vendor-authored content-addressed plugins. The example sugar dict in §6.1 is Spring (`@Valid` → concept:non-null). A logger sugar dict for slf4j or pino is the same shape.
- R5's three observation shapes (Monitor / Witness / Gate) are orthogonal to log-emit but constrain where log-emit can appear (Monitor body, typically; sometimes Witness body if signed).
- The recursive citation mechanism is in #1023 (IN-FLIGHT per PR #1156).

**Residual**: `concept:log-emit` itself is not yet minted in `concept-shapes/catalog/`. Reference sugar dicts for Java/TS/Python loggers are not yet authored. The four loss dimensions (level / structured / sink-buffering / context-propagation) are not yet specified.

**Recommendation**: **PARTIAL-OBSOLETE**.

R3 + R4 take ~70% of the original scope by reframing the mechanism: log-emit is a concept (mint it in the catalog) plus per-vendor sugar-dict plugins (Spring/slf4j/pino author their own). The substrate does not need a per-library body-template substitution catalog at all under R4 (sugar dicts ARE the catalog). The umbrella's "minimal catalog/body-template cells for selected target libraries" wording is obsolete; under R4 those become vendor plugins not substrate catalog entries.

Residual after R3/R4 reframe:

- (a) Mint `concept:log-emit(level, message, fields)` in `concept-shapes/catalog/`.
- (b) Define the four loss dimensions formally (this IS substrate work; loss schema applies across all kits).
- (c) Author one reference sugar dict (Java + slf4j) as a worked example, parallel to the Spring example in `sugar-dict-memento.md` §6.1.
- (d) Acceptance criterion "logger-less project can refuse or select an explicit floor" maps to `--strict-sugar` per sugar-dict-memento §5, which is already specified. Acceptance can cite the existing spec rather than re-define.

Concrete next step: rewrite #884 body to drop the per-library body-template language and replace with the (a)-(d) residual. Don't retire the umbrella; the catalog mint, loss-dim spec, and reference plugin are real residual work.

### #889 sugar-selection-policy

**Original scope**: Define and implement a sugar-selection policy memento (or generalized `PolicyMemento` domain with kind `"sugar-selection"`). Policy ranks candidate sugar/body-template entries; inclusive vs best-only vs refuse; uses registered loss-function plugins from #738; produces selected/rejected CIDs with reasons.

**R14-era overlap**:
- **`sugar-dict-memento.md` §4 specifies the entire selection-and-emission algorithm**: §4.1 enumerate matching entries; §4.2 score each candidate via the loaded loss-function-memento; §4.3 rank-and-select with `best-only` vs `inclusive` modes; §4.4 deterministic tie-break; §4.5 emit. §5 specifies strict-mode refusal via `PluginLoadFailureMemento` with `reason_kind = "no-matching-sugar-entry"`.
- **`policy-profile-memento.md` motivation** explicitly names "a sugar selection policy says how loss and mode coverage are judged" as one of the three policy lanes in a profile (alongside witness consensus and emission gating). The `policy_cid` field cites the gate-local policy memento.
- **`policy-memento.md` §8 ("Out of Scope")** says the rule language for `admission_rule`/`refusal_rule` is opaque; gate-specific kinds are listed (Threshold/Property/Signature/HumanAcceptance/ProofGate) but no `SugarSelectionPolicyMemento` kind exists.
- R6 ratifies PolicyProfileMemento as the per-consumer acceptance mechanism.

**Residual**: The selection mechanism #889 asked for is **already specified** by sugar-dict-memento §4 plus loss-function-memento. The policy-profile spec recognizes "sugar selection" as a policy lane, but no concrete `SugarSelectionPolicyMemento` exists as a member of the PolicyMemento family. The gap is whether `sugar-dict-memento.md §4`'s emission policy (`best-only` / `inclusive` / strict-sugar with `--strict-sugar` flag) IS the sugar-selection policy, or whether a separate `SugarSelectionPolicyMemento` is needed that the profile cites by CID.

The conflict resolves either of two ways:

- **A**: emission-policy keywords (best-only / inclusive / strict) ARE the policy, fed into `sugar-dict-memento.md §4` directly. A `SugarSelectionPolicyMemento` would just be a small wrapper holding `{ mode, loss_function_cid, strict: bool }`.
- **B**: A richer `SugarSelectionPolicyMemento` carrying admission/refusal rules per the PolicyMemento family pattern is genuinely needed, and `sugar-dict-memento.md §4` is the mechanism but not the policy.

**Recommendation**: **REFRAME** (not PARTIAL-OBSOLETE).

The mechanism #889 wanted is largely already specified, but the issue's specific deliverable ("Spec lands for sugar-selection policy or generalized PolicyMemento domain named sugar-selection") is NOT specified anywhere. PolicyProfileMemento talks about "sugar selection" as a lane but does not define the policy memento that lane cites. PolicyMemento family does not list a `SugarSelectionPolicyMemento` kind.

The reframed scope:

- (a) Decide A vs B above. Mark `needs-additional-architect-call` on this fork.
- (b) Under A: amend `sugar-dict-memento.md` to formally name the §4 emission policy as `SugarSelectionPolicyMemento` (or document that no separate memento is needed because the policy IS the loaded loss-function-memento CID plus `{best-only | inclusive | strict}`).
- (c) Under B: spec the new `SugarSelectionPolicyMemento` as a kind in the PolicyMemento family, with admission/refusal rules over (concept, language, library_tag) candidates.
- (d) Either way: integration test demonstrating that the same (concept, contract) emits different surfaces under two policies while preserving the concept's CID. This test does not yet exist.

Concrete next step: post #889 comment with the A-vs-B fork and request architect ruling. Do not implement until the fork is resolved.

### #755 runtime-mode emission sugars

**Original scope**: `provekit bind --mode=witness|monitor|emitter|gate` annotates concept callsites with requested observation modes. The realize side composes ordinary operation body + one or more `concept:contract-observation(...)` wrappers selected by policy. Multi-mode emission must be deterministic and recorded. Acceptance: CLI accepts the four-mode vocabulary; gate is not confused with monitor; relift recovers mode tags as structured evidence.

**R14-era overlap**:
- R5 ratifies THREE observation shapes (Monitor/Witness/Gate); `emitter` is not ratified as a fourth.
- R12's RealizationMemento::Boundary covers library-backed wrappers (slf4j-backed log-emit, JUnit-backed witness, Bean Validation-backed gate). R12's RealizationMemento::SugarCarrier covers the comment-floor fallback when no library wrapper exists.
- R6 + PolicyProfileMemento ratify policy-mediated mode selection. The profile's `emission_mode` field already encodes `gate`/`monitor`/`witness`/`emitter` (matching #755's vocabulary, NOT R5's three-shape vocabulary; another substrate inconsistency).
- `sugar-dict-memento.md` §2.3 still references all four modes (Witness, Monitor, Emitter, Gate).

**Residual**: The CLI flag `--mode=...` is not yet wired. Deterministic multi-mode emission ordering is not yet specified. Result-preserving wrapper composition (after-return / after-throw / before composition points from #880) is not yet specified.

**Recommendation**: **REFRAME**.

The umbrella survives but as a child of #880's reframed scope:

1. The mode vocabulary must reconcile with R5. If R5's three-shape ruling stands, `emitter` is folded into Monitor (most plausible reading: unsigned dispatch-to-sink IS a Monitor that writes to a sink rather than a buffered record); `sugar-dict-memento.md` §2.3 and `policy-profile-memento.md` Field Discipline section must update accordingly. This mirrors the architect-call from #880; both issues share the same blocker.
2. The CLI `--mode=...` surface becomes one of: `--mode=monitor|witness|gate` (R5-aligned) or remains four-mode (if architect rules R5 missed a shape). Pick after the architect-call.
3. The multi-mode composition story (inclusive policy emits multiple selected surfaces per callsite, ordering deterministic) belongs in the reframed #880 spec, not separately in #755.

Concrete next step: same architect-call as #880. Treat #755 as a sub-issue of #880's reframed umbrella once the mode vocabulary is locked. Do not close.

## Cross-cutting observations

1. **Mode-vocabulary substrate inconsistency**. R5 says three shapes (Monitor/Witness/Gate). `sugar-dict-memento.md` §2.3 says four (Witness/Monitor/Emitter/Gate). `policy-profile-memento.md` Field Discipline says "gate, monitor, witness, or emitter from local defaults." The interpretive R14-era rulings have not yet propagated to the two specs that name modes. Both #880 and #755 are blocked on this reconciliation, AND the two specs need follow-up edits regardless of how the four umbrellas resolve. This is the single most consequential cross-cutting issue.

2. **PolicyMemento family has a sugar-selection-shaped hole**. PolicyProfileMemento cites "sugar selection" as a policy lane but no member of the PolicyMemento family in `policy-memento.md` §3 is named for sugar selection. Either sugar-dict-memento §4's emission policy IS the policy (in which case the docs should cross-reference), or a new `SugarSelectionPolicyMemento` kind is missing.

3. **PR #1156's preliminary read was directionally right but coarse**. All four umbrellas do survive R14 in some form, but the differentiated diagnoses are:
   - #880: REFRAME (mode vocabulary reconciliation + R3/R4 re-anchoring)
   - #884: PARTIAL-OBSOLETE (R3/R4 take most of it; small concrete residual)
   - #889: REFRAME (the policy lane is named but the policy memento is missing)
   - #755: REFRAME (blocked on same mode-vocabulary as #880)

4. **Two umbrellas share one blocker**. #880 and #755 are gated on the same architect-call (R5 three-shape vs four-mode). One ruling resolves both. Bundle the call.

## Recommended actions

1. **Open an architect-call** on mode-vocabulary reconciliation: "R5 ratifies three observation shapes (Monitor/Witness/Gate). #880, #755, `sugar-dict-memento.md` §2.3, and `policy-profile-memento.md` Field Discipline still reference Emitter as a fourth mode. Pick: (a) fold Emitter into Monitor, (b) keep four modes and amend R5 to four shapes, (c) keep three shapes at R5 with Emitter as a property of a Monitor variant." This unblocks #880 and #755.

2. **Open an architect-call** on #889's A-vs-B fork (sugar-dict-memento §4's emission policy IS the policy vs a separate `SugarSelectionPolicyMemento` kind is needed in the PolicyMemento family).

3. **Rewrite #884's body** to drop the per-library body-template language, retain (a) mint concept:log-emit, (b) define the four loss dimensions, (c) author one reference sugar dict (Java/slf4j), (d) cite sugar-dict-memento §5 for the no-logger refusal path. Do not close.

4. **Update PR #1156's `docs/audits/2026-05-18-open-issue-triage.md`** to reference this audit and replace the "preliminary read" with the differentiated diagnoses above. (Out of scope for this commit; flag as follow-up.)

5. **Spec-level cleanup** (independent of the umbrellas, but exposed by them): once the mode vocabulary is locked, amend `sugar-dict-memento.md` §2.3 and `policy-profile-memento.md` Field Discipline accordingly. This is a small docs PR.

6. **Do NOT** retire any of the four umbrellas. All four have real residual scope. Closure for any is premature pending the two architect-calls above.

## Open items flagged `needs-additional-architect-call`

- #880 + #755 shared: mode-vocabulary reconciliation (R5 three-shape vs four-mode)
- #889: A-vs-B fork on whether sugar-dict-memento §4 IS the sugar-selection policy or whether a separate `SugarSelectionPolicyMemento` kind is needed
