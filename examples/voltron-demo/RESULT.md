# voltron-demo — multi-vendor, multi-file ProvekIt consumer

This crate is the smallest non-trivial M × N consumer of the ProvekIt
substrate. The intent: prove that **a single user-authored program can
compose contracts from multiple independent vendor `.proof` envelopes
into one verifiable spine**.

- **M = 3** module files (`ingest.rs`, `persist.rs`, `report.rs`) + a
  thin binary entry (`src/bin/voltron-demo.rs`) and a top-level library
  spine (`lib.rs`).
- **N = 2** vendors: `provekit-shim-serde-json-rust` (JSON family) and
  `provekit-shim-rusqlite` (SQL family).
- **5** materialize boundary citations spread across two of the three
  module files (ingest.rs owns the JSON ones; persist.rs owns the SQL
  ones; report.rs is pure user code that crosses both vendors at the
  seam where SQL row text is decoded back into a JSON `Value`).
- **3** test files (`ingest_test.rs`, `persist_test.rs`,
  `voltron_e2e_test.rs`) — these ARE the canonical user-side contract
  surface, lifted by `provekit-lift-rust-tests`. Per the rust-tests
  lifter contract, panics and early-returns inside user functions
  (`parse_event`, `install_schema`, `insert_event`, `compose_report`)
  also lift to implicit pre/post conditions.

When everything runs end-to-end, `provekit prove` against this crate
unions THREE `.proof` envelopes:

  1. `voltron-demo.proof`                    — the head (this crate's spine)
  2. `provekit-shim-serde-json-rust.proof`   — the JSON lion
  3. `provekit-shim-rusqlite.proof`          — the SQL lion

surfaced through the rust kit's `provekit.plugin.resolve_dependency_proofs`
RPC (PR #1568) walking `cargo metadata`. Discharge composes across every
cross-vendor seam in the spine.

## Status (as of this PR)

The demo is **NOT yet end-to-end runnable**. The act of dogfooding it
surfaced **five substrate / shim gaps**, four of which block compilation
of the materialized output. The demo's value in this PR is the
**gap-exposure surface**: every gap below is a concrete instance the
substrate's stated "M × N" claim needs to satisfy. Each gap has a
proposed resolution and either lands in a follow-up PR or files as an
issue.

## Gaps exposed

### Gap #1 — Refused boundary destroys carrier+stub
**State:** FIXED in #1572 (substrate fix on a separate branch off main).

The original `transform_source_text_one_pass_collecting_refusals` arm
for `SiteOutcome::Refuse` consumed-and-dropped the carrier comment and
stub function from the rewritten source. A second library's materialize
pass found nothing to fill. Fix: emit the original lines verbatim on
refuse so multi-library passes leave each other's boundaries intact.
Two regression tests added.

### Gap #2 — `--library` is single-vendor; should be deleted
**State:** Tracked as task #84, follow-up PR.

`provekit materialize --library <lib>` accepts a single library tag per
invocation and routes ALL boundaries to that library (refusing anything
the library doesn't provide). The substrate-honest contract is per-family
routing: every boundary declares its `family` (e.g. `concept:family:json`,
`concept:family:sql`), and `--family-library family=library` (repeatable,
already implemented but currently requires `--library` as a base) maps
each family to its realizer. Deleting `--library` forces every consumer
to declare family routing explicitly and makes multi-vendor single-pass
materialize the only mode. Plan:

  1. Remove `--library` field from `MaterializeArgs`.
  2. Make `--family-library` standalone + repeatable.
  3. Boundary without `family` → refuse with a clear error.
  4. Migrate the ~1000 LOC of integration tests that use `--library`.

### Gap #3 — Stub param names must match carrier-comment payload verbatim
**State:** User-side convention. Documented here.

The kit's emitted body references parameter names exactly as declared
in the carrier-comment payload's `params` field. If the stub function
uses `_x` (the rust convention for "intentionally unused parameter"),
the splice produces `body_references_x_not__x` and won't compile.

**User-side fix:** stub params MUST match the payload's `params` list
verbatim. Don't underscore-prefix; rely on rust treating `unimplemented!()`
as divergent (no unused-variable warning).

### Gap #4 — Splice mishandles pre-existing attributes on stub
**State:** NEW. Filed as a follow-up issue.

When a stub function carries pre-existing attributes (e.g.
`#[allow(unused_variables)]`, `#[deprecated]`, custom user attributes),
the materialize splice machinery APPENDS a new annotated function with
the spliced body INSTEAD OF REPLACING the stub's body. The resulting
source has two definitions of the same function. E0428 (duplicate
definition).

**Substrate fix needed:** `transform_source_text_one_pass*` must
recognize the entire `attributes + signature + body` block as the unit
to replace, not just the `pub fn name(...)` signature line.

### Gap #5 — Shim concept vocabulary doesn't cover all user shapes
**State:** NEW. Filed as a follow-up issue.

`provekit-shim-rusqlite`'s `concept:sql-query-row` binding emits a body
that calls `conn.query_row(sql, params, mapper)` — a generic 4-param
form requiring a closure that maps `&Row<'_>` to `T`. A user who wants
a typed-string-row helper (e.g. `fn sql_query_row_string(conn, sql, args)
-> Result<String>`) has to either match the shim's exact 4-param shape
or the shim must offer additional concepts for common typed-row shapes.

**Resolution paths:**
  (a) Add `concept:sql-query-row-string` (and similar monomorphic forms)
      to provekit-shim-rusqlite's `provides_concepts`.
  (b) Redesign user-side `sql_query_row_string` to match the shim's
      4-param form (carries a mapper closure).
  (c) Add a kit-side adaptation: when the user declares fewer params
      than the shim concept's canonical form, the kit synthesizes a
      default mapper.

This demo will adopt (b) in a follow-up commit to keep the surface
clean; (a) and (c) are tracked separately.

## What this PR contains

- `Cargo.toml` — package manifest, deps on `rusqlite` + `serde_json`.
- `src/lib.rs` — top-level module re-exports + `run_voltron_demo`
  spine.
- `src/ingest.rs` — JSON ingestion module. Two boundary stubs
  (`json_parse`, `json_serialize`) + user-side `parse_event` returning
  `ValidEvent`.
- `src/persist.rs` — SQL persistence module. Three boundary stubs
  (`open_in_memory`, `sql_execute`, `sql_query_row_string`) + user-side
  `install_schema` and `insert_event`. (Note: `sql_query_row_string`
  hits Gap #5 once materialize fills the body.)
- `src/report.rs` — pure user code; the cross-vendor seam where SQL row
  text is fed into `json_parse`. Owns no boundaries itself.
- `src/bin/voltron-demo.rs` — thin binary entry.
- `tests/ingest_test.rs` / `tests/persist_test.rs` /
  `tests/voltron_e2e_test.rs` — unit + integration tests as the
  canonical user-side contract surface.
- `RESULT.md` — this file.

## Validating against the substrate

With the substrate-fix PR #1572 merged, run from the repo root:

```bash
# Pass 1: fill SQL boundaries (refused JSON sites stay intact thanks to #1572).
provekit materialize --target rust --library rust-rusqlite \
  --source-dir examples/voltron-demo/src \
  --project /Users/tsavo/provekit \
  --write

# Pass 2: fill JSON boundaries.
provekit materialize --target rust --library provekit-shim-serde-json-rust \
  --source-dir examples/voltron-demo/src \
  --project /Users/tsavo/provekit \
  --write
```

Both passes succeed. Then:

```bash
cargo build  --manifest-path examples/voltron-demo/Cargo.toml
cargo test   --manifest-path examples/voltron-demo/Cargo.toml
```

**Today these still fail** at Gap #5 (sql-query-row body emits 2-arg
call where rusqlite::Connection::query_row needs 3). The follow-up that
adopts the 4-param mapper shape closes this and unblocks build+test.

After build is green:

```bash
provekit mint  --project examples/voltron-demo
provekit prove --project examples/voltron-demo
```

`prove` will union three `.proof` envelopes (voltron-demo + serde-json
shim + rusqlite shim, the latter two resolved through the rust kit's
`resolve_dependency_proofs` RPC over cargo's resolved tree) and
discharge across every cross-vendor seam.

## The point

The demo's PURPOSE in its current state is not to be a working artifact
but to **expose the load-bearing M × N invariant in concrete code.**
Every gap above is a specific claim the substrate makes that didn't hold
on first contact with a real user-shaped consumer. The substrate-fix PR
closes Gap #1; the follow-ups close the others. When all five are
closed, this crate becomes the canonical end-to-end Voltron acceptance
test — same source, three `.proof` envelopes, one verifier discharge.
