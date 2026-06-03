# GOAL: provekit proves provekit, for real (the durable north star)

This is the living goal. Any session or agent picking up the self-application
work starts here. It defines what "a fully working product" means, the single
metric that tracks it, the dependency-ordered path, and the discipline every
change must obey. Update it as state moves; do not let it go stale.

## The promise

provekit is a verifier PRODUCT a real vendor deploys against their existing
code: a TypeScript shop, a Rust shop, a Java shop, a Go shop points provekit at
its production codebase and gets correctness their existing unit tests cannot
reach with ZERO source modifications. No rewrites, no annotations, no proof
scaffolding added to the target project. The proof that it is real and not a
demo is that it proves ITSELF (snake-eats-tail on `provekit-cli` and
`libprovekit`) AND the federation surface holds across the four kits that ship
today (Python, TypeScript, Go, Java) with one language-blind verifier.

Contracts can live anywhere in the code. Per-language lifters mean contracts
can be formed by any ecosystem-native library or framework surface a project
already uses: Creusot, Spring, JUnit, Zod, serde derives, struct tags, and the
rest of the language's ordinary vocabulary. Vendors do not learn provekit's
contract language first; provekit's lifters learn the vendor's ecosystem.

"Fully working" =
1. **Sound and honest.** Every call site is enumerated, categorized, and either
   discharged with a real proof, refused as honestly undecidable (with the
   reason), or named as residue. Always-on invariants: silently-dropped = 0,
   false-pass = 0, no partial function labeled "total" to inflate a number.
2. **Substantive.** On provekit's own code, K (provably panic-safe sites) covers
   the provable categories via SOUND reasoning, not a fixture and not a vacuous
   label. The reasoning tiers that close the real buckets are shipped.
3. **Observable and self-checking.** The target verdict is one command
   (`provekit self-check`), pinned by a golden in CI, with `doctor` validating
   kit wiring up front and `provekit release-gate` producing the v1 evidence
   receipt. Progress is a number you watch move; regressions scream with a
   readable diff.
4. **Self-describing.** The wiring is executable knowledge (`doctor` + the
   runbook), not tribal knowledge or stale prose.
5. **Cross-language.** The same substrate proves Python, TypeScript, Go, and
   Java. Each new language is cheaper than the last. Verifier knows nothing
   about which language it is verifying.
6. **Vendor-deployable.** A vendor with an existing TypeScript / Rust / Java /
   Go codebase and existing unit tests points provekit at their project,
   installs it cleanly per ecosystem (`cargo install`, `npm i -g`, Maven
   plugin, `go install`), and the verifier discharges enough sites with ZERO
   source modifications for the output to materially differentiate from "your
   tests pass." That means K rises into the hundreds on a real production
   target, not single digits. Vendor onboarding is documented, the
   differential vs unit tests is concrete, and at least one OSS project per
   language is verified end-to-end as the pitch artifact.

Rust v1 status as of 2026-06-01: criteria 1-4 are satisfied for
`provekit-cli` and `libprovekit` by #1787. Criterion 5 (cross-language)
substantially landed across 2026-06-01 through 2026-06-02 via slices 14-28
(panicLoci emission across Java/Go/TS, 4-language kit_declaration parity at
#1865, federation hardening through #1883). Criterion 6 (vendor-deployable)
is now the active arc; see Phase 5.

## Where we are (2026-06-01, Rust v1 done)

### Phase 0 - SCAFFOLDING - DONE

- `self-check --json` on production crates: shipped.
- Golden snapshot in CI: shipped (`provekit-cli/tests/self_check_golden.rs`
  under conformance gate; readable diff on regression).
- `provekit doctor` HARD-fails (exit 2) on the manifest method/phase footgun
  (#1742); language-blind via plugin-self-declared `consumer_surfaces`.
- No-silent-failure floor PRs (recent arc):
  - **#1747** panic-locus preservation + guard-branch routing (the mechanism
    for sound panic-safe discharge: callsite-scoped producer-post resolution,
    content-gated `panic-safe` tag).
  - **#1750** fail-closed `panicLoci` extraction in cmd_mint (silent-drop on
    malformed input became a loud Err naming the field and offending type).
  - **#1752** (closes #1748) multi-line receiver/unwrap line preservation
    (walk_rpc emits producer.start_line; verifier untouched).
  - **#1753** (closes #1751) convergent oracle harness with K=3 stable-pass
    gate; tracing-to-file with stderr clean.
  - **#1755** (closes #1754) mid-run `.provekit/imports/` mutation guard
    (catches concurrent bcargo/rsync wiping deps; loud Err with symmetric
    diff).
  - **#1756** (#1749 walk envelope surface) threads `panic_loci` through the
    provekit-walk single-contract envelope mint path and keys `EnvelopeCache`
    by `contract_cid` plus panic-loci fingerprint, so header provenance cannot
    silently stale or disappear.
  - **#1758** (closes #1749) threads `panic_loci` through the provekit-lift
    direct mint path, including the CID prepass and real mint pass; malformed
    loci fail closed, coalescing preserves all distinct provenance, and the
    stale self-check golden is refreshed with baseline evidence (#1757 tracks
    why the stale golden reached main).
- Plugin subprocess stderr inherits by default (the `Stdio::null` that hid
  load-bearing bugs is gone); counted `warn!` on missing callsite provenance;
  tracing throughout, not eprintln.

### Phase 1 - SOUNDNESS - DONE

- **#1717**: opaque-sorted `forall` encoded soundly (or refused). Detector
  `forall x:<opaque>. false` is undecidable, not collapsed to `true`. CLOSED.

### Phase 2 - SUBSTANTIVE K - DONE

- **Mechanism proven on fixture** (stage3-serde-totality-fixture, warm-oracle
  e2e, regressed-and-confirmed across #1752):
  `to_string(&Value).unwrap()` discharges **panic-safe**;
  `to_string(&MyStruct).unwrap()` stays undecidable (refuse-floor intact).
  dischargeSplit `{panicSafe:1, falsePass:0, silentlyDropped:0}`.
- **Production K measurement on `provekit-cli`** (convergent harness, K=3
  stable, 2026-06-01):
  - `silentlyDropped=0, falsePass=0, droppedSites=[]` (hard floor HELD).
  - `panicSafe=0, panicCensus=32` (mechanism not yet applied to production).
  - `(attempted, resolved) = (3764, 3358)` stable across 3 passes; the 406
    unresolved receivers are honest oracle ceiling (generics, dyn dispatch,
    macros), not cold-pass artifacts.
- **D-lib through D-fn slice queue complete through #1771.**
  - **PR-A (#1759, MERGED 2026-06-01):** Result::expect partial in rust-std
    shim + walk disambiguation `(result, expect) -> result_expect`. Verifier
    untouched. f_expect fixture e2e proves end-to-end composition with the
    existing #1747 panic-safe discharge path (warm-oracle convergent harness
    K=3, dischargeSplit `{panicSafe:2, falsePass:0, silentlyDropped:0}`).
    Infrastructure for PR-C; no current production K delta.
  - **PR-B (#1760, MERGED 2026-06-01):** libprovekit rust-implications
    consumer enablement.
    `.provekit/lift/rust-implications/manifest.toml` + config.toml entry.
    Wires libprovekit self-check to enumerate its own callsite obligations.
    Warm-oracle baseline: `requested=true, engaged=true, attempted=3012,
    resolved=2706`; `dischargeSplit={falsePass:0, panicSafe:0,
    reflexive:665, undecidable:1518, vacuous:154}`;
    `panicCensus=15`; `silentlyDropped=0`, `droppedSites=[]`. Baseline
    scoreboard measurement only; no K delta.
  - **PR-C (#1762, MERGED 2026-06-01):** per-type infallibility totality for
    the 4 confirmed libprovekit D-lib sites. Per-crate
    `infallible_serialize.toml` manifest, walk_rpc disambiguation extension
    for per-crate concrete types, lift_implications lookup for blessed types.
    Discrimination triplet held. Cross-crate Sort blessing (audited_for_crate
    metadata for external types). Result on libprovekit self-check:
    `panicSafe=4`, dischargeSplit `{panicSafe:4, falsePass:0,
    silentlyDropped:0}`, floor invariants intact. **First real production K
    delta on production code: +4.**
- **D-lib `&Value` for provekit-cli (PR-D, #1765, MERGED 2026-06-01).**
  Closes the 2 `kit_dispatch.rs` `&Value` sites. Bundles four structural
  fixes surfaced during this slice:
  - **A**: verifier bundle provenance keyed by containing-contract bundle,
    not global symbol map (fixes cross-target discharge of imported sites).
  - **B**: verifier `enumerate_callsites` now enumerates from `panicLoci`
    metadata, not just pre/post/inv (previously, panic sites with no
    pre/post/inv obligation were silently invisible).
  - **C-thin**: dep proof flow via RPC for serde_json shim; auditable,
    manifest-driven; rust kit `resolve_dependency_proofs` wired.
  - **Claim envelope metadata persistence**: `bodyDischargeEligible` /
    `bodyDischargeRefusalReason` survive mint -> reload (silent-degradation
    floor fix; axiom claims no longer lost on persist round trip).
  Result on provekit-cli self-check: `panicSafe=6` (+2 kit_dispatch + 4
  libprovekit imported via cross-target discharge), floor invariants intact.
  **Cumulative production K so far: 6 sites discharged via sound reasoning
  on real production code.**
- **C `json!` construction tracking for provekit-cli (#1767, MERGED
  2026-06-01).** Rust kit tracks explicit local `json!({ ... })`
  construction facts and emits guarded postcondition terms for known string
  field `as_str().unwrap()` sites. The verifier stays language-blind:
  `cmd_protocol` discharges via kit-emitted `cf_guarded(...)` terms, while the
  existing producer-bridge path still handles the 2 `kit_dispatch` `&Value`
  sites. Result on provekit-cli self-check after syncing `provekit-realize-rust`:
  `dischargeSplit={falsePass:0, panicSafe:13, reflexive:1009,
  undecidable:1799, vacuous:875}`, `silentlyDropped=0`, `droppedSites=[]`,
  `panicCensus=53`; the 7 `cmd_protocol.rs` sites and the 2 `kit_dispatch.rs`
  sites are proven. **Cumulative production K so far: 13 sites discharged via
  sound reasoning on real production code.**
- **B guarded panic partial propagation (#1769, MERGED 2026-06-01).** Rust kit
  lifts intra-function guard facts for `assert!(x.is_some()/is_ok()/is_err())`
  and `len()==1 -> into_iter().next().unwrap/expect()` into guarded panic
  partial obligations. Self-check dependency mints now inherit `--oracle`, so
  local dependency proofs carry the same receiver-type disambiguation as the
  target; packaged external proofs remain proof bytes. The verifier stays
  language-blind and now refuses a panic locus with no scoped bridge target
  instead of falling back to a global same-symbol body contract. Result on
  libprovekit self-check: `panicSafe=10`, `falsePass=0`,
  `silentlyDropped=0`, `droppedSites=[]`, `panicCensus=36`. Result on
  provekit-cli self-check: `panicSafe=19`, `falsePass=0`,
  `silentlyDropped=0`, `droppedSites=[]`, `panicCensus=53`; the +6 over
  post-#1767 is 5 B prelude/std-shim sites plus imported `libprovekit`
  `wp.rs:295` (`len()==1 -> next().unwrap()`). **Cumulative production K so
  far: 19 sites discharged via sound reasoning on real production code.**
- **D-fn cross-function postconditions (#1771, verified
  2026-06-01).** Rust kit reads audited
  `.provekit/contracts/function_postconditions.toml` entries for
  `Cid::parse(format!("blake3-512:{}", "0".repeat(128)))` and
  `ConceptOpCatalog.cid(CONCEPT_BIND_RESULT)`, emits singleton postcondition
  contracts, and routes manifest-backed panic receivers through fixed std
  partials (`is_ok` -> Result, `is_some` -> Option). The verifier remains
  language-blind: it consumes opaque producer coordinates and generic
  singleton `P(result)` facts, with no Rust predicate vocabulary hardcoded.
  Result on libprovekit self-check: `panicSafe=12`, `falsePass=0`,
  `silentlyDropped=0`, `droppedSites=[]`, `panicCensus=35`,
  `oracle={requested:true, engaged:true, attempted:2740, resolved:2504}`.
  Result on provekit-cli self-check: `panicSafe=21`, `falsePass=0`,
  `silentlyDropped=0`, `droppedSites=[]`, `panicCensus=53`,
  `oracle={requested:true, engaged:true, attempted:3496, resolved:3410}`.
  **Cumulative production K so far: 21 sites discharged via sound reasoning
  on real production code.**
- **Residue declaration (#1775, MERGED 2026-06-01).** Rust kit reads
  target-scoped `.provekit/residue.toml` entries and emits
  `panic-site-annotation` lift diagnostics. `self-check` joins those
  diagnostics into the panic census and fails closed on stale, duplicate, or
  proven-site annotations. Result on libprovekit self-check:
  `panicSafe=12`, `falsePass=0`, `silentlyDropped=0`, `droppedSites=[]`,
  `panicCensus=35`, with 1 `platform_semantics_runtime_residue` row. Result
  on provekit-cli self-check: `panicSafe=21`, `falsePass=0`,
  `silentlyDropped=0`, `droppedSites=[]`, `panicCensus=53`, with 8
  `lock_poisoning_residue` rows and 1 closeable `D-lib` tier-to-close row for
  `RealizeRequest` serialization. #1773 tracks cross-target propagation of
  annotations as proof mementos.
- **Phase 5 reproducible K re-baseline (#1896, MERGED 2026-06-02).** The
  Phase 5 baseline now uses the same reproducible infrastructure for the clean
  main baseline and the slice under test: `bcargo`, battleaxe
  rust-analyzer on stable 1.96.0, oracle enabled, and default self-check
  convergence. On that infrastructure, clean main before #1896 reported
  `panicSafe=12`, `falsePass=0`, `silentlyDropped=0`, `droppedSites=[]`,
  `panicCensus=54`, `bridges.emitted=2818`, and
  `oracle={attempted:4160, resolved:4066}`. #1896 added the
  provekit-cli-local Rust-kit `infallible_serialize.toml` entry for
  `serde_json::to_value(RealizeRequest)` and removed the matching residue row.
  Current main after #1896 reports `panicSafe=13`, `falsePass=0`,
  `silentlyDropped=0`, `droppedSites=[]`, `panicCensus=54`,
  `bridges.emitted=2819`, and the same `oracle={attempted:4160,
  resolved:4066}`. The normalized panic-census delta is exactly one row:
  `src/kit_dispatch.rs:2416 method:expect` moves from the closeable D-lib
  tier-to-close bucket to proven. Earlier provekit-cli K=21 / panicCensus=53
  references are retained as historical measurements from a different setup,
  not the current Phase 5 baseline.

### Phase 3 - RESIDUE NAMED + V1 RELEASE - DONE

- **Residue declaration ledger:** #1775 names the target-scoped residue,
  #1776 updates the GOAL ledger for residue, and #1778 records the doctor
  scope/design that makes the release gate auditable.
- **Doctor + release-gate arc:** #1779 refactors doctor around a reusable
  engine, #1780 adds `DoctorMode`, #1781 validates dependency-proof state
  consistency, #1782 validates oracle host readiness, #1784 checks annotation
  runtime consistency, #1785 aggregates the no-silent-failure floor, and
  #1787 adds `provekit release-gate`.
- **Rust v1 executable claim:** `provekit release-gate --json` runs
  `doctor --mode releaseGate` and `self-check --json` on `provekit-cli` and
  `libprovekit`, aggregates the receipt, and exits 0 only when the floor and
  target evidence are green. The v1 git tag itself is a separate release-policy
  decision; the evidence command now exists.

## Current reproducible census (provekit-cli, Rust self-application)

Phase 5 K movement is measured on reproducible self-check infrastructure:
`bcargo`, battleaxe rust-analyzer on stable 1.96.0, oracle enabled, and
default self-check convergence. As of #1896, the `provekit-cli` target reports
`panicCensus=54`, `panicSafe=13`, `falsePass=0`, `silentlyDropped=0`, and
`droppedSites=[]`. Clean main before #1896 was K=12 on the same
infrastructure; #1896 moved exactly one closeable D-lib row,
`src/kit_dispatch.rs:2416 method:expect`, from unproven to proven.

The #1774 reproducibility caveat is closed: the release gate validates the
dependency-proof state as part of doctor release-gate mode before accepting the
v1 K claim. The older K=21 / panicCensus=53 figure came from a different
measurement setup and is not the current Phase 5 baseline.

| Category | Count | Closing mechanism |
|---|---|---|
| proven K | 13 panic-safe | Current reproducible K after #1896. D-lib, C `json!`, B guarded-panic, and D-fn tiers remain the closing mechanisms, but K is now reported against the reproducible bcargo + battleaxe RA baseline. |
| residue | 8 honest residue | Mutex `.lock().expect()` rows. Honest residue; "lock is total" would be unsound. |
| closeable tier-to-close | 0 named D-lib rows | #1896 closed the `RealizeRequest` serialization row with provekit-cli-owned per-type D-lib manifest coverage, mirroring libprovekit's `infallible_serialize.toml` pattern. |
| raw unproven | 33 | Still honest unproven rows in the panic census. They are not labeled panic-safe, not silently dropped, and not allowed to inflate K. |

The honest read: Rust v1 is not K == N. It is K covering the provable buckets,
residue named, hard floor never violated, and doctor/release-gate green.

## Current census (libprovekit, Rust v1 release gate)

PR-B (#1760) enabled the rust-implications consumer surface for libprovekit and
captured the production baseline PR-C measures against. Cold numbers are not the
baseline: the cold run had oracle off by invocation mistake and produced
`panicCensus=27`, all receiver-type unresolved. The warm run is the real
comparison point.

Warm baseline:
- `oracle={requested:true, engaged:true, attempted:3012, resolved:2706}`
- `bridges.emitted=2344`
- `liftGaps={no-contract-for-callee:2860, panic-site-unproven:3,
  unsupported-macro-callsite:423}`
- `dischargeSplit={falsePass:0, panicSafe:0, reflexive:665,
  undecidable:1518, vacuous:154}`
- hard floor held: `silentlyDropped=0`, `droppedSites=[]`

| Category | Count | Closing mechanism |
|---|---|---|
| D-lib | 4 | serde_json totality for `RealizedSource`, `Sort`, `Dialect`, `Term`. These are PR-C's confirmed production K delta targets. |
| B | 5 | Intra-fn fact flow: rust-std shim guards (`assert!(opt.is_some()/result.is_ok())` before unwrap/expect) plus `len==1 -> next().unwrap()`. |
| D-fn | 2 | Cross-function postconditions: catalog primitive `.cid()` and `Cid::parse` on literal. |
| residue | 1 | `platform_semantics_for_lower_target("python").expect(...)`: filesystem/config loading invariant. |
| oracle-residue | 3 | Receiver did not resolve to a known panic partial: two compose.rs rows plus one wp/tests.rs direct row. |
| unknown | 0 | Every warm row has a named category. |

Expected PR-C K delta on libprovekit's self-check: **+4** confirmed D-lib
sites. The B and D-fn rows are later tiers, not PR-C.

Post-#1769 current libprovekit score: `panicCensus=36`, `panicSafe=10`,
`falsePass=0`, `silentlyDropped=0`, `droppedSites=[]`; the B guarded-panic
slice contributes 6 current K rows here: 5 prelude/std-shim rows plus
`wp.rs:295` (`len()==1 -> next().unwrap()`), on top of the 4 D-lib sites.

Post-#1771 main libprovekit score: `panicCensus=35`, `panicSafe=12`,
`falsePass=0`, `silentlyDropped=0`, `droppedSites=[]`; the +2 rows are
`src/core/bind.rs:550` (`ConceptOpCatalog.cid(CONCEPT_BIND_RESULT).expect(...)`)
and `src/wp/tests.rs:74`
(`Cid::parse(format!("blake3-512:{}", "0".repeat(128))).expect(...)`).

Post-#1775 main libprovekit score: `panicCensus=35`, `panicSafe=12`,
`falsePass=0`, `silentlyDropped=0`, `droppedSites=[]`; the
`src/core/platform_semantics.rs:42` row is now explicitly
`platform_semantics_runtime_residue` with tier `irreducible`.

Rust v1 release-gate score: `panicCensus=36`, `panicSafe=12`,
`falsePass=0`, `silentlyDropped=0`, `droppedSites=[]`; the receipt records
12 proven panic-safe rows, 1 honest residue row, and 23 raw unproven rows.
The floor remains intact.

## The metric (the one number we watch)

`provekit self-check` over provekit's own crates emits, deterministically:
- enumerated callsites, **silently-dropped = 0** (hard invariant)
- discharge split: panic-safe **K**, reflexive (labeled), vacuous, undecidable
  **M**, **false-pass = 0** (hard invariant)
- panic census: each unproven site -> { category, tier-that-would-close-it }

Progress = K rising as tiers ship, M shrinking toward the honest residue, the
invariants never violated. "Fully working" is K covering the provable buckets
with the residue NAMED, not K == N.

## The path (dependency-ordered)

### Phase 0 - SCAFFOLDING - DONE
See "Where we are."

### Phase 1 - SOUNDNESS - DONE
- #1717: CLOSED.

### Phase 2 - SUBSTANTIVE K - DONE
Each tier ships as one PR, golden-pinned, with visible scoreboard delta.

- **D-lib per-type for libprovekit** (in progress; three-PR split):
  - **PR-A (MERGED, #1759):** Result::expect partial in rust-std shim + walk
    disambiguation `(result, expect) -> result_expect`. Verifier untouched;
    f_expect fixture e2e confirms end-to-end composition with #1747's
    panic-safe path. Infrastructure for PR-C; no current K delta.
  - **PR-B (MERGED, #1760):** libprovekit
    `.provekit/lift/rust-implications/manifest.toml` + `config.toml` entry.
    Enables libprovekit self-check to enumerate its own callsite obligations.
    Warm-oracle baseline: `panicCensus=15`, confirmed D-lib=4, B=5,
    D-fn=2, residue=1, oracle-residue=3, unknown=0. Baseline scoreboard
    only; no K delta.
  - **PR-C (MERGED, #1762):** per-type infallibility for 4 audited
    libprovekit types (`RealizedSource`, `Sort`, `Dialect`, `Term`). +4 K
    delta on libprovekit's self-check. First real production K via sound
    reasoning.
- **D-lib `&Value` for provekit-cli (MERGED, #1765):** 2 kit_dispatch sites
  discharge via &Value totality. Bundles 4 structural fixes (bundle
  provenance, panicLoci enumeration, dep proof RPC flow,
  bodyDischargeEligible metadata persistence). +2 K delta on provekit-cli.
- **C `json!` construction tracking (MERGED, #1767):** closes the 7
  cmd_protocol.rs sites. The Rust kit tracks local `json!` construction facts
  and emits `cf_guarded(is_some(...), ...)` for known string field unwraps.
  +7 K delta on provekit-cli; cumulative production K=13.
- **B intra-fn `assert!` / iterator-length propagation (MERGED, #1769):**
  closes 5 prelude/std-shim sites plus the imported libprovekit `wp.rs:295`
  `len()==1 -> next().unwrap()` site. Also hardens self-check dependency mints
  to inherit `--oracle` and adds the verifier no-scoped-bridge guard. +6 K
  delta on provekit-cli; cumulative production K=19.
- **D-fn cross-function postconditions (MERGED, #1771):** closes the 2
  remaining D-fn sites (`Cid::parse` on literal, catalog primitive `.cid()`).
  +2 K delta on the earlier provekit-cli measurement, reaching historical
  cumulative production K=21 before the Phase 5 reproducible baseline was
  reset.
- **Residue declaration (MERGED, #1775):** target-scoped Rust-kit
  `.provekit/residue.toml` entries annotate panic census rows without
  changing discharge. 8 provekit-cli Mutex poisoning sites become
  `lock_poisoning_residue`; the `RealizeRequest` serialization site became
  an explicit D-lib tier-to-close row, then #1896 closed it with
  provekit-cli-local infallible serialization metadata; the libprovekit
  `platform_semantics` site becomes `platform_semantics_runtime_residue`.

### Phase 3 - RESIDUE NAMED + V1 RELEASE - DONE
- Residue naming is DONE in #1775: the honest residue sites get an explicit
  `residue` category in the panicCensus output (not raw "unproven"; honest
  residue with reason). The closeable `RealizeRequest` row was named here and
  closed later by #1896.
- The doctor + release-gate arc is DONE:
  - #1779 reusable doctor engine.
  - #1780 `DoctorMode`.
  - #1781 dependency-proof state consistency.
  - #1782 oracle host readiness.
  - #1784 annotation runtime check.
  - #1785 floor aggregation.
  - #1787 v1 release-gate command.
- Rust v1 is executable: `provekit release-gate --json` validates
  `provekit-cli` and `libprovekit` through doctor release-gate mode and
  self-check. A git tag remains a separate release-policy decision.

### Phase 4 - SUBSTRATE PROMOTION + CROSS-LANGUAGE PARITY - DONE (2026-06-02)

The architectural thesis is empirically demonstrated. Four languages (Python,
TypeScript, Go, Java) ship with kits that lift to ProofIR, declare their
capabilities through a shared `provekit.plugin.kit_declaration` RPC, and route
through one language-blind Rust verifier. The substrate vocabulary
(`concept:panic-freedom.leaf.runtime-failure-site`) federates at the metadata
layer across all four kits.

Completed across slices 14-28 (2026-06-01 through 2026-06-02):
- **Slice 14-15 (#1850, #1849, #1851, #1852)**: Java source kit emits
  explicit-throw panicLoci; Java RPC dispatch repaired; bcargo Java sync.
- **Slice 16 (#1854)**: Go source kit emits explicit-panic panicLoci with
  intra-package shadow discrimination via `types.Builtin`.
- **Slice 17a (#1855, #1856)**: bcargo provisions TypeScript env
  (BCARGO_TYPESCRIPT_ENV) including root pnpm install + emitter + realize kits.
- **Slice 17b (#1857, #1861)**: TS source kit emits explicit-throw panicLoci
  with __proto__ refusal predicate.
- **Slice 18 (#1859, #1863)**: TS lifter supports object literal as ProofIR
  term; shorthand-vs-PropertyAssignment __proto__ semantics handled correctly.
- **Slices 19-23 (#1866, #1867, #1868, #1870, #1872 - umbrella #1865 closed)**:
  Six-RPC-entry kit_declaration parity across Python source, TS source, Go
  source, Java source, Go verify-facing, Java core. Each kit declares its own
  empirical capability per honesty-gradient discipline (#856). Manager-level
  proofResolution.strategy convention (pip / npm / go-mod / maven).
- **Slice 24 (#1873)**: Java source + Go verify-facing version normalized to
  manifest "0.1.0-draft" via per-kit VERSION constants.
- **Slice 25 (#1864 closed)**: Test harness ETXTBSY safe-write pattern
  (sync_all + fd drop before chmod) + structural regression pin.
- **Slice 26 (#1862 closed)**: tsc --noEmit gate restored via tsconfig module
  node20 / moduleResolution nodenext; per-test 30s timeout boy-scout.
- **Slice 27 (#1878 closed)**: Doctor enforces per-kit manifest/runtime
  version consistency (`kit-declaration-version-consistent` check).
- **Slice 28 (#1883)**: Doctor manifest collection widens beyond [[plugins]]
  to authoring lift surfaces via capabilities predicates (no hard-coded kit
  names; manifest-driven).

Open mechanical follow-ups (Phase 4 residue, not Phase 5 gates): #1885 (Go
implications surface mismatch), #1886 (self-contract aliases expose
kit_declaration), #1887 (stale TS [authoring] surface normalization).

### Phase 5 - VENDOR-DEPLOYABLE VERIFICATION DEPTH - STARTING

The architectural foundation holds. The product shift now is from "we proved
the thesis works cleanly across four languages" to "a TypeScript / Rust /
Java / Go vendor wants to deploy this because pointing it at their existing
code reveals correctness their unit tests cannot reach with ZERO source
modifications."

Proof and materialization are separate surfaces. The proof product runs on
unmodified source: kits lift language-native evidence, the verifier consumes
normalized ProofIR, and correctness is discharged without edits to the target
project. Sugar and boundary annotations are downstream helpers for
materializing sugar under proven correctness (federation, cross-language emit,
and boundary realization). They are not prerequisites to proving correctness.

Phase 5 has two parallel levers:
1. **Shim catalog expansion.** Library partials and totality facts close panic
   sites through sound discharge tiers (`rust-std`, Java standard library, Go
   standard library, ecosystem crates and packages).
2. **Ecosystem-native lifter coverage.** Lifters mine contracts from whatever
   contract-bearing surface the language ecosystem already uses: Creusot,
   Spring, JUnit, Zod, serde derives, class-validator decorators, struct tags,
   and ordinary test frameworks. These contracts may live anywhere in the
   codebase; they are not centralized in provekit-specific files.

The number that moves: K (panic-safe sites discharged via sound reasoning) on
real production code in each language. Today on Rust self-application, the
reproducible Phase 5 baseline is K=13 on provekit-cli after #1896; libprovekit
last documented K=12. Earlier provekit-cli K=21 references came from a
different measurement setup and are not the current baseline. On Python / TS /
Go / Java production targets: not yet measured at the K level; each language
emits panicLoci but discharge tier infrastructure is Rust-only today. A vendor
running today sees mostly "I don't know" verdicts. To shift to "I want to
deploy this," the K-per-language number has to be in the hundreds on a real
codebase, the output has to be actionable, and the value differential vs unit
tests has to be visible.

**Phase 5 staging (T direction 2026-06-02): dogfood is the gold standard,
then third-party parity per language.**
1. **Dogfood depth first.** Drive Rust self-application K into vendor-
   meaningful range (current reproducible K=13 -> into the hundreds on `provekit-cli` and
   `libprovekit`) via shim catalog expansion and discharge tier work. This
   is the proof that the technique scales on a real codebase before we ask
   anyone else to point it at theirs.
2. **Third-party Rust OSS project parity.** Once dogfood "feels eaten" by
   architectural judgment (K depth, residue clearly named, hard floor
   intact), pick one popular OSS Rust crate. Prove it end-to-end with the
   same techniques. Document the differential vs its unit test suite as the
   pitch artifact: "your existing tests pass; provekit additionally proved
   N invariants across all inputs without you changing a line."
3. **Per-language repeat (TS, Java, Go).** Apply the same dogfood-then-
   third-party pattern to each remaining language. Each language gets its
   own discharge tier infrastructure brought to vendor-meaningful K on a
   self-application target, then a third-party OSS project proves the
   parity story for that language ecosystem.

The gate from step 1 to step 2 is soft architectural judgment, not a
numeric trigger. "Dogfood is gold standard" means provekit's own scoreboard
is the example the vendor pitch points to; "next project on" means once
that bar holds, the next project is a third-party in the same language,
and the pattern repeats per language.

**Rust depth expansion (continues the original tier campaign):**
- Multiply rust-std shim catalog: extend coverage of Result/Option methods,
  iterator length idioms, common std partials, panic-implication propagation
  across function boundaries.
- Lift ecosystem-native Rust evidence already in code, including ordinary
  unit tests, derives, serde shape evidence, and crate-local contract surfaces.
- Cross-crate audited shims for top ecosystem crates (serde, tokio, sqlx,
  reqwest, clap, anyhow, thiserror, others). Per-crate
  `infallible_serialize.toml` and equivalent manifests mature into a
  catalog.
- Target: K from 21 to 100+ on `provekit-cli`, K from 12 to 50+ on
  `libprovekit`, with ZERO source modifications.
- **Pitch artifact**: pick one popular OSS Rust crate, prove it end-to-end,
  publish the differential vs its unit test suite as the vendor onboarding
  story.

**TypeScript depth expansion:**
- Add a TS discharge tier (currently emits panicLoci, has no closure logic).
- Handler-aware closure: `throw` dominated by a `try/catch` that catches the
  thrown type is decidable. This is the substrate decision tracked at #750;
  under standing delegation, kit-local structural support lands first
  (reversible), federated concept lifts only when the kit-local pattern has
  empirical legs.
- Type-narrowed throws: `throw new Error(...)` after a type guard is
  decidable; `Promise.reject` handling distinguished from sync throw.
- ts-shim catalog: common Promise idioms, Array/Object access patterns,
  React/Node ecosystem partials.
- TS lifter coverage for ecosystem-native contract surfaces already present in
  source, including Zod schemas, class-validator decorators, assertion
  libraries, and test frameworks.
- Target: nonzero K on a real OSS TypeScript project (e.g., a small library
  used in the wild).

**Java + Go depth expansion (parallel pattern):**
- Discharge-tier pattern proven on Rust ports to Java and Go.
- Java: `catch (Exception e)` closures; checked exception flow; Spring
  null-safety metadata already present in source as type-level totality
  evidence; JUnit tests as ecosystem-native contract sources.
- Go: `defer`/`recover` handling; `error`-return idiom as the canonical
  Go safety pattern; struct tags and test assertions as ecosystem-native
  contract sources.
- Target: discharge story parity across all four languages.

**Substrate decisions under standing delegation:**
- The substrate calls (#750 try/catch handler awareness, #1858 async
  Promise-vs-throw semantics, #1839 verifier CallSite collapse, #1880
  federation initialize capabilities shape) are inside this campaign. They
  are not parked architecture committees; they are discharge-tier
  infrastructure dressed up as substrate questions.
- Discipline (per advisor 2026-06-02): default to the reversible branch.
  Kit-local structural first; federated concept later when binding evidence
  arrives. Each call gets stamped "decided under standing delegation
  2026-06-02, T ratify-or-reverse on return." Honesty-gradient (#856) still
  binds: no identity-valid-but-behavior-inert mints.

**Vendor onboarding (after depth proves valuable on real code):**
- One-command install per language ecosystem.
- Vendor-facing docs: "point at your project, here's what you get, here's
  the differential vs your existing tests."
- Concrete OSS-project pitch artifacts per language.

**The Phase 5 metric:** K per language on a real production target. Hard
floor invariants unchanged: `silentlyDropped = 0`, `falsePass = 0`,
`droppedSites = []`. K rising via sound discharge tier closures, not
labeling.

## The discipline (every change obeys these; the lessons of 2026-05-30)
1. **Golden-pinned**: if it changes a discharge number, it updates the golden
   with a one-line why.
2. **Observable**: it emits a loud, structured, counted signal; no silent
   drop/refuse/fallback. Concurrent harness mutation is loud (#1755);
   malformed input is loud (#1750); cold oracle either loops to convergence
   or fails loud (#1753).
3. **Language-blind verifier**: no language-specific names in the CLI or
   verifier; semantics live in the kit; refuse-floor preserved per language.
4. **Refuse-floor**: never a vacuous or false "cannot panic"; unproven stays
   honestly undecidable.
5. **CID-not-name**: identity is content CID; name is opaque sugar /
   resolution index.
6. **Self-describing**: new wiring is validated by `doctor` and documented in
   the runbook.
7. **Scaffolding before features**: the observability surface exists before
   the thing it observes.

## Non-goals / honest bounds
- K != N. Genuine residue stays unproven, by design and honestly.
- "Make the number go up by labeling a partial function total" is the vacuous
  trap; only sound contracts (a real pre that discharges, or a true
  type-level totality) count. Phase 5 multiplies K via discharge tier closure,
  NOT via labeling.
- Cross-language cycle proof (byte-identical CIDs across languages) is not a
  Phase 5 gate. The Phase 5 win is "a vendor's existing code gets correctness
  output their unit tests miss with ZERO source modifications." Cycle proof is
  architectural validation that lands later, after vendor adoption proves the
  depth story.
- Federation breadth is sufficient at four languages (Python / TS / Go / Java)
  for Phase 5. Adding a fifth language is not a Phase 5 priority; depth in the
  four shipped languages is.
- Annotations as proof prerequisites are out of scope entirely. A vendor
  should get nontrivial K from running provekit against an unmodified
  codebase. Sugar and boundary annotations are post-proof materializers for
  federation, cross-language emit, and boundary realization; they are not
  proof-enabling artifacts. If initial K requires source modifications, the
  catalog / discharge tier is too shallow.
- Provekit-specific contract centralization is the wrong product shape. A
  contract can be formed anywhere ordinary source, tests, framework metadata,
  or library-specific declarations already carry one. Lifters are the
  ecosystem boundary that turns those surfaces into ProofIR.

## Pointers

- **Plan file**: `docs/self-application/PLAN-panic-loci-completion.md`.
  Current slice queue, advisor checkpoints, verification commands.
- **Runbook**: `docs/self-application/KIT-SETUP-AND-SELF-APPLICATION.md`.
  One command: `scripts/self-apply.sh`.
- **Diagnosis**: `docs/self-application/serde-panic-freedom-diagnosis.md`.
  Full arc of the #1747 root-cause hunt.
- **Recent PRs (this arc)**:
  - #1747 panic-locus preservation + guard-branch routing.
  - #1750 fail-closed panicLoci extraction.
  - #1752 (#1748) multi-line emitter fix.
  - #1753 (#1751) convergent oracle harness + tracing.
  - #1755 (#1754) mid-run imports mutation guard.
  - #1756 (#1749 walk envelope) panic_loci threading + EnvelopeCache
    fingerprint key.
  - #1758 (#1749 lift direct) panic_loci threading through provekit-lift
    direct mint.
  - #1759 (D-lib PR-A) Result::expect partial in rust-std shim + walk
    disambiguation mapping. Infrastructure for D-lib per-type slice.
  - #1760 (D-lib PR-B) libprovekit rust-implications consumer enablement +
    warm-oracle baseline scoreboard (`panicCensus=15`, confirmed D-lib=4).
  - #1762 (D-lib PR-C) per-type infallibility for 4 libprovekit types.
    First real production K delta: +4 on libprovekit's self-check.
  - #1765 (D-lib PR-D) `&Value` for provekit-cli + 4 structural fixes
    (verifier bundle provenance, panicLoci enumeration,
    `resolve_dependency_proofs` RPC flow, body-discharge metadata persistence).
    +2 K delta on provekit-cli. Cumulative production K: 6.
  - #1767 (C slice) Rust-kit `json!` construction tracking emits
    `cf_guarded(...)` postcondition terms for known string field unwraps.
    +7 K delta on provekit-cli. Cumulative production K: 13.
  - #1769 (B slice) Rust-kit guarded panic propagation for `assert!` facts and
    `len()==1 -> next()`, self-check dependency mints inherit `--oracle`, and
    verifier no-scoped-bridge guard. +6 K delta on provekit-cli. Cumulative
    production K: 19.
  - #1771 (D-fn slice) manifest-backed cross-function postconditions for
    `Cid::parse` on literal and catalog primitive `.cid()`. +2 K delta on
    provekit-cli. Cumulative production K: 21.
  - #1775 (residue declaration) Rust-kit target-scoped residue manifest and
    `self-check` panic census annotation join. Names 8 provekit-cli Mutex
    residues, 1 provekit-cli D-lib tier-to-close row, and 1 libprovekit
    platform semantics residue.
  - #1776 GOAL ledger update for residue declaration.
  - #1778 DOCTOR-DESIGN.md scope and release-gate design.
  - #1779 (doctor PR 1) reusable doctor engine.
  - #1780 (doctor PR 2) `DoctorMode` scaffold.
  - #1781 (doctor PR 3) dependency-proof state consistency; #1774 is closed.
  - #1782 (doctor PR 4) oracle host readiness adapter.
  - #1784 (doctor PR 5) annotation runtime consistency check.
  - #1785 (doctor PR 6) no-silent-failure floor aggregation.
  - #1787 (doctor PR 7) `provekit release-gate` command and v1 evidence
    receipt.
- **Open follow-ups**:
  - #1757 self-check golden drift reached main without gate update.
  - #1763 self-check should fail closed when requested oracle host cannot
    start. Doctor release-gate mode checks oracle readiness; the self-check
    command behavior remains tracked separately.
  - #1764 cross-crate type totality should live in owning-crate contracts
    (currently project-local with `audited_for_crate` metadata).
  - #1766 self-check should fail closed when a configured
    `resolve_dependency_proofs` RPC binary is missing. Doctor release-gate mode
    validates dependency-proof state; the self-check command behavior remains
    tracked separately.
  - #1773 promote panic-site annotations into proof mementos so target-scoped
    residue declarations can propagate through dependency `.proof` bundles.
  - #1783 retire legacy lift-diagnostic panic-site annotation join.
  - #1786 update stale `--release-gate` doctor spelling to
    `--mode releaseGate`.
- **Key files**: `provekit-verifier/src/{runner.rs, enumerate_callsites.rs,
  body_discharge.rs, handshake.rs, load_all_proofs.rs}`,
  `provekit-walk/src/{lift.rs, bin/walk_rpc.rs, envelope.rs}`,
  `provekit-cli/src/{cmd_self_check.rs, cmd_mint.rs}`,
  `provekit-ir-compiler-smt-lib/src/generated.rs`,
  rust-std shim `examples/provekit-shim-rust-std`.
- **Dispatch**: Codex (`gpt-5.5`, `model_reasoning_effort=xhigh`), isolated
  worktree, FULLY INLINE briefs (no file refs - they do not survive MCP
  serialization). Coordinator owns all gh/PR ops + review. Standing arc:
  see PLAN-panic-loci-completion.md.

**Cross-language federation arc (2026-06-01 - 2026-06-02, Phase 4 closing):**
- #1850 (slice 14) Java source kit explicit-throw panicLoci emission.
- #1849 (slice 15) Java source RPC dispatch repair (Program flag dispatch +
  SourceRpcServer).
- #1851 bcargo Java sync (standalone infrastructure PR).
- #1852 (slice 15 merge) Java source kit RPC dispatch lands.
- #1854 (slice 16) Go source kit explicit-panic panicLoci with
  `types.Builtin` intra-package shadow discrimination.
- #1855 (slice 17a) bcargo TypeScript env provisioning (BCARGO_TYPESCRIPT_ENV).
- #1856 widening: realize-kit npm ci coverage for ts-better-sqlite3 / ts-pg /
  ts-core after Codex P2 audit gap surfaced.
- #1857 / #1861 (slice 17b) TS source kit explicit-throw panicLoci emission
  with `__proto__` prototype-setter refusal predicate.
- #1859 / #1863 (slice 18) TS lifter ObjectLiteralExpression as ProofIR term
  with shorthand-vs-PropertyAssignment `__proto__` discipline.
- #1865 umbrella (closed by slice 21): six-RPC-entry kit_declaration parity.
  Children: #1866 TS source, #1867 Go source, #1868 Java source, #1870 Go
  verify-facing, #1872 Java core; PRs #1869 / #1871 / #1874 / #1875 / #1877.
- #1873 (slice 24) Java source + Go verify-facing version normalization to
  manifest `0.1.0-draft` via per-kit `VERSION` constants. PR #1879.
- #1864 (slice 25 closed) test harness ETXTBSY safe-write pattern
  (`sync_all` + fd drop before chmod) + structural regression pin. PR #1881.
- #1862 (slice 26 closed) tsc --noEmit gate restoration via tsconfig
  `module: node20` / `moduleResolution: nodenext`; per-test 30s timeout
  boy-scout for vitest. PR #1882.
- #1878 (slice 27 closed) doctor enforces per-kit manifest/runtime version
  consistency (`kit-declaration-version-consistent` check). PR #1884.
- #1883 (slice 28) doctor manifest collection widens to authoring lift
  surfaces via capabilities predicates. Manifest-driven, no hard-coded kit
  names. Closes the slice 27 disclosed coverage gap (4 of 6 advisor entries
  now doctor-enforced).
- Phase 4 follow-ups still open: #1885 (Go implications surface mismatch),
  #1886 (self-contract aliases expose kit_declaration consistently), #1887
  (stale TS `[authoring]` surface normalization to `typescript-source`).

**Phase 5 active arc** starts where Phase 4 lands. Phase 4 hardened the
substrate; Phase 5 multiplies K so vendors deploy.
