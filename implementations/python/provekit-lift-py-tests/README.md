# provekit-lift-py-tests

Layer 2 structural lift adapter for Python tests (pytest + unittest), plus a
production-side WP walker for Python callsite composition. It walks the `ast`
of a source file, recognizes structural test patterns Layer 0 cannot, and
emits canonical IR mementos with content-addressed BLAKE3-512 hashes that are
byte-identical to the Rust, TypeScript, and C++ canonicalizers.

## Patterns

1. **Bounded `for` loop -> `forall`-implies.** `for i in range(lo, hi):` and
   `for i in range(hi):` are lifted as `forall i:Int. (lo <= i AND i < hi) ⇒ φ(i)`.
   `for v in [1, 2, 3]:` becomes an enumerated `and`-conjunction. Memento
   name: `<test>::loop::<var>`.
2. **Helper inlining.** A helper with one TYPED parameter and a single
   liftable assertion gets inlined at each call site; mementos named
   `<test>::call::<i>` (zero-indexed in source order).
3. **Multi-assertion characterization.** >=2 top-level liftable asserts
   (`assert ...` or `self.assertEqual(...)` family) fold to one
   `and`-conjunction memento at `<test>`. <2 atoms releases the claim back
   to Layer 0.
4. **`@pytest.mark.parametrize` over a literal arg list.** Each row is
   substituted into the body, then folded to `and(...)`. Memento name:
   `<test>::parametrize::<param-names>`.
5. **Callsite value-scope facts plus implications.** Local assignments and
   simple `if/else` branches around pytest/unittest assertions become
   callsite-owned contracts named `<callee>@<file>:<line>:<col>::facts`
   and `<callee>@<file>:<line>:<col>::assertion`, plus an implication
   from facts to assertion. Tests describe these contracts; they do not own
   the emitted contract identity.

Out of scope: `hypothesis` (Layer 1 already), `pytest.raises`, fixtures,
parametrize over factories, multi-stmt loop bodies.

## Production WP walk

The production walker mirrors `provekit-walk` for Rust. It lifts callee
preconditions from defensive source patterns such as `if cond: raise ...` and
`assert cond`, substitutes actual arguments at each production callsite, then
walks backward through in-scope assignments with `wp(x := e, P) = P[e/x]`.
Each arrival is emitted as a callsite-owned contract edge with `pre` and
`post` slots plus a `python-wp-walk` implication from `pre` to `post`.

## Dispatch model

Layer 2 runs first and returns a `claimed_tests` set listing every test fn
it took ownership of (whether it lifted them or warned-and-skipped). The
dispatcher (Python Layer 0, when it lands) must skip those names so each
test fn is lifted by exactly one layer. Out-of-scope shapes return without
claiming the test, leaving it for Layer 0 / Layer 3. This adapter is
selected via `.provekit/config.toml` `[authoring] surface = "py-tests"`;
auto-detection is intentionally absent.

## Cross-language conformance

`tests/test_canonicalizer.py` and `tests/test_ir_conformance.py` pin the
JCS-encoded bytes and BLAKE3-512 CIDs against the Rust reference. The
unicode-glyph round-trip test (≥, ≤, ≠) is non-negotiable: the kit's
atomic predicate names use these codepoints, and cross-language hash
agreement depends on UTF-8 verbatim emission for U+0080+.

## Usage

```python
from provekit_lift_py_tests import lift_file_layer2

with open("tests/test_thing.py") as f:
    out = lift_file_layer2(f.read(), "tests/test_thing.py")

for decl in out.decls:
    print(decl.name, decl.inv)
for w in out.warnings:
    print(f"WARN {w.item_name}: {w.reason}")
print(f"claimed: {out.claimed_tests}")
```

## Tests

```bash
pip install -e '.[test]'
pytest -v
```
