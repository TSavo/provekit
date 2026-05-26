# Architecture ground-truth audit — 2026-05-25

**Why:** After a rapid run of architecture changes (lower→emit #1476, the `concept:sql-query` cardinality split #1468, kit-self-resolution of shim `.proof`, ORP-witness-via-emit), the conformance gate had been red long enough that reactive fixing kept guessing at "stale test vs real bug." This audit establishes ground truth so the remaining work is deliberate, not inferred. Citations are `file:line` against the repo at the commit this doc lands on.

## Headline

The architecture shift is **largely already reconciled**, and **every remaining gate-red traces to a single cause**: #1476 retired `lower`, but the menagerie demos (bridgeworks + supply-chain-rails, *including* supply-chain's witness production) were never migrated to `emit`. There is **no separate live-path regression** — the `package_inspection` "ORP witness failed" is the same lower-retirement gap (its `package.no-install-side-effect` witness is still produced by `supply-chain-js-lowerer.rs`, and **no emit witness emitter for it exists anywhere in the repo**). Everything else is fixed, correctly quarantined (reversibly), still-valid, or a latent stale test fixture. **The cross-language conformance suite does NOT need wholesale retirement** — most of it tests the current model.

## 1. Mechanism census

| Mechanism | Status | Evidence / note |
|---|---|---|
| `provekit lower` | **RETIRED** | `cmd_lower.rs` gone; not in `REGISTRY_MANIFEST_KINDS` (`kit_dispatch.rs:84` = `["lift","realize","emit","exam-manifest"]`). **Orphaned artifacts:** `.provekit/lower/**` manifests remain on disk in `menagerie/bridgeworks/checked-add-u8` and `menagerie/supply-chain-rails/authenticated-betrayal/packages/*`. Harmless (nothing dispatches them) but should be cleaned with the eventual demo redesign. |
| `provekit emit` | **LIVE** | `cmd_emit.rs`; `REGISTRY_MANIFEST_KINDS` includes `"emit"`; dispatch in `kit_dispatch.rs`. |
| ORP witness | **LIVE, emit-integrated** | Produced *through* emit: `kit_dispatch.rs:2574` "Emit witness dispatch (ORP witness emitter)" → `cmd_emit.rs:485` (`orp-witness`) → invoked from `cmd_mint.rs:1676` (`emit_witnesses` → `cmd_emit::emit_witness_requirement`). **NOT** retired-in-favor-of-emit — it *is* the emit path. |
| `lift` / `mint` / `materialize` | **LIVE / core** | unchanged core verbs. |
| cardinality concepts (`concept:sql-query-{all,row,iterate}`) | **LIVE; split complete in live code** | Flat `concept:sql-query` remains only in `cmd_mint.rs` test fixtures (`:2221/2235/2257`, inside `mod tests {` at `:2198`) — a **latent stale fixture**, not live mint logic, not currently red. Python sqlite3/aiosqlite kits already migrated (#1497). |
| kit-self-resolution (substrate never reads a shim `.proof`) | **HOLDS — no violation** | Grep of `provekit-cli/src` for `.proof` shows only output *writes* (`{cid}.proof` in `cmd_mint`/`cmd_emit`) and test fixtures; `body_templates_from_shim_proof` / `sugar_proof` are gone. |
| cross-language conformance model (C1-C8) | **LIVE / current** | `make prove-<kit>` = "lift-plugin-protocol conformance (C1-C8 verifiers)" (`Makefile:469-539`) — verifies each kit's live lift RPC protocol against captured messages. Not tied to lower/flat-concepts. |

## 2. Gate-red inventory (root-caused + bucketed)

| Failing/skipped item | Bucket | Disposition |
|---|---|---|
| `bridgeworks` smoke: `checked_add_exhibit_passes`, `all_exhibits_reports_contract_and_implication_mementos` | **(a) tests retired mechanism** (`lower`→C witness) | `#[ignore]`'d reversibly (#1521); bridgeworks demo preserved pending redo-on-emit-or-retire. |
| `supply-chain-rails` smoke: `all_exhibits_show_conventional_green_then_provekit_red` | **(a) tests retired mechanism** (`lower` JS-lowerer) | `#[ignore]`'d (#1521). |
| `provekit-realize-python-core`: `test_emit_compile_run_conformance` | **(c) broken harness** — imports cross-kit `cbor2` (via provekit-lift-py-tests) absent in the kit's isolated venv → collection error | `pytest.importorskip("cbor2")` skip-guard (#1521); runs in full env. |
| `supply-chain-rails` smoke: `package_inspection_contract_set_matches_lifted_mint_contract_set` | **(a) lower→emit migration gap** (reclassified from "real failure" after root-cause) | `provekit mint` fails at `emit_witnesses` → `emit_witness_requirement` for `package.no-install-side-effect`. That witness is produced by `supply-chain-js-lowerer.rs:625/643/653` (a `lower`-era kit); **no emit witness emitter for `install-side-effect` exists anywhere in the repo** (grep-confirmed), and the package has no `.provekit/emit/*` manifest. So mint can't satisfy the obligation via emit. Same family as the other lower-era failures. Disposition: quarantine reversibly with the rest, or fix by writing the emit witness emitter when the supply-chain demo is redone on emit. |
| `cmd_verify_python_production_bridge`: `python_mint_auto_writes_body_discharge_bridge` | **(c) CI-env cascade** | Passes locally (`1 passed`). Expect green once the gate env is clean; recheck on next full run. |
| `cmd_mint.rs` flat-`concept:sql-query` test fixtures (`:2221+`) | latent stale fixture | not currently red; fold into cardinality cleanup. |

## 3. Conformance-suite validity

- **`prove-<kit>` C1-C8** — CURRENT (live lift RPC protocol). Keep. (Was skipped, not failing.)
- `slice2_java_realize_plugin_byte_identical.rs` — CURRENT (0 old-model refs).
- `cross_platform_point_query_receipt_test.rs` — CURRENT (0 old-model refs).
- `stage3_cross_language_test.rs` — **verify**: 1 old-model reference; confirm it's not pinning `lower`/flat-`sql-query`.
- `trinity_*`, `verb_composition`, the census tests — already removed in #1476.

**Conclusion:** the cross-language conformance suite is mostly valid; the only redesign candidate surfaced is the single `stage3` reference. No wholesale retirement warranted.

## Deliberate next actions (daylight)

1. **Supply-chain witness migration (the only real work item):** the `package.no-install-side-effect` witness has no emit emitter — it is still js-lowerer-produced. When the supply-chain demo is redone on emit, write the emit witness emitter (`install-side-effect` surface) so `mint` can discharge the obligation; until then `package_inspection` is quarantined like the other lower-era tests. This is the one piece of genuine new emit work the lower-retirement left behind.
2. Verify `stage3_cross_language_test.rs`'s one old-model reference; re-pin or quarantine if stale.
3. On bridgeworks/supply-chain demo decision: when redoing on emit (or retiring), also delete the orphaned `.provekit/lower/**` manifests.
4. Clean the flat-`concept:sql-query` test fixtures in `cmd_mint.rs` with the cardinality cleanup.
5. Confirm `cmd_verify` greens on the next full gate run (expected — CI-env cascade only).
