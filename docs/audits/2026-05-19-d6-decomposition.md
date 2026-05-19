# D6 decomposition: migrate generalization audit

Date: 2026-05-19
Author: Kit (Opus 4.7 1M)
Status: First-pass audit. NOT a dispatch brief. Read before filing any D6 sub-issue.

**Update 2026-05-19 (later same day):** The A1/A2/A3 architect-call framing in section 4 was superseded mid-day after walking through the substrate's existing platform-semantics machinery (`platform_semantics.rs:124`, `compare_op_with` at `types.rs:874`, `propagate_effects` at `effect_propagation.rs:111`). The substrate's M+N hub already handles every dimension the D6 work touches; gaps close via concept-CID mints + per-kit `PlatformSemanticsDeclaration` extensions, NOT via new dispatcher layers or new design choices.

Authoritative framing now lives in:
- `docs/explanation/substrate-uniform-pattern.md` (rules of engagement)
- The foundational concept:literal mint issue
- The substrate envelope-violation fix issue (`kit_cid` elision from content-CID computation)

Sections 1, 2, 3 of this audit (state, substrate-correct flow, gaps with file:line refs) remain accurate as the gap inventory. Section 4's three-option architect-call framing is OBSOLETE; the substrate already provides the answers via existing primitives.

Parent: `docs/audits/2026-05-18-kit-as-substrate-participant-vision.md` section 6 row D6 (`Remove "source must be typescript-better-sqlite3" check. Migrate generalizes to (any source binding-kit, any target binding-kit).`).

The parent row collapsed a multi-step infrastructure build into one bullet. This document pins the actual gaps to file:line and surfaces what does not exist today (so issues do not derive from infrastructure that has not been built).

## 1. State after #1259 (`cbe690ae2`)

D5 dissolved the per-callsite emission path: `compose_migrated_source` now dispatches each `SqlCallsite` through the realize plugin via NTT construction. The 280 lines of inline fixture body text are gone. The hardcoded scaffolding around the callsites (imports, type declarations, helper functions, non-callsite function bodies, function-name transforms, async-widening) still lives inside the migrate command.

The migrate's source-side parsing also still lives inside the command:

- `cmd_bind_migrate.rs:86` reads exactly `<source_dir>/src/users.ts`.
- `cmd_bind_migrate.rs:332-491` `extract_functions` parses TypeScript syntactically inside the migrate (function-keyword scan, brace-matching, parameter parsing, return-type extraction).
- `cmd_bind_migrate.rs:493-553` `extract_sql_callsites` detects callsites by string-matching `.prepare(`, classifies concept by string-matching `.run(` and `.lastInsertRowid`, parses the SQL string by quote-tracking inside the function body.
- `cmd_bind_migrate.rs:256-260` rejects any `(source_lang, source_tag)` other than `("typescript", "better-sqlite3")`.
- `cmd_bind_migrate.rs:287-292` hardcodes target output paths `src/users.ts` and `src/users.py`.

The TargetSurface enum's residual uses after #1259:

- `is_python` (line 271): branches witness emission at run_inner line 153.
- `requires_async_delta` (line 275): branches async-effect propagation at run_inner line 112 (the D4 dimension work, not D6).
- `after_effect` (line 279): same domain as `requires_async_delta`.
- `output_file` (line 287): per-target output-path convention used by `write_migrated_project` (line 2089) and `after_location_for_callsite` (line 1339).

## 2. Substrate-correct migrate flow

What the M+N hub vision asks for, mapped to existing substrate primitives:

1. Resolve `(source_lang, source_tag)` and `(target_lang, target_tag)` from `--library-from` / `--library-to`. EXISTS (`cmd_bind_migrate.rs:75-76`).
2. Probe kit availability for both source binding (lift + declaration faces) and target binding (declaration + realize faces). If any face is missing, refuse loudly via gap record. PARTIALLY EXISTS: `probe_realize_binding` at `cmd_bind_migrate.rs:98-101` covers target realize; nothing probes source lift.
3. Dispatch source lift kit on `source_dir`, get back `BindLiftResult { entries: Vec<BindLiftEntry>, diagnostics }`. EXISTS at `kit_dispatch.rs:504` as `dispatch_bind_lift(workspace_root, source_lang) -> Result<BindLiftResult, KitUnavailable>`. NOT CALLED from cmd_bind_migrate (only from `cmd_transport.rs:237`).
4. For each `BindLiftEntry` whose `concept_annotation` matches one of the migrate's recognized concepts (sql-query, sql-execute, insert-and-get-id), build a NamedTermTree and dispatch the target realize plugin via `dispatch_realize`. PARTIALLY EXISTS: the NTT builder + realize-dispatch path was added in #1259; today's callers feed it `SqlCallsite` records from the in-process parser, not BindLiftEntries.
5. Compose the per-callsite emissions plus the source's non-callsite content into the migrated file. PARTIALLY EXISTS in `compose_migrated_source` post-#1259 (per-callsite emission via substrate; structural shell handling target-language-specific transforms still hardcoded for the demo corpus).
6. Write to `out_dir` at a path resolved from the target binding kit's output-path convention. NOT EXIST as substrate-resolved: today it is the `TargetSurface::output_file()` enum branch.

## 3. Gaps

### G1: No TypeScript bind-lift kit exists

The Rust bind-lift kit exists at `implementations/rust/provekit-walk/src/bin/walk_rpc.rs`. It emits records that decode into `BindLiftEntry`: per-function structure, `concept_annotation` populated from `// @concept:...` source comments above the function (`walk_rpc.rs:527-535`), and `term_shape` from the lifted IR (`walk_rpc.rs:1408+`).

The TypeScript source lift exists at `implementations/typescript/src/lift/typescript-source/index.ts`. It emits `FunctionContractMemento[]` with `pre`/`post`/`effects`/`locus`. SHAPE DIVERGENT: not `BindLiftEntry`, no `concept_annotation`, no SQL-string carrying.

The dispatcher's resolution order (`kit_dispatch.rs:521-583`) looks for a binary named `provekit-bind-lift-typescript` under `implementations/typescript/target/{release,debug}/`. No such binary exists. No source for it under `implementations/typescript/src/`.

Building a TypeScript bind-lift kit means a new TypeScript program that uses the TypeScript compiler API to walk the source directory, recognizes a concept-annotation convention (`// @concept:...` above function declaration, mirroring Rust's), emits per-function `BindLiftEntry`-shaped records over JSON-RPC, and packages as a binary the dispatcher can resolve.

Open design questions before the kit can be built:

- Q1: Where does the SQL string content live in the emitted entry? The Rust convention emits `term_shape` with the lifted body IR. For SQL callsites, the SQL string is a literal argument to `.prepare(...)`. Three candidate places:
  - Inside `term_shape` as a structured node the migrate parses.
  - In a new `concept_arguments: Value` field on `BindLiftEntry` that concept-tagged entries can populate with concept-specific data (for sql-* concepts: `{ "sql": "...", "args_template": "(?, ?)" }`).
  - As a witness with `source_kind: "annotation"` carrying the SQL.
  This is an architect-call before D6b can be specified. Until it is decided, the migrate cannot consume lift output and still need the SQL string per callsite.

- Q2: How does the kit identify which functions to lift? Today's `extract_sql_callsites` only emits a callsite when the function body contains `.prepare(`. A TS bind-lift kit could (a) emit one entry per function and let the consumer filter by `concept_annotation`, or (b) emit only annotated functions. Convention with Rust's `walk_rpc` is (a): it emits every function, populating `concept_annotation` opportunistically (`walk_rpc.rs:398-399`).

### G2: cmd_bind_migrate does not call dispatch_bind_lift

`dispatch_bind_lift` is only called from `cmd_transport.rs:237`. cmd_bind / cmd_bind_migrate use their own parsing. Wiring cmd_bind_migrate to call dispatch_bind_lift requires:

- Replace `extract_functions` + `extract_sql_callsites` (cmd_bind_migrate.rs:332-491, 493-553) with consumption of `BindLiftEntry[]`.
- Map each entry's `concept_annotation` to the existing concept names (`concept:sql-query` etc.) and the SQL string content to whatever G1's Q1 answer resolves to.
- Preserve the existing downstream pipeline: `semantic_changes`, effect propagation, `compose_migrated_source`, receipt build. All of these consume `SqlCallsite[]` and `TsFunction[]` today; the structures themselves either stay (with a different upstream) or get replaced by `BindLiftEntry` directly.

Smallest plausible change: keep the in-memory `SqlCallsite` / `TsFunction` types as adapters, add a `from_bind_lift_entries(entries: &[BindLiftEntry]) -> Result<(Vec<TsFunction>, Vec<SqlCallsite>), String>` translator, replace the `extract_*` calls with `dispatch_bind_lift` + the translator. The structural shell composition stays target-language-specific (see G3) for this stage. This is non-trivial but bounded.

### G3: Structural shell handling is hardcoded for the demo corpus

`compose_migrated_source` (post-#1259) routes per-callsite emission through the substrate. The non-callsite content of the migrated file (imports, type declarations, helper functions, function signatures, function-name transforms TS-case to snake-case, async-widening of non-callsite functions) is generated inside cmd_bind_migrate.

To make the migrate fully target-binding-agnostic, the structural shell would need to be either (a) sourced from per-target-binding scaffolding declarations that ship with the binding kit, or (b) synthesized from the lifted source by source-to-source transformation machinery the substrate provides.

Neither (a) nor (b) is in scope for D6 as the parent row described it. D6's "remove the source-restriction check" only makes sense after the shell handling generalizes too, OR after the migrate's responsibility is rescoped to "emit only the callsite-level body changes" and a separate workflow assembles the full migrated file.

This gap is not D6's job to close; it surfaces as a forward-roadmap row (D6-Shell or fold into an extended D5-Phase-2).

### G4: Hardcoded source-side path

`cmd_bind_migrate.rs:86` joins `<source_dir>/src/users.ts`. The substrate-correct convention is `dispatch_bind_lift` walks the source directory itself per source-language convention. If G1's TypeScript bind-lift kit walks the source dir (the same way `walk_rpc` walks Rust projects), this gap closes as a side effect of G1+G2.

### G5: Hardcoded target output path and write convention

`cmd_bind_migrate.rs:287-292` `output_file` returns `src/users.ts` or `src/users.py`. Used at:

- `cmd_bind_migrate.rs:2089` `write_typescript_project` writes `src_dir.join("users.ts")`.
- Python equivalent in `write_python_project`.
- `cmd_bind_migrate.rs:1339` `after_location_for_callsite` uses `target_surface.output_file()` to resolve `MigrationSourceLocation.file`.

The target-side output path convention belongs in the target binding kit's declaration (sibling to body templates and the platform semantics arm). Until that convention is added to the kit declaration shape, this gap stays open.

### G6: TargetSurface enum's residual dimension-decisions

After G2-G5 close, the enum's residual uses are:

- `is_python` (cmd_bind_migrate.rs:271): used at run_inner line 153 to branch witness emission (cross-language witnesses for Python targets). Should resolve via the target binding kit's declared exam manifest or a per-binding witness-mode dimension.
- `requires_async_delta` / `after_effect`: AsyncMode dimensionalization, parent audit row D4 (issue #1236). Not D6's job.

If D4 lands first, the enum's only remaining use is `is_python` for witness branching. If D6 lands first, the enum can stop being a discriminator and become an optimization shortcut over per-binding-kit declarations.

## 4. Sub-issue derivation

Each row is a concrete file-line-pinned sub-issue. Dependencies are stated.

**Architect-call status (ratified 2026-05-19):**

- **D6-A1: RULED via #1261.** SQL string content lives as `concept:literal { value: "SELECT ...", sort: <String CID> }` in term_shape. NOT a new `concept_arguments` field on `BindLiftEntry`, NOT a witness carrier, NOT a structural-SQL substrate evolution. Value-tier op family covers SQL strings as one instance of "literal in source." Per-plugin body templates substitute SQL via existing `${param0}` mechanism. The D6-A1 question is dissolved by the substrate-uniform pattern; no separate ruling doc needed.
- **D6-A2: RULED.** Language kit owns file-extension declaration (`.ts`, `.py`, `.rs`, `.java`, `.c`). Library kit owns nothing about output paths. Migrate workflow composes output as `<out-dir>/<source-rel-path-without-ext><target-language-ext>`. `TargetSurface::output_file` dissolves; replaced by language-kit-declared extension lookup. Edge case: project-scaffolding files (package.json, Cargo.toml, pom.xml) don't follow simple extension-swap; those stay per-binding-pair hardcoded for the Trinity demo and become future audit-row work for substrate-uniform project-scaffolding via concept catalog (concept:package-manifest, concept:build-config, etc.).
- **D6-A3: RULED on direction; interim acceptable.** Long-term: option (b) — every non-callsite source construct mints as a concept-tier op; realize plugins emit per-target syntax. This is the generalization of the value-tier op family pattern to STRUCTURAL-tier ops. Per `docs/explanation/value-tier-op-family.md` and the parent vision audit's new D18 row "Structural-tier op family mints." Interim: existing hardcoded structural shell in `compose_migrated_source` stays for the Trinity demo (corpus-specific; the users.ts example). Trinity demo doesn't block on it; demo proves value-tier substrate; structural-tier follows when needed.

The D6-A1/A2/A3 rulings are operationally captured via the substrate-uniform pattern (`docs/explanation/substrate-uniform-pattern.md`), value-tier op family doc (`docs/explanation/value-tier-op-family.md`), and parent vision audit's D16/D17/D18 rows. No separate per-ruling docs are needed; the framework dissolves the questions.

| # | Sub-issue | Files / line refs | Depends on | Architect call? |
|---|---|---|---|---|
| D6-A1 | RULED via #1261: concept:literal carries SQL strings in term_shape. No separate ruling doc needed. | (resolved) | (resolved) | RULED |
| D6-A2 | RULED: language kit owns file-extension; workflow composes output path. Scaffolding-file generation per-binding-pair hardcoded interim; future per substrate-uniform project-scaffolding concepts. | `cmd_bind_migrate.rs:287-292`, `2089`, `1339`. Affects each language kit's `PlatformSemanticsDeclaration` (or sibling exam-answer) declaring its canonical file extension. | None | RULED |
| D6-A3 | RULED: option (b) long-term via structural-tier op family mints (parent audit row D18). Interim: existing hardcoded structural shell stays for Trinity demo. | `cmd_bind_migrate.rs::compose_migrated_source` and helpers; future structural-tier mints under `menagerie/concept-shapes/catalog/algorithms/concept:import.<cid>.json`, `concept:type-declaration.<cid>.json`, etc. | None | RULED |
| D6-B | Build TypeScript bind-lift kit (G1). Source under `implementations/typescript/src/lift/typescript-bind/` (new directory) emitting `BindLiftEntry`-shaped records via JSON-RPC. Walks source directory per TypeScript convention. Recognizes `// @concept:...` annotations above function declarations. Populates SQL-content per D6-A1. Build target binary `implementations/typescript/target/release/provekit-bind-lift-typescript` (or convention from the kit's package.json). | New file(s) under `implementations/typescript/src/lift/typescript-bind/`. Mirror shape: `implementations/rust/provekit-walk/src/bin/walk_rpc.rs:368-414` (entry emission), `:527-535` (concept annotation extraction). | D6-A1 | NO (codex after A1) |
| D6-C | Wire cmd_bind_migrate to consume dispatch_bind_lift output (G2). Add `from_bind_lift_entries` adapter producing `(Vec<TsFunction>, Vec<SqlCallsite>)`. Replace `extract_functions` + `extract_sql_callsites` calls at cmd_bind_migrate.rs:89-90 with `dispatch_bind_lift(&repo_root, &source_lang)` + the adapter. Keep `SqlCallsite` and `TsFunction` structures as in-process adapters. | `cmd_bind_migrate.rs:86-93` (replace), `:332-553` (delete the in-process parsers OR keep as test-only fallbacks). | D6-A1, D6-B | NO (codex) |
| D6-D | Remove hardcoded source-restriction (the parent D6 row's bullet). Replace `cmd_bind_migrate.rs:256-260` check with: if `dispatch_bind_lift(workspace_root, source_lang)` returns `KitUnavailable`, refuse loudly via gap record. | `cmd_bind_migrate.rs:256-269`. | D6-B, D6-C | NO (codex) |
| D6-E | Remove hardcoded source-side path (G4). Replace `cmd_bind_migrate.rs:86` `source_dir.join("src").join("users.ts")` with `dispatch_bind_lift` walking source_dir. | `cmd_bind_migrate.rs:86-88`. | Folded into D6-C (the lift kit walks; migrate stops reading a fixed path). | NO (folded) |
| D6-F | Generalize target output-path convention per D6-A2 ruling. Replace `TargetSurface::output_file` + its callers with the kit-declared convention. | `cmd_bind_migrate.rs:287-292`, `:2089`, `:1339`. | D6-A2 | NO (codex) |
| D6-G | Structural shell handling per D6-A3 ruling. Implementation scope depends on which option A3 selects. | `cmd_bind_migrate.rs::compose_migrated_source` and helpers. | D6-A3 | varies by ruling |
| D6-H | Dissolve TargetSurface enum's residual uses (G6). After D6-F lands and D4 (issue #1236) lands, the enum's only remaining use is `is_python` for witness branching. Replace with per-target-binding witness-mode declaration OR fold into target exam manifest. | `cmd_bind_migrate.rs:271-273` (`is_python`). | D6-F, #1236 | NO (codex after D4) |

## 5. Dependency graph

```
D6-A1 (ruling) -----+--> D6-B (TS bind-lift kit) ---> D6-C (wire migrate) ---> D6-D (remove restriction)
                    |                                       |
D6-A2 (ruling) -----+--> D6-F (output-path generalize) -----+
                    |
D6-A3 (ruling) -----+--> D6-G (shell handling)

D4 (#1236, async dimension) + D6-F ---> D6-H (dissolve enum)
```

The three rulings (A1, A2, A3) are architect-call work; they can be drafted in parallel before any codex dispatch.

D6-B is the largest code work and the longest-pole dependency. D6-C, D6-D, D6-F, D6-G, D6-H are mechanically smaller once their prerequisites land.

## 6. What this audit deliberately does not do

- Does not file the sub-issues. Issue derivation is the next step; this audit is the source.
- Does not pick between A1/A2/A3 options. Those are architect decisions Sir owns.
- Does not propose codex briefs. Briefs derive from a ruling-pinned sub-issue, not from this audit.
- Does not commit to D6 closing the parent row's bullet alone. The parent row "Remove source must be typescript-better-sqlite3 check" is D6-D; the row's stated effect "Migrate generalizes to (any source binding-kit, any target binding-kit)" requires D6-A1/A2/A3 rulings, D6-B, D6-C, D6-F, and a decision on D6-G's option.

## 7. Open questions for Sir before any sub-issue files

- A1: Where does SQL-string content live in BindLiftEntry? (term_shape, concept_arguments field, witness, other.)
- A2: Does language kit or library kit own the target output-path convention?
- A3: Structural shell handling option (a/b/c)? This is the largest scope-defining choice.
- Priority: D6 closure as M+N-hub-load-bearing vs. Trinity demo prereqs (#1231 rust-rusqlite, #1232 java-sqlite-jdbc, #1233 exam audit, #1234 cmd_materialize, #1237 shim distribution). M+N hub closure makes D7/D8 cleaner; Trinity demo prereqs give the substrate a second-language-pair proof point. Either ordering is defensible.

## 8. Cross-references

- Parent audit: `docs/audits/2026-05-18-kit-as-substrate-participant-vision.md` section 6 row D6.
- Substrate primitive: `kit_dispatch.rs:504` `dispatch_bind_lift`.
- Substrate primitive: `kit_dispatch.rs:913` `dispatch_realize`.
- Existing Rust bind-lift kit: `implementations/rust/provekit-walk/src/bin/walk_rpc.rs`.
- TypeScript source lift (different shape): `implementations/typescript/src/lift/typescript-source/index.ts`.
- Closed dependency: #1229 (D5) `cbe690ae2` dissolved per-callsite emission.
- Still-open dependencies: D4 (#1236) AsyncMode dimensionalization.
