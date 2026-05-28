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

## Status (as of this PR — final overnight update)

The demo **builds, runs, and passes all tests end-to-end** after closing
Gap #5 user-side. The substrate fix (Gap #1, PR #1572) closes the
multi-library-destroys-refused bug. Three other surfaced gaps (#2, #4,
remaining materialize/mint config polish) are tracked as follow-up
issues and PRs. Each gap is a concrete instance the substrate's stated
M × N claim needed to satisfy on first contact with a real user-shaped
consumer.

### Green path (verified tonight)

```
$ cargo build --manifest-path examples/voltron-demo/Cargo.toml
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.27s

$ cargo test  --manifest-path examples/voltron-demo/Cargo.toml
test result: ok. 5 passed; 0 failed   (tests/ingest_test.rs)
test result: ok. 3 passed; 0 failed   (tests/persist_test.rs)
test result: ok. 3 passed; 0 failed   (tests/voltron_e2e_test.rs)
                                       — including full_spine_round_trip_succeeds,
                                         the cross-vendor end-to-end witness

$ cargo run   --manifest-path examples/voltron-demo/Cargo.toml --bin voltron-demo
voltron round-trip: rowid=1 user=alice type=signup report="{\"age\":30}"
```

11/11 tests green. JSON → SQL → SQL → JSON round-trip across both
vendor lions, in user code, threaded by the head's spine. The bin
output shows the cross-vendor seam closed: `serde_json::to_string` of
the payload feeds `rusqlite`'s INSERT, then `rusqlite`'s SELECT feeds
`serde_json::from_str` back to a `Value`, and the spine prints the
final `age=30` from the round-tripped JSON.

### Mint + prove (Phase-2 polish remaining)

`.provekit/config.toml` is in place declaring `rust-sugar` +
`rust-contracts` lift surfaces. `provekit mint --project .` dispatches
correctly but currently warns that the lifter binary at
`implementations/rust/target/debug/provekit-walk-rpc` is not found —
the manifest's relative path resolves against the demo's project root,
not the workspace root. Two tracked resolutions:

  - Demo carries a project-local manifest override pointing at the
    workspace binary path (simple; small follow-up commit).
  - Substrate teaches the plugin loader to walk up from the project
    root to find workspace-level binaries (broader fix; helps every
    consumer crate that lives inside a monorepo).

Either path is mechanical. Tonight's stop point is here so the green
binary + tests can land on the PR as durable proof-of-spine, and the
mint+prove polish doesn't conflate with the substrate fix already in
flight (#1572).

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
**State:** Tracked as issue #1575. **CLOSED FOR THE DEMO** by adopting
resolution path (b): user-side `sql_query_row<T, P, F>` matches the
shim's 4-param mapper form. Callers pass `|row| row.get(0)` closures.
This keeps the demo green without growing the shim's concept vocabulary.

`provekit-shim-rusqlite`'s `concept:sql-query-row` binding emits a body
calling `conn.query_row(sql, params, mapper)` — a generic 4-param form
requiring a closure mapping `&Row<'_>` to `T`. A user who wants a
typed-string-row helper has to either match the shim's exact 4-param
shape or the shim must offer additional concepts for common typed-row
shapes.

**Resolution paths (#1575 long-term):**
  (a) Add `concept:sql-query-row-string` (and similar monomorphic
      forms) to provekit-shim-rusqlite's `provides_concepts`.
  (b) Redesign user-side function to match the shim's 4-param form
      (carries a mapper closure). **THIS DEMO PATH.**
  (c) Add a kit-side adaptation: when the user declares fewer params
      than the shim concept's canonical form, the kit synthesizes a
      default mapper.

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
