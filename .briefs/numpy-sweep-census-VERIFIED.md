# NumPy pytest sweep — VERIFIED census (architect reproduction, FULL numpy)

Date: 2026-06-04. Branch: codex/python-proven-case-1-nullguard (worktree).
Architect re-ran everything. The background agent's first report was INVALID:
it ran the STALE `/usr/local/lib` 49KB lifter (not the worktree 166KB fixed
kit), and its reconciliation read the wrong warning field. Numbers below are
the FIXED worktree kit + worktree rust binary, over ALL numpy test files.

## Corpus
ALL numpy test files in the extracted source: **179** `test_*.py`
(`/tmp/provekit-classshape-src-20260603/src/numpy/**`). Lift is AST-only —
numpy need not be installed/importable.

## Run setup (reproducible, footgun-proof)
- Project `.provekit/{config.toml, lift/python-tests/manifest.toml}`.
- Manifest `command` uses ABSOLUTE PYTHONPATH at the worktree kit src + python3.14
  (NOT the global `provekit-lift-python` console script, which is STALE 49KB).
- Sanity-checked `provekit_lift_py_tests.layer2.__file__` -> worktree src, and
  `hasattr(_coalesce_same_named_decls) == True`.
- Rust binary: worktree `implementations/rust/target/release/provekit`.
- Flat copy with collision-safe names (9 numpy basenames collide across subdirs);
  the surface does NOT discover recursively, so a nested tree mints empty/errors.

## Lift-layer trichotomy (architect's own reconciliation, qualified-aware, warning-first)
- files 179, parse/lift errors 0
- test fns yielded 6976; with >=1 assert **1085**
  - **cleanly lifted (no warning): 106**
  - **loud-refused (>=1 warning): 979**
  - **SILENT (Δ>0): 0**
  - enumeration misses: 0
- WARNING-FIRST split is the honest one: the nothing-silent catch-all does BOTH
  `claimed_tests.add` AND `warnings.append`, so catch-all refusals also sit in
  claimed_tests. Counting "claimed" as proven overstates it (would read 736).
  proven := claimed AND no warning. "979 refused" includes PARTIAL lifts (a test
  with >=1 unliftable assert is counted refused even if other asserts lifted).

## Prove-layer (z3), full 179-file mint+prove
- mint ok, proof bytes 2,403,824, 0 empty-set / lifter-not-found.
- prove: **344 discharged, 0 violations, 0 load errors.**
  - 344 callsite-consistency obligations reached z3 and were SAT (consistent).
    344 > 106 because one test yields multiple callsite obligations (per-arg EUF,
    per-row parametrize, multiple value-scope bases), and partial-lifts of
    "refused" tests still contribute dischargeable obligations.
  - EUF on real numpy: `Fraction#euf#c:callresult_Fraction_a2(i:3,i:2)`,
    `f#euf#c:callresult_f_a1(i:1023)`; parametrize per-row `::parametrize::...::row0`.

## Findings
1. **numpy IS a pytest suite** — lifter applies directly.
2. **Δ=0 holds across ALL of numpy** (lift layer): every assert-bearing test is
   lifted (fully/partially) or LOUDLY refused; 0 silent; 0 enumeration misses;
   0 parse/load errors.
3. **No teeth (0 contradictory, 0 violations).** This is the EXPECTED null result,
   NOT a discovery: a green suite's asserts about a subject are jointly satisfiable
   by construction (if they contradicted, the test would fail). numpy has nothing
   to falsePass ON. The teeth/falsePass evidence lives in the discrimination
   fixtures (test_contradictory, contradictory_binding), where the lifter DOES
   refuse a planted contradiction. EUF over-unification also cannot manufacture a
   falsePass (it only ADDS constraints → toward UNSAT/refuse), and there were 0
   refusals at prove anyway.
4. **consistency != correctness.** "344 discharged" = "test assertions mutually
   consistent about callsite X", NOT "numpy proven correct".
5. **979 loud-refused** = the residual work-list (NOT Δ): multi-assert
   characterization tests, deep attribute chains, subscript-of-attribute,
   `self.method(...)` receivers, chained comparisons, isinstance on np types,
   parametrize/mixed-body bodies. Each is a candidate for a future lifting
   enhancement; all are conservative-correct today.

## Scope caveat (one honest sentence)
Δ=0 here = nothing-silent at the LIFT layer (claimed-or-warned). It does NOT
mean 1085 obligations reached z3: 344 did. Single-assert / parametrize / refused
tests are "tagged and bagged" or loudly refused per T's definition, so they
legitimately count toward Δ=0 without each hitting the solver.

## Two traps this run exposed (caught by reproducing, not trusting)
- STALE KIT: a `command` resolving a console script from PATH can run an old
  globally-installed copy. Pin ABSOLUTE PYTHONPATH at worktree src; verify __file__.
- RECONCILIATION FIELD BUG (mine): `LiftWarning` carries the name in `.item_name`,
  not `.symbol`/`.args`. Wrong field made 7 loud-refused tests look SILENT.
