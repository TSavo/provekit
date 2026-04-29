# C6 "near 'SQL': syntax error" — diagnosis (Move 2 / r6)

Task: provekit-integration #6
Auditor: c6-debugger (read-only)
Date: 2026-04-29

## Root cause: two-layer bug

The user-visible regression is *two distinct defects layered on each other*. Naming them clearly is the load-bearing part of the diagnosis.

### Layer A — observability (the cause of "regression"). FIXED.

The user reported: "C6 adversarial validation fails with `SqliteError: near 'SQL': syntax error`, but the caller sees only opaque `validation failed`."

The opacity (not the SQL error itself) is what made this a Move 2 / r6 regression: the framework was *able* to surface the error, but two `catch` blocks in `runAdversarialValidation` (`src/fix/principleGen.ts`) swallowed it before anyone could read it.

Both catches were resurfaced in commit `02951d8` ("fix(c6): surface adversarial-validation errors instead of swallowing them", 2026-04-29):

- `src/fix/principleGen.ts:603-619` — inner catch around `queryFn(fixtureDb)`. Pre-fix: silently counted 0 matches. Post-fix: logs `[C6 adversarial] principle '<name>' query failed against fixture: <SQLite error>`.
- `src/fix/principleGen.ts:622-627` — outer catch around the per-fixture run. Pre-fix: silently returned -1. Post-fix: logs `[C6 adversarial] fixture run failed: <message>`.

These pair with the *already-existing* trace at `src/dsl/compiler.ts:885-917` (`[sql:prepare-failed principle=...]`) which dumps the failing principle name, the principle AST, the compiled SQL, and the SQLite error message on any `prepare()` throw. Together, the diagnosis trail goes from silent → fully visible the next time the underlying SQL bug fires.

**Layer A is closed by `02951d8`.** No further code change is needed for the regression-of-observability that #6 was filed against.

### Layer B — the underlying malformed SQL. NOT YET CAPTURED.

The substantive defect (some DSL input compiles to SQL that SQLite rejects with a stray `SQL` token) has *never been captured in a log under the new diagnostics*. The triggering input is ephemeral: it is whatever the C6 LLM proposed during a failing run, never written to disk, discarded once the principle is rejected. Until Layer A's diagnostics fire on a fresh run, we have no AST to point at.

This is by design. The fix shape for Layer B is "capture, then patch" — it cannot be patched speculatively because the input space is open.

## Minimal repro: `scratch/c6-repro.ts`

`scratch/c6-repro.ts` is the isolation harness that #6 asked for. It bypasses the 25-minute fix-loop and exercises the full DSL → AST → `compileProgram` → `sqlite.prepare()` chain with one DSL source taken from stdin, `--src <path>`, or `--fixture <name>`.

Six built-in fixtures stress-test the most plausible "near 'SQL'" hypotheses:

| Fixture | Hypothesis | Result |
|---|---|---|
| `div-by-zero` | baseline (canonical exemplar) | OK |
| `principle-named-sql` | principle named `SQL` leaks into emitted SQL | OK — name only used in diagnostic strings, not in SQL |
| `calls-named-sql` | string literal `"SQL"` on a Text column | OK — `compiler.ts:397` escapes via `replace(/'/g, "''")` |
| `capture-named-sql` | capture key `SQL` becomes `__cap_SQL` alias | OK — valid SQLite identifier |
| `variable-named-sql` | DSL var `$SQL` → alias `node_SQL` | OK — valid identifier |
| `predicate-named-sql` | predicate named `SQL` | parser rejects (separate issue: parser keyword collision; not the SQL bug) |

All six prepare cleanly under `PROVEKIT_SQL_TRACE=1`. The harness is sound; it will *expose* the bug as soon as a real failing DSL is piped to it.

## Why the harness cannot enumerate the bug from outside

The DSL → SQL emission paths I audited cannot produce a stray `SQL` token from currently-known inputs:

- **Identifiers** (table/column/alias names) come from the runtime capability registry (`src/sast/capabilityRegistry.ts`). `resolveCapCol` (`src/dsl/compiler.ts:221-248`) and `getOrBindVar` (`compiler.ts:299-323`) both raise `CompileError` on lookup miss — so an unregistered name fails compile, never reaches SQL.
- **String literals** are single-quoted with `'` doubling (`compiler.ts:397`, `compiler.ts:604`). A literal containing `SQL` is fine.
- **Aliases** are mechanically generated (`cap_<capName>_<n>`, `sub_<capName>_<n>`, `node_<varName>`, `__cap_<captureName>`); none of these formats can introduce a stray bareword.
- **Relation SQL** is built by `descriptor.compile` from the relation registry (`src/dsl/relations.ts`). Relation-arg aliases trace back through `resolveRelationArgAlias` (`compiler.ts:678-756`) and are validated.

The most plausible Layer B candidates — to be confirmed when a real failing AST is captured:

1. The LLM emits a DSL string literal containing characters the compiler does not expect (e.g. backticks, embedded null bytes, or a multi-line literal whose newlines confuse a downstream stage).
2. A relation descriptor's `compile()` constructs malformed SQL for some arg shape (zero-length alias, deref into a node-ref column, etc.).
3. `buildTempDescriptor` (`src/fix/principleGen.ts:1409-1440`) — the stub Drizzle table used for Oracle #18's compile-only check — leaks into the adversarial run path. The `buildStubDrizzleColumn` (`principleGen.ts:1469-1473`) computes a fake `name` from the dslName via a regex; if a column name starts with an uppercase letter the regex emits a leading underscore that gets stripped, but other edge cases (numeric prefix, all-caps column) are not exercised by tests.

## Proposed fix shape

**Phase 1 (now) — no code change.** Layer A is fixed. Wait for the next adversarial run that fires the bug; the auto-diagnostic will print the full failing SQL + AST.

**Phase 2 (when captured) — promote to permanent fixture.** Paste the captured AST/SQL into `scratch/c6-repro.ts` as a new entry in `FIXTURES`, write the failing test, then patch the specific compiler path responsible. The likely shape will be one of:

- A targeted change in the relation descriptor or the SQL-literal escape if the input is a malformed string literal.
- A defensive identifier-quoting pass in `compiler.ts` (wrap every emitted identifier with double-quotes) so any future LLM hallucination of an SQLite-reserved word as a name fails-closed instead of fails-weird.

**Phase 3 (defense-in-depth, optional, regardless of Phase 2 outcome).** Add a unit test that asserts the SQL emitted for every registered capability/relation parses cleanly via `db.prepare()` against an empty schema. This catches the entire class — "compiler ever emits unparseable SQL" — without enumerating inputs.

## Files referenced

- `src/fix/principleGen.ts` — adversarial validation runner; the two surfaced catches.
- `src/dsl/compiler.ts` — DSL → SQL emission; `[sql:prepare-failed]` trace at line 885.
- `scratch/c6-repro.ts` — isolation harness; ready to receive a real failing DSL via stdin.
- `02951d8` — observability fix (Layer A).
