# provekit-lift-go-tests

Layer 2 structural lift adapter for ProvekIt. Walks `*_test.go` Go
source via `go/parser` + `go/ast` and emits canonical IR mementos for
three patterns Layer 0 cannot recognize:

1. **Bounded for-loop -> universal quantifier.** A `for i := lo; i < hi; i++ { <single assertion> }` shape lifts to `forall i: Int. (lo <= i AND i < hi) implies <assertion>`. The loop variable name is preserved verbatim in the IR (NOT replaced with the kit's auto-named `_xN` placeholder) so the canonical CID is stable across runs and across host languages. Memento name: the test fn name itself.
2. **Helper-function inlining.** A non-test function with one typed parameter and a body of exactly one liftable assertion call becomes a per-call-site inline. Each call site emits one memento named `<test>::call::<i>`, with the helper's lifted assertion having the formal parameter substituted for the call-site argument.
3. **Multi-assertion characterization conjunction.** When a test body is two-or-more recognized top-level assertion calls, the test folds to a single `and(...)` memento with the test fn name. If fewer than two atoms lift, the claim is RELEASED so Layer 0 can fall back to single-assertion atomic minting.

## Dispatch model

Layer 2 runs FIRST and returns a `Layer2Output.ClaimedTests` set: every
test fn name this pass took ownership of, regardless of whether the
final lift succeeded. The dispatcher passes that set to Layer 0's
skip-list so Layer 0 will not also emit decls for those tests. The two
layers PARTITION the test fns; nothing double-counts. Pattern 3 is the
one exception: when fewer than two atoms lift, the claim is dropped
mid-flight so Layer 0 can still mint the individual asserts. Skipped
patterns (nested loops, non-identifier loop vars, non-literal range
endpoints) keep their claim and surface a structured warning under the
`go-tests-layer2` adapter tag.

## Cross-language byte equivalence

The mementos this adapter emits canonicalize to the same bytes (and
therefore the same BLAKE3-512 CID) as the Rust and TypeScript Layer 2
adapters' output for the same proposition. The `provekit-ir-symbolic`
Go package owns the JCS-compatible JSON encoding (locked key order,
verbatim unicode atomic predicates, no HTML escaping); the
`canonicalizer` companion owns the `blake3-512:` self-identifying CID
prefix.

## Configuration

This package is selected via `.provekit/config.toml`:

```toml
[authoring]
surface = "go-tests"
```

Auto-detection is intentionally not done; the surface is a project
declaration so two adapters never race for the same file.
