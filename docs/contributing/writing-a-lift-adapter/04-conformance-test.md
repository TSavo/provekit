# Writing a lift adapter, step 4: conformance test

A lift adapter is conformant if, given a fixed source-library input, it produces fixed canonical IR output. The conformance test is the byte-equality check.

This is similar in shape to the kit conformance gate (writing-a-kit/01) but smaller in scope: a kit covers the protocol layers, while a lift adapter covers a single library's annotation surface.

## What a fixture looks like

Each fixture is a pair: a source-library input and a canonical IR output.

```
implementations/<your-language>/
└── lift-adapters/<adapter-name>/
    └── conformance/
        ├── fixtures/
        │   ├── 01-min-max/
        │   │   ├── source.<ext>            # the input
        │   │   └── expected.json           # the canonical IR (pretty-printed for review)
        │   ├── 02-string-pattern/
        │   ├── 03-not-null/
        │   └── ...
        └── runner
```

The source file is a small, focused piece of code in the source library's idiom. The expected file is the canonical IR formula(s) the adapter should produce, pretty-printed so reviewers can read it.

## What the runner does

The runner program:

1. Reads a fixture directory.
2. Runs your lift adapter on `source.<ext>`.
3. Captures the lifted canonical IR.
4. Canonicalizes it (the runner re-canonicalizes after pretty-printing, so the test is byte-equality).
5. Compares against `expected.json`.

The test fails if the actual output diverges from expected at any byte.

The harness wires this in alongside the kit conformance: `make conformance` runs both the kit fixtures and every shipping adapter's fixtures.

## Choosing your fixture set

Aim for coverage by category, not exhaustiveness. A useful starting set covers each annotation kind with a small, focused example:

For Bean Validation, fixtures might be:

- `01-not-null`: a class with `@NotNull` on one field.
- `02-min-max`: `@Min(0) @Max(150)` on an `int`.
- `03-size`: `@Size(min=5, max=10)` on a `String`.
- `04-pattern`: `@Pattern(regexp=...)` on a `String`.
- `05-email`: `@Email` on a `String`.
- `06-positive`: `@Positive` on an `int`.
- `07-conjunction`: a class with multiple fields each having multiple annotations.

For zod:

- `01-string-min`: `z.string().min(5)`.
- `02-number-int`: `z.number().int()`.
- `03-object-nested`: `z.object({ user: z.object({ name: z.string() }) })`.
- `04-array`: `z.array(z.string())`.
- `05-union`: `z.union([z.string(), z.number()])`.
- `06-optional`: `z.string().optional()`.

Eight to fifteen fixtures is a good first scope. More can come later.

## Cross-adapter parity fixtures

If your adapter is one of several that should produce equivalent canonical IR (e.g., the Bean Validation adapter and the JML adapter both produce the same canonical IR for `@NotNull`), add cross-adapter parity fixtures:

```
conformance/cross-adapter-parity/
├── 01-not-null/
│   ├── bean-validation-source.java     # @NotNull
│   ├── jml-source.java                 # //@ requires email != null
│   ├── spring-source.java              # @RequestParam(required=true)
│   └── expected.json                   # the shared canonical IR
```

The runner runs all three adapters on their respective sources and verifies all three produce identical canonical IR bytes. This is what made the Java kit's cross-domain equivalence claim auditable.

If your adapter is the only adapter for a constraint, skip this. If there's a sibling adapter (in your kit or another), add the parity fixture; it's the strongest signal of correctness.

## What "expected.json" should look like

Pretty-printed JSON, for human review. The runner re-canonicalizes before comparison; the human-readable form is just so reviewers can sanity-check what your adapter is claiming the canonical IR is.

Example for `@Min(0) @Max(150)` on `int age`:

```json
{
  "kind": "forall",
  "var": "age",
  "sort": {"kind": "primitive", "name": "Int"},
  "body": {
    "kind": "and",
    "left": {
      "kind": "atomic",
      "predicate": "ge",
      "args": [
        {"kind": "var", "name": "age"},
        {"kind": "const", "type": "Int", "value": 0}
      ]
    },
    "right": {
      "kind": "atomic",
      "predicate": "le",
      "args": [
        {"kind": "var", "name": "age"},
        {"kind": "const", "type": "Int", "value": 150}
      ]
    }
  }
}
```

The runner JCS-canonicalizes both your adapter's output and the expected file, then byte-compares. Pretty-print is for review only.

## Updating fixtures

Fixtures are pinned. When you add a new annotation kind to your adapter, add a fixture. When you fix a bug in canonicalization that changes the canonical bytes, update every affected fixture in one PR. Reviewers should see exactly which canonical IR bytes changed and why.

This is how you avoid silent canonicalization drift: every byte change is reviewed.

## Cross-kit parity

If your adapter is not Rust, your adapter's canonical IR must agree with what the equivalent Rust adapter would produce (if a Rust adapter exists for the same library). Where parity exists, add cross-kit parity tests.

Practically, this means: when the TypeScript `provekit-lift-zod` adapter produces canonical IR for `z.string().min(5)`, those bytes must match what a hypothetical Rust `provekit-lift-zod` adapter would produce for the equivalent expression. Today there is no Rust zod adapter; cross-kit parity for zod is moot. But if a Python `provekit-lift-zod` adapter were added, parity would be required.

The general rule: equivalent inputs produce equivalent canonical IR, regardless of which kit's adapter ran. Conformance fixtures enforce this.

## When this step is done

Your adapter's `conformance/fixtures/` directory has at least one fixture per annotation kind your adapter claims to support. The runner is wired into `make conformance`. CI fails if any fixture's canonical bytes drift.

The adapter is now load-bearing: any future change that affects canonical IR is caught by the fixtures, reviewed, intentional.

## Read next

- [05-publishing.md](05-publishing.md) — naming, versioning, distribution.
- [docs/contributing/adapter-coverage-rubric.md](../adapter-coverage-rubric.md) (when written) — what counts as good coverage.
