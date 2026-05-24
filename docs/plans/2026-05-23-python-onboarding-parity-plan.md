# Python onboarding parity plan (mirror Go #1445)

## STEP 1 empirical result (real python source lifter on `def double(x: int) -> int: return x * 2`)

Driven via `python3 -m provekit_lift_python_source --rpc`, the real source lifter emits a
`function-contract` for `double.double`:

```
post = (= (var return_value) (python:return (python:mul (var x) (const 2 Int))))
formals = ["x"], formalSorts = [Value], returnSort = Value
```

Gaps vs the z3-dischargeable shape the verifier's `body_discharge::CatalogResolver` expects
(`RESULT_VAR = "result"`, core SMT ops, `value_expr()` over `post`):

1. **Result var**: emits `return_value`; verifier needs `result`. (Go emits `result`.)
2. **`python:return` wrapper**: the value-expr is wrapped in `python:return(...)` inside the
   post; verifier needs `result == <value-expr>` directly (bare op). Go has no wrapper.
3. **Namespaced ops**: emits `python:mul` (like Go's `go:mul`); z3 needs core `*`. Needs a
   `coreArithOp`-style normalization (mirror Go `NormalizeCoreArith`).
4. **Sorts = `Value`**: z3 cannot discharge integer arith with sort `Value`; needs `Int`
   from the `: int` annotation. Refuse when arithmetic body lacks an int annotation.

Conclusion: python needs a verify-facing dialect transform (more gaps than Go's single
op-normalization, but same KIND of work). Buildable; not a stop-and-call gap (advisor concurred).

### Div soundness (Go lesson, applied preemptively)
`python:div` (true div `/`), `python:floordiv` (`//`), `python:mod` (`%`) MUST stay
namespaced (uninterpreted). Python `//`/`%` floor toward -inf (matches SMT-LIB `div`/`mod`
on that axis) BUT python true `/` is float and `//` semantics still differ from truncation;
to stay sound and mirror Go, leave ALL three uninterpreted -> Undecidable, never a false
discharge. Regression mirrors `cmd_verify_go_division_unsound.rs`.

## STEP 2 build
- New verify-facing transform in `provekit_lift_python_source` (e.g. `verify_dialect.py`):
  takes a lifted `function-contract`, returns the dischargeable form (result var, unwrap
  return, core ops, Int sorts) or refuses (returns None / diagnostic) when not faithful.
- New leaf-assertion harvester for pytest `assert double(3) == 6` -> `contract{inv = =(double(3), 6)}`
  with `double(3)` a `ctor`. Mirror Go `LiftLeafAssertions` whitelist.
- New binary `provekit-lift-python-verify` (`--rpc`): emits the verify-facing
  function-contracts (+ `bridgeSourceSymbol`) from non-test .py + harvested callsites from
  `*_test.py` / `test_*.py`. Modes: bare / bindings(library-bindings) / contracts(ir-document)
  mirroring Go's `liftMode`.
- `examples/python-double/` fixture: `double.py`, `test_double.py`, `.provekit/config.toml`,
  `.provekit/lift/python/manifest.toml`.
- Rust test `cmd_verify_python_production_bridge.rs` (mirror Go): real lifter -> mint
  (auto-bridge, assert via `bridges_by_symbol`) -> verify positive (discharge, witness,
  exit 0) + negative (broken body x*3 -> unsatisfied, exit 1, no witness).
- Rust test `cmd_verify_python_division_unsound.rs`: `halve(x) = x // 2`, assert
  `halve(-7) == expected` -> undecidable, no witness, exit 3.

## STEP 3 authoring surface
Python ALREADY has `@sugar.bind(concept=, library=)` -> `library-sugar-binding-entry` and
`@contract(pre=,post=,inv=)` decorators (bind_lifter.py). Parity gap: gate the verify-facing
function-contract emission on a `@provekit.boundary(...)` / `@provekit.sugar(...)` declaration
(mirror Go `AnnotatedOnly`) so "declare -> get contract" holds. Close loop: declare -> contract
-> discharge (STEP 2) -> realize back to python via existing realize kit.
