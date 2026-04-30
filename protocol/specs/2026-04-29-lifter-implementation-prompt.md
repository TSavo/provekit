# Lifter Implementation — Teaching Prompt

> Dispatch this prompt to the `implementer-against-spec` agent.
> Single commit. Conventional commit format. Co-Authored-By: Claude Sonnet 4.6.

## Stakes

The TS-IR language spec describes the framework's authoring surface — the typed
subset of TypeScript that humans, LLMs, and the framework itself use to declare
invariants. The lifter is the projector that walks `tsc.Program` AST and
produces canonical `IrFormula` values from in-subset TypeScript expressions.

Without the lifter:
- `.invariant.ts` files are markdown — they don't produce verification artifacts
- The catalog seeds (`protocol/specs/builtins-catalog/parseInt.invariant.ts`,
  `Math.invariant.ts`) cannot be loaded by anything
- User invariants cannot be verified
- `provekit prove` and `provekit generate` cannot be built
- The framework remains theoretical

With the lifter, the spec becomes operational. The catalog files become real
mementos. User-authored invariants flow through the canonicalizer (already
shipped) into the memento store (already shipped) and become content-addressed,
signed verification artifacts.

**This is the load-bearing piece for v1.** The lifter is plumbing, but it's
THE plumbing. Get it right; the rest of the framework cashes out from here.

## Read first

In order. One-line rationale per file:

1. **`protocol/specs/2026-04-29-ts-ir-language.md`** — THE SPEC. The canonical
   description of the TS subset, the lift rules, the file-anchoring constraint,
   the API. Read sections 3, 4, 5, 6, 9 carefully (file anchoring, quantifier
   syntax, sort marking, the IN/OUT subset, the lifter's per-AST-node dispatch
   table).

2. **`src/ir/index.ts`** — what `provekit/ir` currently exports. The lifter
   produces values that match these exports. The `IrFormula` type is from
   `formulas.ts`; the canonicalizer (already shipped) consumes that exact shape.

3. **`src/ir/formulas.ts`** — the `IrFormula` discriminated union. EVERY lift
   rule's output must be a valid `IrFormula` value. The canonicalizer's
   exhaustive switch will reject anything else.

4. **`src/canonicalizer/irFormula.ts`** — re-exports `IrFormula` from the IR
   library. Confirms the lifter's output type. Don't redefine; import.

5. **`src/canonicalizer/canonicalize.ts`** — what the canonicalizer expects as
   input. The lifter's output flows directly into this. Sanity-check that the
   shapes line up.

6. **`protocol/specs/builtins-catalog/parseInt.invariant.ts`** — the worked
   example. The lifter MUST be able to consume this file and produce the
   expected IrFormula values for each property declaration. Use it as the
   primary fixture for tests.

7. **`protocol/specs/builtins-catalog/Math.invariant.ts`** — second fixture.
   Demonstrates `forAll`, `exists`, `implies`, member access, registry calls
   (`Math.abs`, `Number.isInteger`).

8. **`src/ir/property.ts`** — current `property()` builder. The spec describes
   a SURFACE form `property(name, formula)`; the current library has a different
   shape `property({ name, scope, bindings, formula })`. The lifter's job is to
   project the SURFACE form into the existing builder form (or into IrFormula
   directly). Reconcile this; the spec wins on surface; bridge mechanically.

## What you are building

**Module:** `src/ir/lift/`

**Files:**
- `src/ir/lift/index.ts` — public API: `liftProject(programOrConfigPath)` →
  `LiftResult`
- `src/ir/lift/visitor.ts` — the visitor pass over `tsc.Program` AST
- `src/ir/lift/rules.ts` — per-AST-node dispatch table (the lift rules from
  spec §9)
- `src/ir/lift/sorts.ts` — sort resolution from TypeScript type annotations
- `src/ir/lift/anchoring.ts` — `.invariant.ts` file-anchoring enforcement
- `src/ir/lift/registry.ts` — pure-function registry interface and default
  TS-kit registry
- `src/ir/lift/diagnostics.ts` — error reporting (use `tsc`'s Diagnostic
  interface)
- `src/ir/lift/lift.test.ts` — test suite

**Public API:**

```typescript
// src/ir/lift/index.ts
import type { Program } from "typescript";
import type { IrFormula } from "../formulas.js";

export interface LiftedProperty {
  name: string;
  formula: IrFormula;
  sourceLocation: { filePath: string; line: number; column: number };
  filePath: string;
}

export interface LiftResult {
  properties: LiftedProperty[];
  diagnostics: Diagnostic[];
}

export function liftProject(program: Program): LiftResult;
export function liftFile(filePath: string, program: Program): LiftedProperty[];
export function liftFormulaExpression(
  expression: ts.Expression,
  context: LiftContext,
): IrFormula;
```

`liftProject` is the entry point. It walks every `*.invariant.ts` file in the
project and lifts every `property(...)` call. It returns the full set of
lifted properties plus diagnostics for any rejection.

`liftFile` is for testing: lift one file in isolation.

`liftFormulaExpression` is the core: lift a single expression to IrFormula.
Used by both file-lifting and tests.

## Tasks

### Task 1 — Set up the module skeleton

Create `src/ir/lift/` with index.ts and stubs for the other files. Make
`liftProject` return an empty result. Add the test file with one passing
test that imports from `./index.js`. Compile clean with `npx tsc --noEmit`.

Commit only at the end of all tasks; this is task structure, not commit
structure.

### Task 2 — File-anchoring enforcement (spec §3)

In `src/ir/lift/anchoring.ts`, implement:

```typescript
export function checkAnchoring(file: SourceFile): Diagnostic[];
```

Returns a diagnostic for every `provekit.property` (or destructured `property`,
or `provekit/ir` import's `property`) call site found in a file whose path
does NOT match `*.invariant.ts`. The diagnostic reads:

```
provekit.property may only appear in .invariant.ts files. 
Move this declaration to <co-located-name>.invariant.ts.
```

Test cases:
- `src/billing/invoice.ts` containing `property(...)` → 1 diagnostic
- `src/billing/invoice.invariant.ts` containing `property(...)` → 0 diagnostics
- `src/billing/invoice.ts` with no `property(...)` calls → 0 diagnostics
- `src/billing/invoice.invariant.ts` with no `property(...)` calls → 0
  diagnostics

### Task 3 — Sort resolution (spec §5)

In `src/ir/lift/sorts.ts`, implement:

```typescript
export function resolveSort(
  type: ts.Type,
  checker: ts.TypeChecker,
): Sort | null;
```

Returns the corresponding `Sort` (from `src/ir/sorts.ts`) for primitive
brands (`Int`, `Real`, `Bool`, `StringSort`) and user-defined branded types
(types with a `__sort` property). Returns `null` for unrecognized types.

Test cases:
- Lambda param `(x: Int) => ...` → `Sort = Int`
- Lambda param `(x: Real) => ...` → `Sort = Real`
- Lambda param `(x: Cents) => ...` (branded with `__sort: 'Cents'`) → `Sort = primitive('Cents')`
- Lambda param `(x: number) => ...` (no brand) → `null` (rejected)

### Task 4 — Per-AST-node dispatch (spec §9)

In `src/ir/lift/rules.ts`, implement the lift rule for each AST node listed
in spec §9's table. This is the core work.

Each rule has the shape:

```typescript
function liftBinaryAnd(
  node: ts.BinaryExpression,
  context: LiftContext,
): IrFormula {
  // node.operatorToken.kind === SyntaxKind.AmpersandAmpersandToken
  return {
    kind: "and",
    conjuncts: [
      liftFormulaExpression(node.left, context),
      liftFormulaExpression(node.right, context),
    ],
  };
}
```

Cover ALL node kinds from §9's table:
- BinaryExpression (`&&`, `||`, `===`, `!==`, `<`, `<=`, `>`, `>=`, `+`, `-`,
  `*`, `/`, `%`, `??`)
- PrefixUnaryExpression (`!`, `-`)
- ConditionalExpression (ternary)
- CallExpression — three sub-cases:
  - Method calls: `xs.every(...)`, `xs.some(...)` → ForAll/Exists
  - Library calls: `forAll<T>(...)`, `exists<T>(...)`, `implies(a, b)`, `iff(a, b)`
  - Registry calls: any function in the pure-function registry → Atomic predicate
- Identifier (lambda param vs const-bound; resolve via TypeChecker)
- PropertyAccessExpression (member access)
- OptionalChainingExpression — desugar to ternary
- NumericLiteral, StringLiteral, TrueKeyword, FalseKeyword, NullKeyword,
  UndefinedKeyword

For any node kind NOT in the IN list (spec §6.1), emit a diagnostic and return
a sentinel "unliftable" IrFormula (or throw a `LiftError` that the visitor
catches; pick whichever cleanly threads diagnostics).

### Task 5 — The visitor pass (spec §9)

In `src/ir/lift/visitor.ts`, implement the visitor that walks
`tsc.Program`'s files and finds `property(name, formula)` call sites. For
each call site:

1. Extract the name (first arg, must be string literal — diagnostic if not)
2. Extract the formula expression (second arg)
3. Lift the formula via `liftFormulaExpression`
4. Construct a `LiftedProperty` with location info

The visitor uses `ts.forEachChild` to walk. Skip files whose path doesn't
end in `.invariant.ts` — those are not subject to lifting (anchoring rejection
happens separately).

### Task 6 — Pure-function registry (spec §11)

In `src/ir/lift/registry.ts`, define:

```typescript
export interface RegistryEntry {
  name: string;             // e.g. 'parseInt', 'Math.abs'
  signatureSorts: Sort[];   // parameter sorts, in order
  returnSort: Sort;         // return sort
}

export interface PureFunctionRegistry {
  has(name: string): boolean;
  get(name: string): RegistryEntry | undefined;
}

export function defaultTsKitRegistry(): PureFunctionRegistry;
```

`defaultTsKitRegistry` returns a registry containing AT MINIMUM:

- `parseInt`, `parseFloat`, `isNaN`, `isFinite`
- `Number.isInteger`, `Number.isFinite`, `Number.isNaN`, `Number.parseInt`,
  `Number.parseFloat`
- `Math.abs`, `Math.max`, `Math.min`, `Math.floor`, `Math.ceil`, `Math.round`,
  `Math.sign`, `Math.sqrt`, `Math.pow`, `Math.log`, `Math.exp`
- `String.prototype.length` (read), `String.prototype.charAt`,
  `String.prototype.charCodeAt`, `String.prototype.includes`,
  `String.prototype.startsWith`, `String.prototype.endsWith`
- `Array.prototype.length` (read), `Array.prototype.includes`,
  `Array.prototype.indexOf`, `Array.prototype.at`

These entries are stub-shape (correct sorts; no symbolic-range contracts yet —
those come in a future task). The lift rule for CallExpression checks the
registry: if the callee is registered, lift as Atomic with `predicate: name`;
otherwise, diagnostic.

### Task 7 — Wire the public API and integration test

In `src/ir/lift/index.ts`, expose the public API. Add a fixture-based
integration test that:

1. Constructs a `tsc.Program` from a temporary directory containing a copy
   of `protocol/specs/builtins-catalog/parseInt.invariant.ts` (or use that file
   directly via project root configuration).
2. Calls `liftProject(program)`.
3. Asserts:
   - At least 17 `LiftedProperty` entries exist (matches parseInt's catalog)
   - Each property's `name` matches the expected string ("parseIntCanReturnZero",
     etc.)
   - Each property's `formula` is a valid `IrFormula` (compiles past
     canonicalization without error)
   - Diagnostics is empty (no anchoring violations, no unliftable nodes)

### Task 8 — Test the negative cases

Add tests for each major rejection case:

- `for...of` loop in invariant body → diagnostic
- Mutation in invariant body → diagnostic
- `await` in invariant body → diagnostic
- `try/catch` in invariant body → diagnostic
- Closure over reassigned `let` → diagnostic
- Call to unregistered function → diagnostic
- `provekit.property` in non-`.invariant.ts` file → anchoring diagnostic

Each test asserts the specific diagnostic message and the source location.

### Task 9 — Run the full suite

Verify all existing tests still pass plus the new lift tests:

```
npx vitest run src/ir/lift
```

Expected: ALL new tests pass. Full-suite test count grows by ~30 tests.

```
npx tsc --noEmit
```

Expected: clean. No new type errors.

## Good vs bad examples

### Good — clean dispatch table per AST node

```typescript
// rules.ts
const BINARY_OP_RULES: Record<ts.SyntaxKind, BinaryRule> = {
  [ts.SyntaxKind.AmpersandAmpersandToken]: liftBinaryAnd,
  [ts.SyntaxKind.BarBarToken]: liftBinaryOr,
  [ts.SyntaxKind.EqualsEqualsEqualsToken]: liftBinaryEqEqEq,
  // ...
};

export function liftBinaryExpression(
  node: ts.BinaryExpression,
  context: LiftContext,
): IrFormula {
  const rule = BINARY_OP_RULES[node.operatorToken.kind];
  if (!rule) {
    context.diagnostics.push({
      file: node.getSourceFile(),
      start: node.getStart(),
      length: node.getWidth(),
      messageText: `Operator ${ts.SyntaxKind[node.operatorToken.kind]} is not allowed in IR.`,
      category: ts.DiagnosticCategory.Error,
      code: 0,
    });
    return UNLIFTABLE;
  }
  return rule(node, context);
}
```

This is good because: clear dispatch, single source of truth for the rule
table, diagnostics threaded through context, easy to extend.

### Bad — monolithic if-else

```typescript
// DON'T DO THIS
function liftBinaryExpression(node: ts.BinaryExpression, context: LiftContext): IrFormula {
  if (node.operatorToken.kind === ts.SyntaxKind.AmpersandAmpersandToken) {
    return { kind: "and", conjuncts: [liftFormulaExpression(node.left, context), liftFormulaExpression(node.right, context)] };
  } else if (node.operatorToken.kind === ts.SyntaxKind.BarBarToken) {
    return { kind: "or", conjuncts: [liftFormulaExpression(node.left, context), liftFormulaExpression(node.right, context)] };
  } else if (node.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken) {
    // ... 15 more branches
  } else {
    throw new Error("unsupported operator");
  }
}
```

This is bad because: the dispatch table is tangled into a chain, adding new
operators requires reading 200 lines of if-else, and the diagnostic message
is unhelpful.

### Good — sort resolution that handles brands

```typescript
// sorts.ts
export function resolveSort(
  type: ts.Type,
  checker: ts.TypeChecker,
): Sort | null {
  // Primitive brand check: look for `__sort` property
  const brandProperty = type.getProperty("__sort");
  if (brandProperty) {
    const brandType = checker.getTypeOfSymbolAtLocation(
      brandProperty,
      brandProperty.valueDeclaration!,
    );
    if (brandType.isStringLiteral()) {
      return primitiveSort(brandType.value);
    }
  }
  // No brand — reject (must be branded type for sort)
  return null;
}
```

### Bad — assuming TypeChecker symbols are always present

```typescript
// DON'T DO THIS
export function resolveSort(type: ts.Type): Sort {
  const name = (type as any).symbol.name; // CRASHES on primitives
  return { kind: "primitive", name } as Sort;
}
```

This is bad because: it assumes `.symbol` exists (it doesn't for primitives),
casts through `any`, and produces invalid `Sort` values for unbranded types.

## Quiet parts

These are non-obvious from the spec but matter:

1. **`tsc.Program` construction.** You'll need to construct a Program for
   testing. Use `ts.createProgram({ rootNames: [...], options: { strict: true,
   target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, moduleResolution:
   ts.ModuleResolutionKind.NodeNext } })`. Strict mode is required per spec.

2. **Path matching for anchoring.** Use `filePath.endsWith('.invariant.ts')`
   not regex. Robust against weird path separators on Windows (which doesn't
   matter for v1, but the assertion is cleaner).

3. **The current `property()` API mismatch.** The library's existing `property`
   takes `{ name, scope, bindings, formula }`. The spec's surface is `property(name, formula)`.
   Don't try to harmonize these in the lifter — the lifter takes the SURFACE
   form (string + lambda) and produces a `LiftedProperty` with name + formula.
   Don't call the existing `property()` builder. Don't depend on `BindingScope`
   or `Bindings`. The lifter's output is a fresh shape.

4. **Lambda variable scoping.** When lifting `forAll<Int>(x => x > 0)`, the
   `x` in the body is a TypeScript identifier that needs to resolve to an
   IR `var` term. Use a `LiftContext` that tracks bound variables in scope.
   Each quantifier lift pushes a binding; the corresponding `Identifier`
   lift pops the same name and produces a `var` IrTerm with the matching name.

5. **Optional chaining desugaring.** `obj?.field` lifts as
   `If(IsNull(lift(obj)), Undefined, Project(lift(obj), 'field'))`. But
   IrFormula's existing kinds don't have `If` or `Undefined` — those would
   be terms, not formulas. For v1, when desugaring optional chains, lift to
   a Boolean expression: `obj === null || obj === undefined ? false : (obj.field
   condition)`. Adjust per the actual usage context.

6. **Default registry seeding.** Don't hand-write 50 registry entries inline
   in `defaultTsKitRegistry()`. Define them in a structured way (array of
   tuples, or read from a JSON manifest). Future work expands the registry;
   make extension trivial.

7. **The catalog files at `protocol/specs/builtins-catalog/` use the SURFACE form.**
   They will not type-check against the current `provekit/ir` library because
   they use `provekit/ir`'s SPEC'D surface API, not its existing builder API.
   This is expected. The lifter you're building is what makes them lift-able.
   Don't try to make them type-check against the current library; that
   contradicts the spec.

8. **Test fixtures should NOT be the catalog files directly.** Copy minimal
   versions into `src/ir/lift/__fixtures__/` for testing. The catalog files
   at `protocol/specs/builtins-catalog/` are spec artifacts; they shouldn't be
   coupled to the lifter's test suite (different change cadence).

9. **`registry calls` lift to atomic predicates.** `Math.abs(x)` lifts to
   `{ kind: "atomic", predicate: "Math.abs", args: [lift(x)] }`. The atom's
   predicate string IS the function's registered name. The downstream
   prover/canonicalizer treats it as an opaque function symbol with
   registry-supplied semantics.

10. **Diagnostics, not exceptions.** The lifter accumulates diagnostics and
    keeps walking. A single rejection doesn't abort the whole project lift.
    The `LiftResult.diagnostics` is the comprehensive error report; the
    `properties` array contains everything that DID lift successfully.

## Cut list

Out of scope for v1:

- The lowerer (IR → TS source). Future task.
- Symbolic-range contracts in registry entries (just sort signatures for v1).
- Cross-file property composition via `ref("name")`.
- Recursion in predicate bodies.
- Higher-order quantifiers.
- Generic function calls (the registry's per-instantiation contract).
- Performance optimization. Correctness over speed.
- Watch-mode incremental lifting.
- The `provekit/ir` SURFACE API as an actually-runtime library. The lifter's
  output replaces it; we don't need a runtime `property()` that takes
  `(name, formula)` arguments.
- IDE / LSP integration.
- COBOL/Rust/Python kit lifters. TS only for v1.

## Verify

```
cd /Users/tsavo/provekit
npx vitest run src/ir/lift
```

Expected: all new tests pass. Test count for `src/ir/lift` is ~30+.

```
npx vitest run src/ir src/canonicalizer
```

Expected: existing tests in `src/ir` and `src/canonicalizer` continue to pass.
No regressions.

```
npx tsc --noEmit
```

Expected: clean. No new type errors. Existing errors (if any) unchanged.

```
npx vitest run
```

Expected: full-suite count increases by ~30. Pre-existing failures (the
formulate.test.ts ones, structuredOutput.test.ts ones, etc.) are owned by
parallel work and are not your concern.

## Project conventions to apply

- pnpm, not npm. Don't introduce npm or yarn artifacts.
- Drizzle migrations live in `drizzle/`. The lifter doesn't touch the DB; not
  applicable here.
- `node:crypto` preferred over external crypto. The lifter doesn't need crypto.
- No external dependencies beyond `typescript` (already in `package.json`).
- No emojis in code, comments, commit messages, or output.
- Comments only when WHY is non-obvious. Default to none.
- Tests use vitest. Mock via `vi.hoisted` only when truly needed.

## Commit

Single commit. Conventional commit format:

```
feat(lift): TS predicate lifter — projects type-checked TS into IrFormula

Implements src/ir/lift/ per protocol/specs/2026-04-29-ts-ir-language.md. The
lifter is the operational realization of the spec's "TypeScript IS the IR"
claim — visitor pass over tsc.Program, dispatches per AST node type, produces
IrFormula values that flow into the existing canonicalizer.

Files added:
- src/ir/lift/index.ts (public API)
- src/ir/lift/visitor.ts (the walker)
- src/ir/lift/rules.ts (per-node dispatch table)
- src/ir/lift/sorts.ts (sort resolution from type annotations)
- src/ir/lift/anchoring.ts (.invariant.ts file enforcement)
- src/ir/lift/registry.ts (pure-function registry; default TS-kit set)
- src/ir/lift/diagnostics.ts (error reporting via tsc.Diagnostic)
- src/ir/lift/lift.test.ts (~30 tests)

Spec compliance: file anchoring rejection (§3), quantifier syntax (§4), sort
marking (§5), IN/OUT subset (§6), per-AST-node dispatch (§9), pure-function
registry (§11). Out of scope per spec cut list (§17): lowerer, recursion,
higher-order quantifiers, generic instantiation contracts.

Tests: ~30 new in src/ir/lift; existing src/ir + src/canonicalizer tests
unchanged. Full-suite count grows by ~30.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

Single commit. Push to the dispatch branch (you're working in a worktree).
Report SHA, test count delta, and any spec gaps surfaced during implementation
(diagnostics about ambiguous spec language, missing subset coverage, etc.).
