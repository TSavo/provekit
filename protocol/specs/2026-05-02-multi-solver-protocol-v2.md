# Multi-Solver Protocol (v2)

**Status:** v1.4.0 normative draft
**Date:** 2026-05-02
**Catalog property:** Listed in the v1.4.0 catalog as `multi-solver-protocol-v2`; CID is computed from this file's bytes per `2026-04-30-protocol-catalog-format.md` §2.1 (raw-byte BLAKE3-512).
**Owner:** verifier crate.
**Supersedes:** `2026-04-30-multi-solver-protocol.md` (v1, listed in the v1.3.0 catalog at `multi-solver-protocol`, CID `blake3-512:71fc7ac22997938629d835f87e4e8a322026d77c1e1f834c9fbe0f79cca4e903792c628e96d3004c88d29706f4d87bc042ff837fef571c0cb3012495a03003d3`). The v1 spec remains valid for any v1.3.0-or-earlier verifier; v1.4.0 verifiers running in `coverage_required` consensus mode require v2 conformance.

## §0. The protocol is the bytes

The multi-solver verdict is composed from a pool of SolveResult envelopes. Each envelope is structured data with a defined byte form. The composition rules in this spec operate on those bytes; two conformant verifiers in two languages, given the same pool of SolveResult envelopes, produce the same composed verdict and the same provenance record.

If this English text says one thing and a conformant verifier's bytes say another, the bytes win and the English is updated.

## §1. Why v2 exists

The v1 multi-solver protocol composed solver verdicts under four execution modes: `Single`, `Chain`, `Portfolio { first-wins }`, `Portfolio { consensus }`, `Dispatch`. Consensus mode required all definitive verdicts to agree on the FOL fragment. It had no concept of *partial competence*: a solver that returned `Discharged` after silently skipping a position it could not soundly translate looked indistinguishable from a solver that handled the whole formula.

The IR-compiler-protocol/2 spec (`2026-05-02-ir-compiler-protocol-v2.md`) closes that gap on the compiler side: every v2 compiler emits an `OpacityManifest` declaring which IR positions it could not soundly translate. This v2 multi-solver spec closes the gap on the verifier side: a new plan mode, `Portfolio { consensus, coverage_required: true }`, refuses to compose a verdict unless every opaque position any solver reported is *covered* by some other solver in the pool that did NOT need to mark it opaque AND returned `Discharged`.

The IR is unchanged. The IR knows nothing about Coq, opacity, or solver capabilities. **Each compiler is the authority on what its theory can soundly handle.** The verifier's authority is composition, not translation.

This spec defines:

1. The new `Portfolio { consensus, coverage_required: true }` execution mode (§4).
2. The verdict-composition rule that consumes OpacityManifests (§5).
3. The SolveResult envelope grammar including the manifest field (§6).
4. The configuration shape extending v1's `[solvers]` block with `coverage_required` (§7).
5. The backward-compatibility positioning relative to v1 modes (§8).

The OpacityManifest's grammar, canonicalization, position content-addressing, and worked example live in `2026-05-02-opacity-manifest-grammar.md`. The compiler's emission requirement lives in `2026-05-02-ir-compiler-protocol-v2.md`. This spec consumes both.

## §2. Inheritance from v1

Everything in `2026-04-30-multi-solver-protocol.md` carries forward unchanged unless this spec says otherwise:

- The `[solvers]` configuration grammar in `.provekit/config.toml`, extended in §7 with one new field.
- The execution modes `Single`, `Chain`, `Portfolio { first-wins }`, `Portfolio { consensus }` (now equivalent to `Portfolio { consensus, coverage_required: false }`), `Dispatch`.
- Stub solver shorthands.
- Per-fragment dispatch heuristics.
- Per-solver IR-compiler dispatch via the `ir_compiler` tag.
- Per-solver implication-memento provenance.
- The `min_solver_witnesses` trust knob.
- The `TierStats` report schema.

The single substantive change is the introduction of `coverage_required` and the new verdict-composition rule that activates when it is set to `true`.

## §3. Terminology

- **Solver pool.** The ordered list of solvers configured under `[solvers] portfolio = [...]`.
- **SolveResult.** The envelope a single solver returns: a verdict plus the OpacityManifest emitted by that solver's IR compiler.
- **Verdict.** One of `Discharged`, `Unsatisfied`, `Undecidable`, `Disagreement` (the v1 enum). v1.4.0 adds no verdict cases; coverage failures surface as `Undecidable` with a structured reason.
- **OpacityManifest.** The structure defined in `2026-05-02-opacity-manifest-grammar.md`. Each entry has a `positionCid` and a `reasonCode`.
- **Coverage union.** Across a pool's SolveResults, the set of distinct `positionCid`s that appear in any solver's manifest.
- **Coverage.** A `positionCid` is *covered* by solver S iff S's manifest contains NO entry with that `positionCid` AND S's verdict is `Discharged`.
- **`coverage_required`.** A boolean knob on `Portfolio { consensus }` mode that, when `true`, requires every position in the coverage union to be covered by at least one solver in the pool.

## §4. Execution modes (v2)

### §4.1 Modes inherited unchanged from v1

| Mode | v2 semantics |
|---|---|
| `Single` | Invoke one solver; its verdict is the call site's verdict. No coverage check. |
| `Chain` | Sequential fall-through; first definitive verdict wins. No coverage check. |
| `Portfolio { first-wins }` | Parallel; first definitive verdict wins. No coverage check. |
| `Portfolio { consensus }` | Equivalent to `Portfolio { consensus, coverage_required: false }`. Parallel; ALL definitive verdicts must agree on the FOL fragment. No coverage check. |
| `Dispatch` | Inspect formula theory; pick matching solver. Single-solver dispatch. No coverage check. |

In all of the above, OpacityManifests in SolveResult envelopes are recorded for telemetry and provenance but do NOT participate in verdict composition. v1 and v2 compilers can be mixed freely.

### §4.2 New mode: `Portfolio { consensus, coverage_required: true }`

The defining mode of v1.4.0. Sibling to v1's `Portfolio { consensus }`; activated by setting `coverage_required = true` in the `[solvers]` block.

**INVARIANT (mode admission):** When `coverage_required = true`, every solver in the pool MUST be backed by an IR compiler reporting `protocol_version = "ir-compiler-protocol/2"` at handshake time. The verifier MUST reject the pool configuration before any solve attempt if any compiler in the pool is v1-only. Rejection surfaces as a configuration error (`config_error.coverage_requires_v2_compilers`), not as a runtime verdict.

This is **scorched earth, no back-compat**, by design: composing partial competence requires every compiler in the pool to declare what it cannot soundly handle. A v1.3.0-or-earlier solver cannot make that declaration; admitting it to a `coverage_required` pool would defeat the soundness rule the mode exists to enforce.

The execution rule:

1. Spawn every solver in the pool against the formula in parallel (per the v1 portfolio shape; cancellation semantics inherit from v1's open follow-up).
2. Collect each solver's SolveResult envelope `(verdict_i, manifest_i)`.
3. Apply the verdict-composition rule (§5).

## §5. Verdict-composition rule

Given a pool `S = { s_1, ..., s_n }` with SolveResults `(verdict_i, manifest_i)`:

```
ConsensusCoverage(formula, S) :=
  // Step 1: reject any pool member with a missing or v1-shaped manifest.
  if ∃ i : manifest_i is absent or not ir-compiler-protocol/2
    then Undecidable("coverage_required: solver s_i lacks v2 manifest")

  // Step 2: any concrete refutation kills the consensus.
  if ∃ i : verdict_i = Unsatisfied
    then Unsatisfied  // formula is concretely refuted; no coverage check needed

  // Step 3: existing v1 disagreement check, applied per-position.
  if ∃ i, j : verdict_i ∈ {Discharged, Unsatisfied} ∧
              verdict_j ∈ {Discharged, Unsatisfied} ∧
              verdict_i ≠ verdict_j
    then Disagreement(by_solver = ...)

  // Step 4: compute the coverage union.
  let opacityUnion = ⋃_{i=1..n} { entry.positionCid | entry ∈ manifest_i.opacities }

  // Step 5: every opaque position must be covered.
  if ∃ p ∈ opacityUnion :
       ¬∃ i ∈ {1..n} :
            verdict_i = Discharged ∧
            ¬∃ entry ∈ manifest_i.opacities : entry.positionCid = p
    then Undecidable("coverage_required: position <p> uncovered by any solver in pool")

  // Step 6: at least one solver must have returned Discharged.
  if ¬∃ i : verdict_i = Discharged
    then Undecidable("coverage_required: no solver discharged")

  // Step 7: consensus succeeds.
  return Discharged with provenance:
    - per_solver_results: { (s_i, verdict_i, manifest_i) }
    - coverage_union: opacityUnion
    - coverage_assignments: { p ↦ s_i  |  for each p, the lexicographically-first solver
                                          in the pool that covered p }
```

**INVARIANT (composition soundness):** When `ConsensusCoverage(formula, S) = Discharged`, every position in the coverage union was reasoned about by at least one solver in the pool that did not need to mark it opaque AND that solver returned `Discharged`. The composition is sound: every part of the formula was soundly discharged by some pool member.

**INVARIANT (per-position disagreement orthogonality):** The disagreement check in step 3 applies to the *whole formula* between any two solvers that both returned definitive verdicts. The coverage rule in step 5 applies *per-position*. The two are orthogonal: a coverage gap fails as `Undecidable`, a disagreement fails as `Disagreement`. A pool can fail both checks; the v2 verifier reports both signals to telemetry, and the call-site verdict is the one that fails first in step order (Step 2 > Step 3 > Step 5 > Step 6).

**INVARIANT (coverage-assignment determinism):** The `coverage_assignments` map records, for each position in the coverage union, the *first* solver in the pool's configured order that covered the position. Two verifiers consuming the same SolveResult pool with the same configured order produce byte-identical `coverage_assignments` records.

### §5.1 Why step 1 rejects rather than skips

A v1.3.0-or-earlier solver in a `coverage_required` pool is a misconfiguration, not a runtime ambiguity. The verifier MUST refuse to run rather than silently proceed (e.g., by treating the missing manifest as `opacities: []`). Treating absence as emptiness would let an unsound v1 compiler claim sound coverage of a position it never reasoned about.

### §5.2 Why step 2 rejects on Unsatisfied

If any solver returns `Unsatisfied`, the formula is concretely refuted (the solver found a counterexample). Continuing to a coverage check would only confirm the refutation. The verdict is `Unsatisfied` with provenance; the disagreement check of step 3 still flags the case where another solver returned `Discharged` for the same formula (a SOLVER DISAGREEMENT in v1's sense).

### §5.3 Why empty `opacities` is sound

If every solver in the pool returns `(Discharged, opacities: [])`, the coverage union is empty, step 5 is vacuously satisfied, step 6 finds at least one `Discharged`, and the verdict is `Discharged`. This is the "every solver translated everything soundly and discharged the formula" case; v2 reduces to v1 consensus when no opacity is reported by any solver.

## §6. SolveResult envelope grammar

Every solver invocation under any v2 mode produces a SolveResult envelope, JCS-canonicalized for inclusion in the verdict provenance record:

```ebnf
SolveResult ::= "{"
                  "\"opacityManifest\"" ":" OpacityManifest ","
                  "\"solver\"" ":" SolverTag ","
                  "\"verdict\"" ":" Verdict ","
                  "\"wallClockMs\"" ":" Number
                "}"

Verdict     ::= "\"Discharged\"" | "\"Unsatisfied\"" | "\"Undecidable\"" | "\"Disagreement\""

SolverTag   ::= "\"" SolverName "@" SolverVersion "\""    // e.g., "z3@4.13.0"
```

**INVARIANT:** Every SolveResult MUST carry `opacityManifest`, `solver`, `verdict`, and `wallClockMs`. Additional fields MAY appear; v2 verifiers ignore unknown fields. The keys appear in JCS-sorted order (`opacityManifest`, `solver`, `verdict`, `wallClockMs`) when canonicalized.

**INVARIANT (manifest nesting):** The `opacityManifest` field's bytes inside the SolveResult envelope are byte-identical to the same manifest's bytes when canonicalized in isolation. JCS canonicalization is structural; already-canonical sub-objects survive an outer canonicalization pass unchanged. See `2026-05-02-opacity-manifest-grammar.md` §7.

The verdict provenance for a `Portfolio { consensus, coverage_required: true }` discharge is the array of SolveResult envelopes for every solver in the pool, plus the `coverage_union` and `coverage_assignments` fields produced by step 7 of §5. This array becomes part of the implication memento minted for the call site (the v1 per-solver memento rule generalizes; one consensus memento now references every contributing SolveResult).

## §7. Configuration shape

Extends v1's `[solvers]` block with one new field, `coverage_required`:

```toml
[solvers]
mode = "consensus"
coverage_required = true                 # opt-in to v2 coverage check; default false
portfolio = ["z3", "cvc5", "coq"]

[solvers.z3]
ir_compiler = "smt-lib-v2.6"
binary = "z3"

[solvers.cvc5]
ir_compiler = "smt-lib-v2.6"
binary = "cvc5"

[solvers.coq]
ir_compiler = "gallina"
binary = "coqtop"
```

`coverage_required` is OPTIONAL. Default value is `false`. When unset or `false`, the verifier uses v1 consensus semantics (all definitive verdicts must agree on the FOL fragment; no coverage check). When `true`, the verifier applies the §5 ConsensusCoverage rule.

**INVARIANT (default is v1):** Within v1.4.0, the default behavior of `Portfolio { consensus }` is byte-for-byte the v1 behavior. Operators opt in to v2 coverage by setting `coverage_required = true` explicitly.

`coverage_required = true` requires `mode = "consensus"`. Any other combination (`coverage_required = true` with `mode = "first-wins"`, etc.) is a configuration error. The verifier refuses to start with `config_error.coverage_required_requires_consensus_mode`.

## §8. Backward-compatibility

**v1 modes unchanged.** `Single`, `Chain`, `Portfolio { first-wins }`, `Portfolio { consensus, coverage_required: false }`, and `Dispatch` continue to work exactly as in v1, with v1 or v2 compilers freely mixable. The OpacityManifest, when emitted by a v2 compiler, is recorded for telemetry but does not affect the verdict.

**v1 compilers excluded from `coverage_required` pools.** Per §4.2 admission rule, a `coverage_required = true` pool rejects v1 compilers at startup. The opt-out is to drop `coverage_required` and run v1 consensus, in which case the v1 compiler participates normally.

**v2 verifiers consuming v1 pools.** A v2 verifier configured with `coverage_required = false` (or unset) is operationally identical to a v1 verifier on the same configuration. Operators may upgrade verifiers without upgrading any solver until they explicitly opt in to v2 coverage.

**v1 verifiers consuming v2 SolveResults.** A v1 verifier consuming a SolveResult emitted by a v2 compiler ignores the `opacityManifest` field. The verdict and provenance the v1 verifier records are identical to what it would record for a v1 SolveResult; the manifest's bytes are dead weight in the envelope. This is forward-compatible by design.

**Catalog migration.** The v1.4.0 catalog cut (separate spec work) decides whether `multi-solver-protocol-v2` enters the catalog as a sibling of `multi-solver-protocol` (this spec's draft assumption) or replaces it. Either way, the v1 spec's CID remains valid in any older catalog that referenced it.

## §9. Telemetry and reporting

`TierStats` extends v1's schema with a coverage section. The new fields are additive:

```rust
pub struct TierStats {
    // v1 fields (unchanged)
    pub discharged_by_hash: usize,
    pub discharged_by_cache: usize,
    pub vacuous_discharge: usize,
    pub solved_and_minted: usize,
    pub residue: usize,
    pub violations: usize,
    pub disagreements: usize,
    pub solver_invocations: usize,
    pub per_solver: BTreeMap<String, SolverStats>,

    // v2 additions
    pub coverage_required_runs: usize,           // calls under coverage_required = true
    pub coverage_holds: usize,                    // ... that produced a Discharged verdict
    pub coverage_gaps: usize,                     // ... that failed step 5
    pub coverage_v1_compiler_rejections: usize,   // pools rejected at admission per §4.2
    pub opacity_positions_seen: usize,            // size of coverage_union, summed
    pub opacity_positions_covered: usize,         // positions covered, summed
}
```

The v1 `disagreements` counter continues to count step-3 disagreements (unchanged). v2 introduces `coverage_gaps` for step-5 failures and `coverage_v1_compiler_rejections` for §4.2 admission failures. The counters are independent: a single call site can register a disagreement, a coverage gap, or both, and the call-site verdict reports the first failure in step order.

The verifier's report includes a per-call-site coverage breakdown when `coverage_required = true`: the coverage union, the coverage assignment, and (on failure) the uncovered positions with the reason codes any solver attached to them. Operators use this to decide which solver to add to the pool to close the gap.

## §10. Acceptance

This spec is satisfied by:

- `cargo build --release -p provekit-verifier` succeeds with the v2 modes.
- `cargo test --release -p provekit-verifier` passes (v1 tests green plus new tests for `coverage_required` admission, the §5 composition rule, and the §7 configuration error paths).
- Integration test `tests/multi_solver_modes_v2.rs` exercises:
  - The §8 worked example: SMT marks two opacity positions; Coq marks none; Coq returns `Discharged`; consensus holds.
  - The "coverage gap" case: SMT marks one position; no other solver in the pool covers it; consensus fails as `Undecidable("coverage_required: position <p> uncovered")`.
  - The "v1 compiler in v2 pool" case: a stub v1 compiler in a `coverage_required` pool causes startup rejection.
  - The "Unsatisfied short-circuits coverage" case: one solver returns `Unsatisfied`; the coverage union is computed for telemetry but the verdict is `Unsatisfied`.
- The v1.4.0 demo at `examples/multi-solver-coverage-demo/` runs end-to-end with stub v2 compilers and produces a `Discharged` verdict via coverage.
- The v1 multi-solver demo at `examples/multi-solver-demo/` continues to pass byte-for-byte (v1 modes unchanged).

## §11. Open follow-ups

- **Disagreement memento role.** Inherited from v1 §9. The `verdict-disagreement` memento envelope shape is still TBD. v2 adds a peer concern: a `coverage-gap` memento envelope that signs a position-uncovered failure for downstream auditors. New schema CID, new memento `kind = "coverage-gap"`. Deferred to a later spec.
- **Subprocess cancellation.** Inherited from v1 §9. `coverage_required` mode benefits from cancellation slightly less than `first-wins` (every solver's verdict is needed for the coverage union), but a solver returning `Unsatisfied` could short-circuit the rest per §5.2.
- **Coverage-aware pool sizing heuristics.** Once we have empirical data on which compilers cover which opacity-reason classes, the verifier could suggest pool augmentations ("your pool has 3 unhandled `dependent_type` positions; consider adding a Lean solver"). Future telemetry-driven feature; not normative.
- **Transitive coverage.** If solver A covers position p_1 but marks p_2 opaque, and solver B covers p_2 but is unable to compile to handle p_1 in the same call, the pool has no joint solution. v2 does not attempt to split the formula across solvers; future "formula-fragmentation" mode could.

## §12. Related specs

- `2026-05-02-opacity-manifest-grammar.md` — the manifest's byte-level shape and position content-addressing.
- `2026-05-02-ir-compiler-protocol-v2.md` — the compiler-side emission requirement that produces the manifests this spec consumes.
- `2026-04-30-multi-solver-protocol.md` — the v1 spec this v2 supersedes. CID `blake3-512:71fc7ac22997938629d835f87e4e8a322026d77c1e1f834c9fbe0f79cca4e903792c628e96d3004c88d29706f4d87bc042ff837fef571c0cb3012495a03003d3` per the v1.3.0 catalog.
- `2026-04-30-handshake-algorithm.md` — the three-tier discharge model. v2's per-solver mementos (inherited from v1 §5) flow into the same Tier-2 cache.
- `2026-04-30-memento-envelope-grammar.md` — implication memento shape; per-solver provenance is unchanged in v2.
- `2026-04-30-protocol-catalog-format.md` — the rule by which this spec's CID is computed.
- `2026-04-30-canonicalization-grammar.md` — JCS canonicalization, normative for SolveResult envelopes.
