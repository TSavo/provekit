# Kit-as-Substrate-Participant Vision: Audit and Roadmap

Date: 2026-05-18
Status: Active. Source of truth for the kit-protocol-correctness work. All future GitHub issues on this thread derive from rows in this audit.

## 1. The Vision

The substrate is a PROTOCOL, not a codebase. Its value is what it standardizes (the wire contract for kits + the content-addressed artifact for libraries), not what it implements in any one place. ProvekIt's wire contract is **PEP 1.7.0 over JSON-RPC** for kit dispatch + **`.proof` bundles** for content-addressed substrate artifacts.

The "anyone can write a new language kit and ship libraries in that language and it just works" property holds because:

- All kits implement PEP 1.7.0 over JSON-RPC. The substrate dispatches via subprocess + stdio. Kit-implementation-language is invisible to the dispatcher.
- All libraries ship one `.proof` bundle alongside their existing distribution (npm, pip, cargo, maven). The substrate resolves library kits via package-manager + .proof read.
- The substrate's `libprovekit` is thin: protocol + dispatcher + primitives + ProofIR + canonicalizer. Language- and library-specific behavior lives in kits, not substrate code.

This audit catalogs:

1. The substrate primitives that ARE substrate-resident (correctly).
2. The kits currently in the codebase, classified by face completion and ownership category.
3. The workflow reinventions where substrate CODE does work the PROTOCOL should be doing.
4. The dissolution roadmap pairing kit-face mints with workflow dissolution PRs.
5. The vision realization milestones leading to the Trinity demo (Rust ↔ Java ↔ Python round-trip with library `.proof` bundles + language exams + all five workflows composing one set of primitives).

## 2. Paper grounding

This vision is documented across the After-X arc:

| Paper | Title | Relevance |
|---|---|---|
| `docs/papers/12-after-languages-how-proofir-represents-every-language.md` | ProofIR as universal IR | The canonical interchange between language kits. |
| `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md` | Languages as content-addressed algebras | Why the kit's lift face produces algebra terms. |
| `docs/papers/16-after-portability-the-universal-address-space.md` | Universal address space via CIDs | Why .proof + CIDs are the load-bearing carrier. |
| `docs/papers/21-after-cross-language-every-cross-x-dissolves.md` | Cross-X dissolution | Why the M+N hub matters: every cross-(lang, library) reduces to compose-of-two-kits. |
| `docs/papers/22-after-vendoring-migration-as-source-transformation.md` | Libraries ship sugar | Why library kits MUST be vendor-distributed via .proof. |
| `docs/papers/23-after-packages-the-proof-envelope-carries-the-binding.md` | .proof envelope binding | The .proof is the package-level binding artifact. |
| `docs/papers/25-after-architectures-the-program-was-already-realized.md` | Architecture endgame | The substrate-as-protocol terminus. |

This audit is not the source of the vision; the papers are. The audit maps the CODE BASE to the vision and identifies where they diverge.

## 3. Substrate primitives (the protocol surface)

The kept-thin substrate code that all workflows compose. These should never grow language- or library-specific branches.

| Primitive | File | Symbol | What it does |
|---|---|---|---|
| Lift dispatch | `implementations/rust/provekit-cli/src/kit_dispatch.rs:504` | `dispatch_bind_lift(workspace_root, source_lang)` | Resolve `kind=lift` plugin for a language via manifest convention. Invoke via PEP 1.7.0 over JSON-RPC. |
| Realize dispatch | `implementations/rust/provekit-cli/src/kit_dispatch.rs:913` | `dispatch_realize(workspace_root, target_lang, library_tag, request)` | Resolve `kind=realize` plugin for `(target_lang, library_tag.unwrap_or("default"))` via manifest convention. Invoke via PEP 1.7.0. |
| Exam dispatch | `implementations/rust/provekit-cli/src/kit_dispatch.rs` | `dispatch_exam_manifest(...)` | Resolve `kind=exam-manifest` plugin. Validates ExamManifestMemento. |
| Language semantics lookup | `implementations/rust/libprovekit/src/core/platform_semantics.rs:34` | `platform_semantics_for_lower_target(target: &str)` | Returns language-kit declaration for a language. Arms: python, rust, java, c, typescript. |
| Binding semantics lookup | `implementations/rust/libprovekit/src/core/platform_semantics.rs:144` | `binding_semantics_for_tag(binding_tag: &str)` | Returns library-kit declaration. Arms today: better-sqlite3, pg. |
| Composition | `implementations/rust/libprovekit/src/core/platform_semantics.rs:122` | `platform_semantics_for_binding(lang, tag)` | Composes language-kit + library-kit via `merge_declarations`. Binding-kit wins op-CID conflicts. |
| Trichotomy verdict | `implementations/rust/libprovekit/src/core/types.rs` | `PlatformSemanticsDeclaration::compare_op_with(op_cid, other)` | Returns four-state `OpCoverageVerdict`: `NoOpinion` / `Uncharacterizable { absent_on }` / `Same` / `Divergent(c)`. The substrate's load-bearing decision primitive. |
| Effect propagation | `implementations/rust/libprovekit/src/effect_propagation.rs` | `propagate_effects(graph)` | Widen / Halt / Refuse propagation through the call graph. Used for async-contagion today; generalizable. |
| BindKit | `implementations/rust/libprovekit/src/core/bind.rs:98` | `BindKit::transform(input)` | Substrate-only algebra pass. Lift IR → applicative-encoded bind-result Term::Op tree. No language/library dispatch. |
| Address | `implementations/rust/libprovekit/src/core/primitives.rs:30` | `address(&value)` | JCS + BLAKE3-512 content-addressing. The universal identity. |
| Canonical encode | `implementations/rust/libprovekit/src/canonical.rs` | `json_cid`, `encode_jcs`, etc. | JCS canonicalization. The wire format. |
| Path executor | `implementations/rust/libprovekit/src/core/path_executor.rs` | `execute_path(path, registry, inputs)` | Composes a sequence of verbs into a chain of DomainClaims. |
| Exam manifest load | `implementations/rust/libprovekit/src/exam_manifest.rs:147` | `load_default_exam_manifest()` | Loads the v1.1 exam manifest. Pins kit set for a run per pin-all-three. |

These primitives ARE the protocol surface. Any workflow that doesn't compose THESE is reinventing.

## 4. Kit faces inventory

A KIT is one substrate participant. Two kinds:

- **Language kit**: owned by substrate (or in the future, language ecosystem). Declares language-native platform semantics + lifts/realizes language-native code.
- **Library kit (sugar)**: owned by library author (or substrate-bootstrap during early adoption). Declares library-specific binding semantics + lifts/realizes library-bound boundary calls.

Each kit has three faces: **lift** (source → ProofIR), **declaration** (dimensions/tags for trichotomy), **realize** (ProofIR → source).

### 4.1 Language kit inventory

| Language | Lift face | Declaration face | Realize face | Classification |
|---|---|---|---|---|
| Python | `implementations/python/provekit-lift-python-source/` | `implementations/rust/libprovekit/src/core/platform_semantics/python_common.rs:1-77` and `python_lift_source.rs` | `implementations/python/provekit-realize-python-core/` | Bootstrap-resident (decl); plugin-resident (lift, realize) |
| TypeScript | TODO: locate ts lift plugin | `implementations/rust/libprovekit/src/core/platform_semantics/typescript.rs:1-212` | `implementations/typescript/provekit-realize-typescript-core/` | Bootstrap-resident (decl); plugin-resident (realize) |
| Java | `implementations/java/provekit-lift-java-source/` | `implementations/rust/libprovekit/src/core/platform_semantics/java.rs:1-142` | `implementations/java/provekit-realize-java-core/` | Bootstrap-resident (decl); plugin-resident (lift, realize) |
| Rust | `implementations/rust/provekit-walk/src/bin/walk_rpc.rs` | `implementations/rust/provekit-realize-rust-core/src/platform_semantics.rs` | `implementations/rust/provekit-realize-rust-core/` | All bootstrap- or plugin-resident |
| C | TODO: locate c lift plugin | `implementations/c/provekit-realize-c-core/platform_semantics.rs` | `implementations/c/provekit-realize-c-core/` | Bootstrap-resident |

### 4.2 Library kit inventory (current)

**Audit miss correction (2026-05-19, post D5a verification per #1238):** The original audit listed body-template JSON files as "exists" without verifying their CONTENT. Empirical probe via #1243 (the D5a verification harness) showed that "file exists" does not imply "face complete." The realize face requires THREE sub-checks: (a) plugin code exists, (b) body-template JSON exists, (c) **entries cover the migrate's actual `(concept_name, param_count, requires_param_types)` shapes**. The original audit verified (a) and (b) but not (c). The matrix below now records (c) status as a separate column.

The harness result for the migrate demo's 12 (callsite, target) probes: 0 MATCHES / 0 COSMETIC / 0 SEMANTIC / 12 MISSING. Failure modes were either `is_stub=true` (TS-pg) or `MissingTemplateError` (Python plugins). The bodies templates that DO exist are keyed for the canonical `(sql, args)` two-param probe shape, not for the migrate's per-function-signature callsite shapes.

| (Language, Library) | Library tag | Declaration face | Body templates exist | Body templates cover migrate's callsites | Realize plugin | Lift plugin |
|---|---|---|---|---|---|---|
| TypeScript + better-sqlite3 | `better-sqlite3` | `platform_semantics/better_sqlite3.rs` (180 lines: RowIdMechanism=LastInsertRowid) | `menagerie/typescript-language-signature/specs/body-templates/typescript-canonical-bodies-better-sqlite3.json` | TODO: verify per D5a harness (source side; not exercised by migrate target dispatch) | `implementations/typescript/provekit-realize-typescript-better-sqlite3/` | TODO: locate |
| TypeScript + pg | `pg` | `platform_semantics/pg.rs` (191 lines: RowIdMechanism=ReturningClause) | TODO: locate body templates JSON | **NO — plugin returns is_stub=true for the migrate's actual callsite signatures (4/4 callsites MISSING per #1243)** | `implementations/typescript/provekit-realize-typescript-pg/` | TODO: locate |
| Python + sqlite3 | `sqlite3` | `platform_semantics/python_sqlite3.rs` (landed via #1240) | `menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-sqlite3.json` (has entries for concept:sql-query + concept:sql-execute with `signature_guard: {min_params: 2, max_params: 2}` keyed for the canonical (sql, args) probe shape) | **NO — entries require 2 params; migrate's callsites have per-function signatures (e.g., `getUserById(id: int)` is 1 param). 4/4 callsites MISSING per #1243** | `implementations/python/provekit-realize-python-sqlite3/` (352 lines) | TODO: locate |
| Python + aiosqlite | `aiosqlite` | `platform_semantics/python_aiosqlite.rs` (landed via #1241) | `menagerie/python-language-signature/specs/body-templates/python-canonical-bodies-aiosqlite.json` | **NO — same shape as python-sqlite3. 4/4 callsites MISSING per #1243** | `implementations/python/provekit-realize-python-aiosqlite/` | TODO: locate |
| Python + requests | `requests` | TODO: verify | TODO: locate | TODO: verify per the harness | `implementations/python/provekit-realize-python-requests/` | TODO: locate |

### 4.3 Library kit inventory (Trinity demo target — missing)

For the Rust ↔ Java ↔ Python Trinity demo Sir specified, the following library kits must be minted:

| (Language, Library) | Library tag | Status | Notes |
|---|---|---|---|
| Rust + rusqlite (or similar) | `rusqlite` | **NOT STARTED** | Required for Trinity demo's Rust leg. Mechanism: `Connection::last_insert_rowid()` (similar shape to better-sqlite3 but Rust API). |
| Java + sqlite-jdbc | `sqlite-jdbc` | **NOT STARTED** | Required for Trinity demo's Java leg. Mechanism: PreparedStatement.getGeneratedKeys() + ResultSet. |
| Python + sqlite3 | `sqlite3` | **PARTIAL** | Realize plugin exists; declaration face missing. |

### 4.4 Language exam inventory

Language exams (ExamManifestMemento) pin which kits participate in a substrate run. Per `2026-05-18-pin-all-three` memory and `exam_manifest.rs`, the exam is load-bearing for substrate-honesty.

| Language | Exam manifest | Status |
|---|---|---|
| Default (cross-language) | `menagerie/concept-shapes/exams/v1.1.blake3-512:<cid>.json` (loaded via `DEFAULT_EXAM_MANIFEST_JSON`) | Exists |
| Rust-specific | TODO: confirm whether a Rust-only language exam exists | UNCLEAR |
| Java-specific | TODO: confirm | UNCLEAR |
| Python-specific | TODO: confirm | UNCLEAR |
| TypeScript-specific | TODO: confirm | UNCLEAR |
| C-specific | TODO: confirm | UNCLEAR |

If only one default cross-language exam exists today, the Trinity demo requires per-language exams (per Sir's "language exams for all 3 languages") OR confirmation that the default exam suffices.

## 5. Workflow reinventions catalog

For each CLI verb, audit whether it composes substrate primitives or reinvents them.

### 5.1 cmd_bind_migrate.rs — five reinventions identified

| Reinvention | File:line | Substrate primitive it should consume |
|---|---|---|
| `TargetSurface` enum (`PythonSqlite3 \| PythonAiosqlite \| TypescriptPg`) hardcoded | `cmd_bind_migrate.rs:230-260` | `dispatch_realize(target_lang, target_tag, request)` already abstracts target dispatch. The enum reinvents target selection. |
| `requires_async_delta(self) -> bool` hardcoded per-target | `cmd_bind_migrate.rs:267` | Should be a declared dimension per binding-kit (AsyncMode or equivalent). Trichotomy handles like RowIdMechanism. |
| **`render_migrated_source(target_surface)` — 280+ lines of HARDCODED FIXTURE OUTPUT** | `cmd_bind_migrate.rs:1392-1578+` (the function dispatches to three sub-functions, each a multi-paragraph raw-string-literal) | **THE CENTRAL REINVENTION**: the migrate command's source emission is not synthesized — it's a `match TargetSurface -> raw_string_literal` that returns the exact text the migrate test asserts. The realize plugins (which ARE substrate participants) already implement source emission via body templates and PEP 1.7.0. `dispatch_realize` returns `RealizedSource { source, is_stub, ... }`. The hand-emission is the migrate command's biggest substrate-correctness violation. |
| Early return on None in `platform_semantic_changes_for_targets` | `cmd_bind_migrate.rs:753-758` | Should let `compare_op_with` produce the 4-state verdict including `Uncharacterizable { absent_on }`. The early-return collapses NoOpinion and Uncharacterizable. |
| "source must be typescript-better-sqlite3" hardcoded source restriction | `cmd_bind_migrate.rs:248-252` | The migrate should generalize to (any source binding-kit, any target binding-kit) once the M+N hub is complete. |

**Flagged finding** (verified 2026-05-18): `render_migrated_source` is functionally a fixture selector, not a renderer. Three sub-functions (`render_ts_pg_source`, `render_python_sqlite3_source`, `render_python_aiosqlite_source`) each emit a multi-hundred-line raw string literal of the predetermined migrated source. The migrate tests (`migrate_async_rewrite_test.rs`, `stage3_cross_language_test.rs`) assert against these literals. The test is verifying that the function returns the string the function contains. The substrate's actual realize plugins are bypassed entirely in this code path. Dissolution is tracked in audit row D5 / issue #1229.

### 5.2-5.7 cmd_*.rs triage matrix (audited 2026-05-18 per issue #1235)

All 26 remaining `cmd_*.rs` files audited. Categories: **(a)** thin composition of substrate primitives; **(b)** on-critical-path with reinventions; **(c)** off-critical-path for the Trinity demo / substrate-correctness work; **(d)** substrate-only primitive (no kit dispatch by construction).

| Command | Category | Notes |
|---|---|---|
| `cmd_agent.rs` | (c) | Plugin enumeration / tool descriptor emission. Substrate-orthogonal. |
| `cmd_ask.rs` | (c) | Librarian formula query. Substrate-orthogonal. |
| `cmd_bind.rs` | (a) | Substrate-only algebra pass; dispatches exam manifest via `dispatch_exam_manifest` primitive. `RuntimeMode` enum is substrate-coherent (Monitor/Emitter/Witness/Gate per R5 amendment ruling). |
| `cmd_bind_migrate.rs` | **(b)** | **FIVE reinventions tracked in #1226-#1230. The only command with substantive reinventions on the substrate-correctness path.** |
| `cmd_ci.rs` | (c) | CICP reference admission. `lang -> make prove-X` (`cmd_ci.rs:755-767`) is build-infrastructure alias not substrate dispatch. |
| `cmd_compose.rs` | (d) | Compose primitive JSON-RPC subprocess transport per CCP §6.3. `WireAtomicKind` enum (Load/Store/Rmw/Cas) is substrate-coherent. |
| `cmd_dump.rs` | (c) | Pretty-print catalog members + bodies. Substrate-orthogonal. |
| `cmd_exam.rs` | (a) | Dispatches via PEP 1.7.0 exam-manifest primitive. |
| `cmd_fix.rs` | (c) | Agent-driven patch workflow. Substrate-orthogonal. |
| `cmd_hash.rs` | (d) | `blake3-512` of input. Substrate primitive. |
| `cmd_implicate.rs` | (d) | Substrate primitive (implication over CIDs). |
| `cmd_init.rs` | (c) | Project initialization. Substrate-orthogonal. |
| `cmd_lift.rs` | (a) | Dispatches via lift-plugin protocol primitives. |
| `cmd_link.rs` | (c) | Bridge linkage per `2026-05-03-bridge-linkage-protocol.md`. |
| `cmd_lower.rs` | (a) | Dispatches via `dispatch_lower_witness` / `dispatch_realize`. `LowerMode` enum (Witness) is substrate-coherent. |
| `cmd_mint.rs` | (a) | Lift-plugin protocol dispatcher. |
| `cmd_must.rs` | (c) | Agent-driven English-to-contract translation. Substrate-orthogonal. |
| `cmd_package.rs` | (a) | Supply-chain receipt helpers. Related to D13a shim distribution but no reinvention. |
| `cmd_plugin.rs` | (a) | PEP 1.7.0 plugin flag plumbing. |
| `cmd_proof.rs` | (a) | Proof artifact workflow. |
| `cmd_protocol.rs` | (c) | Protocol catalog evolution. Substrate-orthogonal. |
| `cmd_prove.rs` | (a) | Six-stage pipeline + lift-plugin conformance gate. |
| `cmd_search.rs` | (c) | Pattern search. Substrate-orthogonal. |
| `cmd_transport.rs` | (a) | Cross-language transport via substrate primitives. Lang alias table (`cmd_transport.rs:377-385`: `py` → `python`, `ts` → `typescript`) is normalization not reinvention. |
| `cmd_verify_protocol.rs` | (a) | Catalog verification. |
| `cmd_version.rs` | (c) | Version info. Substrate-orthogonal. |
| `cmd_witness.rs` | (a) | Mints witness memento via substrate primitive. |

**Summary:** of 27 commands (including `cmd_bind_migrate`), 1 has substantive reinventions (already tracked); 12 are thin compositions or substrate-only primitives; 14 are off-critical-path for the Trinity demo work. The substrate is largely coherent at the workflow layer; the reinventions cluster in `cmd_bind_migrate.rs`. No additional issues filed from this triage — #1226-#1230 capture the dissolution work.

### 5.8 cmd_materialize.rs — DOES NOT EXIST

The materialize verb is implied by the vision (Sir's "give me SQL" workflow) but no `cmd_materialize.rs` exists. Implementation is tracked in #1234 (audit row D11).

## 6. Dissolution roadmap

Each work unit is a PAIR: a kit-face mint (or face completion) + the workflow reinvention it enables to dissolve. Each row has been filed as a GitHub issue (or noted as folded / forward-roadmap).

| # | Kit-face mint | Workflow dissolution | Effect | Issue |
|---|---|---|---|---|
| D1 | Mint Python + sqlite3 library kit declaration face (`platform_semantics/python_sqlite3.rs` + arm in `binding_semantics_for_tag`) | Remove `TargetSurface::PythonSqlite3` arm + its branches in `requires_async_delta` and `render_migrated_source`. Migrate dispatches through `dispatch_realize("python", Some("sqlite3"), ...)`. | `platform_semantics_for_binding("python", "sqlite3")` resolves. Trichotomy fires. Receipt characterizes RowIdMechanism divergence (CursorLastRowid vs LastInsertRowid). | #1226 |
| D2 | Mint Python + aiosqlite library kit declaration face | Same as D1 for PythonAiosqlite arm. | Adds async-aware Python binding kit. | #1227 |
| D3 | Fix `platform_semantic_changes_for_targets` early-return | Let `compare_op_with` produce 4-state verdict including `Uncharacterizable { absent_on }` for unilateral declarations. | Substrate becomes honest per #1204 trichotomy ruling. | #1228 |
| D4 | Introduce `AsyncMode` (or equivalent) dimension. Declare AsyncMode per binding kit (better-sqlite3: Sync, pg: Async, sqlite3: Sync, aiosqlite: Async). | Remove `requires_async_delta` flag. Effect propagation reads AsyncMode declarations. | Async-ness becomes uniform with other platform semantics. | #1236 |
| D5a | **Substrate-output verification** for the migrate demo. Invoke `realize_probe_via_path` (existing infrastructure at `cmd_bind_migrate.rs:969`) per real callsite, capture realized output, diff against `render_*_source` hardcoded fixtures. Substrate-honesty checkpoint before D5 dissolution. | none yet; this row produces a diff report + per-finding plan. | Confirms substrate emits substrate-correct source OR surfaces fixture/plugin gaps to fix before dissolution. | #1238 (harness landed #1243; result: 12/12 MISSING) |
| D5b | **TypeScript realize-core: `bodyTemplateFor` reads named_term_tree (NTT) for args_shape, falls back to param_types when NTT absent.** Plugin code change; no schema change; no new templates. Covers both TS-pg AND TS-better-sqlite3 because both delegate to the shared `provekit-realize-typescript-core` factory at `realizer.js:29`. | The plugin's per-function-signature lookup currently returns is_stub=true for the migrate's actual callsites. After this fix, the lookup canonicalizes via NTT and the existing (sql, args)-shaped templates apply uniformly. | First of three plugin fixes that flip 4/4 TS-pg callsites MISSING → MATCHES. | #1244 |
| D5c | **Python-sqlite3 plugin: same NTT-canonicalization fix at `realizer.py:76`.** | Same as D5b for python-sqlite3. | Flips 4/4 python-sqlite3 callsites MISSING → MATCHES. | #1245 |
| D5d | **Python-aiosqlite plugin: same NTT-canonicalization fix at `realizer.py:76`.** Near copy of D5c. | Same as D5b for python-aiosqlite. | Flips 4/4 python-aiosqlite callsites MISSING → MATCHES. | #1246 |
| D5e | **D5a harness extension: NTT-mode probes alongside bare-signature probes.** The bare-signature probe is what missed body-template-completeness the first time; the NTT-mode probe is the gate that proves the fix. | New test variant in `d5_realize_verification_harness_per_callsite_per_target` that constructs NTT for each probe + reruns the 12 verification points. Bare-signature probes stay (they verify D5b/D5c/D5d's fallback path). | Hardens the substrate-honesty checkpoint going forward — every future kit-completeness claim runs both probe modes. | #1247 |
| D5f | **TS realize-core `renderTemplate` uses NTT-derived substitution values, not function params.** PR #1248 added NTT-based args_shape for the guard match, but `renderTemplate` (line 248 of `realizer.js`) still substitutes `${paramN}` from the function's params. For the migrate's typed callsites (e.g., `getUserById(id)`), `${param1}` stays unresolved and renderTemplate returns null, so the plugin re-emits a stub even when the guard match passed. Surfaced by NTT-mode harness on `1c7a913ec`: 4/4 ts-pg cells still MISSING. | Pass NTT-derived template params (the args' `source` field) into `renderTemplate` when NTT is present. Mirrors the python plugins' `_template_lookup_signature` pattern. | Flips 3/4 ts-pg cells from MISSING to SEMANTIC or MATCHES (recordEvent still blocked by D5h). | #1252 |
| D5g | **Python sqlite3 + aiosqlite plugins diverge in args_shape NTT-derivation.** python-sqlite3 maps NTT sort names via `_map_ntt_arg_descriptor` (`"Sql"` → `"str"`, `"SqlArgs"` → `"list[object]"`). python-aiosqlite returns the raw sort via `_tree_arg_shape` (`"Sql"`, `"SqlArgs"`). PRs #1249 and #1250 are NOT identical despite being filed as near-copy fixes. Latent today because catalog guards don't use `requires_param_types`, but will matter when finer guards land. | Port `_map_ntt_arg_descriptor` from python-sqlite3 to python-aiosqlite, OR extract a shared Python helper. | Eliminates a latent inconsistency before it blocks future work. | #1253 |
| D5h | **`concept:insert-and-get-id` is missing from ALL THREE target body-template catalogs.** Verified: ts-pg, python-sqlite3, python-aiosqlite all have only `concept:sql-query` and `concept:sql-execute`. The `recordEvent` callsite (classified as `concept:insert-and-get-id`) cannot find a matching template on any target, so 3/12 cells stay MISSING regardless of plugin fixes. | Mint `concept:insert-and-get-id` entries in each catalog parameterized by `(sql, args)` with `min_params=2 / max_params=2`. Each entry emits the per-binding INSERT body (pg uses RETURNING clause; sqlite3/aiosqlite use `cursor.lastrowid`). Sub-question for triage: who translates SQL strings from sqlite3-flavor `?` to pg-flavor `$1 RETURNING id`, the migrate, the plugin, or a separate `concept:sql-translate` primitive? | Flips 3 recordEvent cells from MISSING to SEMANTIC or MATCHES. Closes the catalog gap empirically. | #1254 |
| D5 | **Replace `render_migrated_source(target_surface)` with `dispatch_realize` invocation per callsite. The RealizeRequest builder constructs NTT from each SqlCallsite** (sql string + sample_args become the canonical concept:sql-query / concept:sql-execute / concept:insert-and-get-id args). | After D5b/D5c/D5d unblock the plugins and D5e proves the path, plus D5f+D5h close the renderTemplate-substitution and the insert-and-get-id-catalog gaps: the 280+ lines of hardcoded fixture output are deleted. The migrate's per-callsite NTT-construction is the load-bearing primitive-shaping work D5 was always going to need. | Migrate workflow becomes thin; it composes primitives end-to-end. | #1229 (scope refined; depends on D5b+D5c+D5d+D5e+D5f+D5h; D5g is non-blocking cleanup) |
| D6 | Remove "source must be typescript-better-sqlite3" check. | Migrate generalizes to (any source binding-kit, any target binding-kit). | M+N hub fully load-bearing. | #1230 |
| D7 | Mint Rust + rusqlite library kit (all three faces) | Migrate can target rusqlite as source or target. | First Trinity leg: Rust. | #1231 |
| D8 | Mint Java + sqlite-jdbc library kit (all three faces) | Migrate can target sqlite-jdbc as source or target. | Second Trinity leg: Java. | #1232 |
| D9 | Complete Python + sqlite3 library kit. | Folded into D1 — body templates and realize plugin already exist; D1 mints the missing declaration face. | Python leg complete after D1. | folded into #1226 |
| D10 | Confirm or mint per-language exam manifests for Rust, Java, Python | Substrate runs with per-language exams (per pin-all-three). | Federation across the Trinity becomes provable. | #1233 |
| D11 | Implement `cmd_materialize.rs` | New verb composes `dispatch_realize` + .proof reading. | "Give me SQL" workflow works end-to-end. | #1234 |
| D12 | Audit remaining cmd_*.rs for reinventions, dissolve each pair | Workflow layer becomes uniformly thin. | Vision realized at the workflow layer. | #1235 |
| D13a | Ship Trinity-demo library kits as shim packages (phase B): `java-sqlite-jdbc-proof.jar`, `provekit-shim-python-sqlite3` (pip), `provekit-shim-rusqlite` (cargo) | `binding_semantics_for_tag` resolves library tags via shim package discovery + .proof read. No hardcoded arms. | Substrate consumes library kits via vendor-ecosystem package managers without requiring vendor cooperation. | #1237 |
| D13b | Land vendor adoption for first library (paper 22 / phase C) | Selected library's vendor merges shim into native distribution. | Demonstrates viral adoption loop end-to-end. | not yet filed (forward roadmap) |
| D14 | Externalize language kits into separate distributions (cosmic-brain endgame, paper 25) | `platform_semantics_for_lower_target` resolves languages via dynamic kit registration. | Substrate becomes purely protocol. | not yet filed (forward roadmap) |
| D15 | Unify lift face + realize face as bidirectional kits per body-template spec | One declarative kit spec → both directions, derived. | The cosmic-brain endgame. | not yet filed (forward roadmap) |

D1–D6 unblock the substrate-correctness fixes for the existing migrate demo (TypeScript ↔ Python).
D7–D11 deliver the Trinity demo (Rust ↔ Java ↔ Python with materialize).
D12 systematizes the workflow-layer audit.
D13a delivers the shim distribution for the Trinity demo's library kits.
D13b–D15 complete the vision realization.

## 7. Vision realization milestones

The substrate's terminus state, with the Trinity demo as the load-bearing checkpoint. Distribution evolves through three phases per the vendor adoption arc; the shim is the bridge that unblocks substrate adoption without requiring vendor cooperation.

### 7.1 Distribution evolution

| Phase | Library kit distribution | Example: java-sqlite-jdbc | Vendor commitment |
|---|---|---|---|
| A: Bootstrap-resident | Kit declaration in `libprovekit/src/core/platform_semantics/<tag>.rs` + body templates in `menagerie/<lang>-language-signature/specs/body-templates/`. Realize plugin in `implementations/<lang>/`. | Today's state for better-sqlite3, pg. | None required. Substrate's own bootstrap path. |
| B: Shim distribution | A separate package (sibling to the library) contains the `.proof` bundle. The substrate discovers it via package-manager dependency resolution. Library author is unaffected. | `org.provekit-shim:java-sqlite-jdbc-proof:X.Y.Z.jar` containing one file (`provekit.proof`). Pairs with `org.xerial:sqlite-jdbc:X.Y.Z`. The shim is a substrate-community-shipped artifact. | None required from the library author. Anyone can ship a shim. |
| C: Vendor adoption | The library author ships the `.proof` bundle inside the library's own package. Shim becomes unnecessary. | `org.xerial:sqlite-jdbc:X.Y.Z.jar` containing `META-INF/provekit/provekit.proof` natively. | Yes — vendor adopts. |

The shim model is the **viral adoption loop**'s mechanism: anyone can write a shim for any library. The substrate works. When usage grows, vendors absorb the shim into their library distribution. Per `project_provekit_libraries_ship_sugar` memory: "Libraries ship their own sugar — Self-Attested concept-bindings (paper 22 = After Vendoring)."

Per-language shim package format:

| Language | Shim format | Example package id | .proof location inside |
|---|---|---|---|
| Java | `.jar` containing one resource | `org.provekit-shim:java-sqlite-jdbc-proof` | `META-INF/provekit/provekit.proof` (or shim-conventional path) |
| Python | wheel / sdist | `provekit-shim-python-sqlite3` (pip) | `provekit_shim_python_sqlite3/provekit.proof` |
| Rust | crate (sibling) | `provekit-shim-rusqlite` (cargo) | included via `include_bytes!` or `assets/provekit.proof` |
| TypeScript / JS | npm package | `@provekit-shim/typescript-better-sqlite3` | `provekit.proof` at package root |
| Go | module | `github.com/provekit-shim/go-sqlite3` | `provekit.proof` at module root |

The shim is one file in a thin package. No code, no build infrastructure, no fork of the library. Just the `.proof` carried by the language's existing package manager.

### 7.2 Milestones

| Milestone | What works | Distribution phase |
|---|---|---|
| M1: Trinity Local Demo | Rust ↔ Java ↔ Python round-trip with sqlite library kits, all workflows compose primitives. | A (bootstrap-resident) |
| M2: Shim-Distributed Trinity | Same as M1 but library `.proof` bundles ship as shim packages (`java-sqlite-jdbc-proof.jar` etc.). The substrate resolves library tags via package-manager dependency resolution. `binding_semantics_for_tag` dissolves to a shim-discovery primitive. | B (shim distribution) |
| M3: Vendor-Adopted Library Kits | Selected vendors absorb shims into their library distribution. The substrate finds `.proof` inside the library's own package. Shim packages remain for libraries whose vendors haven't adopted. | C (vendor adoption) — partial |
| M4: Externalized Language Kits | Language kits ship as separate distributions, not in `libprovekit/src/core/platform_semantics/`. `libprovekit` shrinks to protocol + dispatcher + primitives + ProofIR + canonicalizer. | Both kit kinds externalized. |
| M5: Bidirectional Kits | One declarative kit spec → both lift and realize directions, derived. The cosmic-brain endgame: kit declaration model is unified, lift+realize fall out of one source. | Substrate is purely protocol; kits are declarative bidirectional artifacts. |

## 8. Issue derivation

GitHub issues derive from rows in this audit:

- Each kit-face mint in section 4 → one issue (e.g., "mint Python + sqlite3 library kit declaration face").
- Each workflow reinvention in section 5 → one issue (e.g., "dissolve TargetSurface::PythonSqlite3 arm in cmd_bind_migrate").
- Each dissolution-roadmap pair in section 6 → typically TWO issues (the mint + the dissolution), worked in sequence.
- Each milestone in section 7 → one umbrella issue covering its sub-rows.

No issue gets filed without verifying file:line refs against the actual code. No issue gets dispatched to codex without confirming the change shape matches what the workflow's production code path actually looks like.

## 9. TODO sections requiring deeper verification

This document is a first-pass audit. The following sections need code-level verification before issues derive from them:

- [ ] Section 5 (workflow reinventions): all cmd_*.rs files beyond cmd_bind_migrate need triage. Pin file:line for each reinvention.
- [ ] Section 4.1: TypeScript and C lift plugin locations.
- [ ] Section 4.2: body-template JSON locations for pg, sqlite3, aiosqlite, requests.
- [ ] Section 4.4: language exam manifests beyond the v1.1 default — confirm or surface as gap.
- [ ] Section 6 D7, D8: the exact library identifiers and rusqlite/sqlite-jdbc API mechanisms to declare.

This audit document is intended to grow as the verifications complete. Each TODO is its own focused investigation that adds a confirmed row.

## 10. Discipline

- This audit document is the single source of truth for the kit-protocol-correctness work. Updates to the vision flow back into this document, not into ad-hoc memos.
- No GitHub issue gets filed without referencing a row here.
- No codex dispatch happens without the issue + this document's row + verified file:line.
- The audit grows incrementally; the early state captured here is the bootstrap, not the endgame.
