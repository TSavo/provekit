# TS-IR Language Specification

> The IR is a typed subset of TypeScript. Any LLM that can write JavaScript can write invariants — because the invariants ARE JavaScript, mechanically restricted by the lifter to the FOL-projectable subset. The TypeScript compiler IS the proof checker.

**Status:** Canonical specification for the TypeScript IR authoring surface.

**Companion specs:**
- `2026-04-29-verification-ir.md` — architectural identity (host language IS the IR)
- `2026-04-29-ir-library.md` — the IR library's brand types and builders (the lifter's emit target)
- `2026-04-29-ast-canonicalizer.md` — byte-identical hash machinery (consumes lifter output)
- `2026-04-29-per-language-kit-standard.md` — kit boundary constraints
- `2026-04-29-implementation-fungibility.md` — why the spec is durable across reimplementations

## 1. Thesis

The framework's value cashes out only if every TypeScript codebase — production, legacy, vibe-coded, hand-crafted — can be made provably correct **without modifying the existing code**. The IR cannot be a separate language with its own compiler, because that imposes a translation step every developer (human or LLM) has to perform. The IR cannot be a runtime library, because that requires the production code to import the framework. Either choice contaminates the artifact under verification.

The resolution: **TypeScript is the IR.** Specifically, a typed subset of TypeScript that the TypeScript compiler itself parses, type-checks, and resolves. The lifter is a single-pass visitor over `tsc`'s AST that projects in-subset expressions into formal IR formulas. Out-of-subset constructs are compile-time errors emitted by the lifter, not by the surface language.

This produces a compounding architecture:

1. **LLMs can write TypeScript.** Every model trained after 2020 has ingested billions of lines of JS/TS. There is no LLM that cannot produce a function body in TypeScript.

2. **The same LLMs can write invariants.** Invariants are TypeScript predicates. No new language. No new training corpus. No new conceptual model. `(x: Int) => x > 0` is the same shape as the function bodies the LLM has been emitting since GPT-2.

3. **The invariants don't need to be good — they need to exist.** A trivial invariant ("the result is a number") catches `undefined` and `NaN`. A weak invariant ("the result is non-negative") catches negative results. Even garbage invariants raise the floor over no invariants.

4. **One-size-fits-all template generates invariants from intent.** A single prompt template — "given this diff, these tests, and this intent text, write invariants in IR" — works for every LLM that can write TypeScript. The framework invokes the template at commit time. The user never authors invariants directly.

5. **Accumulated invariants constrain future output.** Once an invariant exists, the framework rejects any commit (human-authored or LLM-generated) that violates it. The invariant becomes the institutional memory the LLM doesn't have.

6. **Shadow AST walking compounds the value.** The prover walks the code's AST symbolically — branch by branch, path by path — and at every reachable state checks whether ALL existing invariants hold. A new code path added to an existing function inherits coverage from every pre-existing invariant on that function. **No new invariant required to catch failures in new code.**

7. **Software ages backwards.** Each commit may add zero new invariants. But each commit's code modifications get checked against the entire existing invariant set, automatically. Coverage grows as code grows, even if invariant authorship stops. Codebases get more provably correct over time, not less.

This is constraint-by-design, mechanically enforced at the git commit boundary. The LLM is dumb; the gate is smart; the invariants compound.

## 2. Operational Model: Two LLM Calls

The framework runs LLMs twice for each commit. Same dumb LLM both times.

**LLM #1 (user-prompted):** writes the code change AND the tests. This is the workflow developers already use today — "give me a tax calculator with bracket logic" produces a function and a test file. The LLM never writes invariants directly. It writes code and tests, like always.

**LLM #2 (framework-invoked):** at commit time, the framework reads (diff, new tests, old tests still in scope, intent text from commit message or `--intent` flag) and invokes LLM #2 with a one-size-fits-all template:

```
Given the following diff, the following tests, and this intent description,
write invariants in IR (TypeScript subset) that:
- Pass for all the new tests
- Are consistent with the diff's intended semantics
- Capture properties the function should satisfy for ALL inputs in the domain

Write to <path>.invariant.ts using `property(name, predicate)` declarations.
```

LLM #2's output is invariants. The framework then verifies the diff against those invariants via the prover. Pass → commit lands. Fail → rejected with counterexample.

**The user's only authoring task is the prompt and (optionally) the test cases.** Both are natural-language and executable-example artifacts they would have written anyway. The invariants are generated mechanically from those artifacts plus the diff.

This is why the IR-as-typed-TS-subset matters. LLM #2 is dumb; if the IR were a custom DSL, LLM #2 would frequently emit syntactically invalid IR. Because the IR is TypeScript, every output LLM #2 produces is at minimum syntactically valid TypeScript. The lifter then enforces the subset boundary. If LLM #2 emits `for (const x of xs) { check(x); }`, the lifter rejects it with "use `xs.every(x => check(x))` instead." LLM #2 (or the framework calling it) iterates against that feedback.

## 3. File Anchoring

**All invariant declarations MUST live in `.invariant.ts` files.** The lifter REJECTS `sugar.property` calls discovered in any other file. This is mechanical enforcement of constraint-by-design.

**Why mechanical, not convention:**

The whole framework value hinges on production code remaining bit-identical to whatever it was before Sugar arrived. A 40-year-old COBOL banking system, a Rust kernel module, a Java microservice — none of them can be modified to gain verification. Modifying the artifact under verification IS the invalidation. The audit trail breaks; the compliance posture voids; the whole thesis collapses.

A "convention" that invariants SHOULD live in dedicated files but MAY live inline is not enforcement. The first time an LLM (or a hurried developer) drops `sugar.property(...)` into `invoice.ts`, the codebase has crossed the line from constraint-by-design to contract-by-design. The lifter cannot allow this transition. It must reject it at compile time.

**The rule:**

```
For each .ts file F:
  If F's name matches *.invariant.ts:
    Lift `sugar.property` calls in F.
  Else:
    If F contains `sugar.property` calls:
      Emit diagnostic at the call site:
        "sugar.property may only appear in .invariant.ts files.
         Move this declaration to <co-located>.invariant.ts."
      Halt the lift.
```

**File-pairing convention:**

Invariants for `src/billing/invoice.ts` live in `src/billing/invoice.invariant.ts`. The convention is co-location at the directory level — same directory, sibling file, `.invariant.ts` suffix. The lifter doesn't enforce co-location; an `invariants/` subdirectory is fine. The only enforcement is "no `sugar.property` calls outside `*.invariant.ts` files."

**Production code never imports sugar.** The dependency direction is one-way: invariant files import production-code symbols. Production-code files do NOT import invariant-file symbols. Removing every `.invariant.ts` file leaves a project that's bit-identical to what it was before Sugar adoption. This is the architectural property the framework's value depends on.

**Worked example (good):**

```ts
// src/billing/invoice.ts — UNCHANGED
import type { LineItem } from './types';

export function calculateTotal(items: LineItem[]): number {
  return items.reduce((sum, item) => sum + item.price * item.qty, 0);
}
```

```ts
// src/billing/invoice.invariant.ts — pure metadata, references the function
import { calculateTotal } from './invoice';
import type { LineItem } from './types';
import { property, forAll } from 'sugar/ir';

property("totalIsNonNegative",
  forAll<LineItem[]>(items => calculateTotal(items) >= 0)
);
```

**Worked example (bad — lifter rejects):**

```ts
// src/billing/invoice.ts — DON'T DO THIS
import { property } from 'sugar/ir';

export function calculateTotal(items: LineItem[]): number {
  return items.reduce((sum, item) => sum + item.price * item.qty, 0);
}

property("totalIsNonNegative",
  forAll<LineItem[]>(items => calculateTotal(items) >= 0)
);
// LIFTER ERROR: sugar.property may only appear in .invariant.ts files.
//               Move this declaration to invoice.invariant.ts.
```

## 4. Quantifier Syntax

**Both `.every`/`.some` AND `forAll`/`exists` are valid.** The lifter normalizes both to the same canonical IR.

**Native array-method form:**

```ts
import type { Int } from 'sugar/sorts';
import { property } from 'sugar/ir';

property("allPositive",
  (xs: Int[]) => xs.every(x => x > 0)
);

property("hasZero",
  (xs: Int[]) => xs.some(x => x === 0)
);
```

The lifter recognizes `.every` on a sort-typed array as universal quantification, and `.some` as existential. The receiver's element type provides the sort.

**Builder form:**

```ts
import { property, forAll, exists, Int } from 'sugar/ir';

property("allPositive",
  forAll<Int>(x => x > 0)
);

property("hasZero",
  exists<Int>(x => x === 0)
);
```

**Both lift to identical IR.** Use whichever reads more naturally for the predicate at hand. LLMs trained on JavaScript will reach for `.every`. Mathematicians and SMT-tool-savvy developers will reach for `forAll`. Mixing forms in the same file is allowed.

**Nested quantifiers:**

```ts
property("triangleInequality",
  forAll<Real>(a =>
    forAll<Real>(b =>
      forAll<Real>(c =>
        Math.abs(a - b) <= c // assuming Math.abs is in the registry
      )
    )
  )
);
```

Quantifier nesting is unrestricted at v1. The canonicalizer flattens consecutive same-direction quantifiers (∀x.∀y.P → ∀x,y.P) during AST normalization; see `2026-04-29-ast-canonicalizer.md` §4.

## 5. Sort Marking

**Sorts are inferred from TypeScript type annotations.** The lifter reads the lambda parameter's type annotation via `tsc`'s type checker, resolves it through type aliases and brand types, and produces the corresponding IR sort.

**Primitive sorts** (kit-provided as branded types):

```ts
// In sugar/sorts (kit-supplied for the TS kit)
export type Int = number & { readonly __sort: 'Int' };
export type Real = number & { readonly __sort: 'Real' };
export type Bool = boolean & { readonly __sort: 'Bool' };
export type StringSort = string & { readonly __sort: 'String' };
```

A lambda param annotated `Int`, `Real`, etc. tells the lifter the corresponding IR sort.

**User-defined sorts** (kit-extension):

```ts
// In src/billing/types.ts
export type Cents = number & { readonly __sort: 'Cents' };
```

```ts
// In src/billing/invoice.invariant.ts
import type { Cents } from './types';
import { property, forAll } from 'sugar/ir';

property("nonNegativeCents",
  forAll<Cents>(c => c >= 0)
);
```

The lifter discovers `Cents`'s `__sort` brand via `tsc`'s symbol resolution and registers `Cents` as a sort name in the kit's sort namespace. Sort names must be unique within a project; collisions are compile-time errors.

**Why brand types as sort markers:**

The alternative is a runtime sort registry (`const Cents = sort("Cents")`). That requires the production code to import sugar, violating the constraint-by-design rule. Brand types are zero-runtime — they exist only in the type system — and `tsc` resolves them at lift time. The lifter never needs the production code to know about sugar.

**Lambda body type-checks against the sort:**

```ts
property("xPlusOne",
  forAll<Int>(x => x + 1 > x) // tsc resolves x: Int, the body type-checks as Int
);
```

The TypeScript type system enforces that operations on `x` are valid for `Int`. Subtraction with a `Real` would be a tsc error before the lifter runs.

## 6. The TS Subset That IS the IR

**This is the language boundary the lifter enforces.** Each excluded construct breaks at least one of three properties the lifter must preserve:

- **Soundness:** the lifted formula means the same thing the source code means
- **Completeness:** the lifter can always statically determine what the source means
- **Determinism:** the same source code lifts to byte-identical IR every time

If a construct breaks any of these, it's out for v1.

### 6.1 IN list (legal in `.invariant.ts` files)

**Operators:**
- Equality: `===`, `!==`
- Comparison: `<`, `<=`, `>`, `>=`
- Boolean: `&&`, `||`, `!`
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Optional chaining: `?.` (lifts as nullable-guarded access)
- Nullish coalescing: `??` (lifts as ternary over null/undefined check)

**Literals:**
- Number literals (positive, negative, decimal, hex, octal, binary, BigInt)
- Boolean literals: `true`, `false`
- String literals (only equality-comparable; no string operations beyond `===`)
- `null`, `undefined`

**Control flow:**
- Ternary: `cond ? a : b`
- Short-circuit: `a && b`, `a || b`
- Implication forms: `!a || b`, `implies(a, b)` (see §8)

**Quantifiers:**
- `xs.every(x => P(x))`, `xs.some(x => P(x))` on sort-typed receivers
- `forAll<T>(x => P(x))`, `exists<T>(x => P(x))` from `sugar/ir`

**References:**
- Lambda parameters
- Member access on lambda parameters: `x.field`, `x.field.nested`
- Optional member access: `x?.field`
- Imports of production-code symbols (functions, types, constants)
- Imports of sugar IR builders (`property`, `forAll`, `exists`, `implies`, `iff`, etc.)

**Function calls:**
- Calls into the kit's pure-function registry (e.g., `Math.abs(x)`, `parseInt(s)`)
- Calls into production-code functions THAT ARE THEMSELVES PURE PER REGISTRY ANNOTATION (or per kit-supplied analysis; see §11)

**Closures:**
- Closure over `const` literal: `const LIMIT = 100; property("...", x => x < LIMIT);`
- Closure over `const` from a pure import: `import { MAX } from './limits'; property("...", x => x < MAX);`

The lifter resolves the const value at lift time and inlines it into the IR. The const must be statically resolvable — no `Math.random()` in its initializer, no calls to functions that aren't in the pure registry.

### 6.2 OUT list (compile-time error from the lifter)

**Async / temporal:**
- `await`, `async`
- Generators (`function*`, `yield`)
- Promise constructors

**Defense:** FOL has no notion of time. An `async` function returns a promise (a value-that-doesn't-exist-yet); a predicate that says "the result is non-negative" can't be lifted because the result isn't a value, it's a future value with a failure mode (rejection) the predicate can't express. Time and effects don't compose with timeless propositions.

**Loops:**
- `for`, `for...of`, `for...in`, `while`, `do...while`

**Defense:** Quantifiers replace iteration in FOL. The clean translation of "for all x in xs, P(x)" is `xs.every(x => P(x))`, which lifts trivially. Allowing `for` opens a Pandora's box of patterns (loop vars, accumulators, `break`, `continue`, nested with shared state) that don't have FOL analogues. Drawing the line at "no loops, use `.every`/`.some`" keeps the subset small AND makes predicates more readable. Zero expressive loss.

**Mutations:**
- Assignment (`=`) outside `const` declarations
- Compound assignment (`+=`, `-=`, etc.)
- Increment/decrement (`++`, `--`)
- Mutating array methods (`.push`, `.pop`, `.splice`, `.sort` on receiver, etc.)
- Mutating object methods (`Object.assign`, `delete`, etc.)

**Defense:** A predicate must be a pure function of its inputs. Mutations introduce hidden state — the predicate's output depends on what happened before, which FOL can't express without temporal logic or arrays-of-states. Use `.reduce` for folds (lifts cleanly); use quantifiers for assertions over collections.

**Exceptions:**
- `try`, `catch`, `finally`
- `throw`

**Defense:** A predicate's truth value is binary (true/false) or trinary (true/false/undecidable). Exceptions introduce a fourth state — "tried to evaluate but failed" — that conflates "P is false" with "P couldn't be evaluated." The right pattern: encode the failure condition AS a guard predicate (`isValid(x) && P(x)`), don't catch.

**Object identity / dynamic dispatch:**
- `this`
- `new` (constructor calls)
- Prototype access (`Object.getPrototypeOf`, `__proto__`, `instanceof` against non-branded types)
- Class declarations, methods (use bare functions instead)

**Defense:** All three introduce dynamic behavior the lifter can't statically resolve. `this` is implicit context. `new` runs constructor logic the lifter would have to interpret. Prototype access dispatches dynamically based on runtime object identity. The lifter would need to embed a JavaScript interpreter to handle them. Clean alternative: pure functions over plain data — don't take a method, take a function; don't construct, take constructed values as inputs.

**Side-effecting calls:**
- Any call to a function NOT in the kit's pure-function registry
- I/O (`fs.*`, `process.*`, `console.*`)
- Networking (`fetch`, `XMLHttpRequest`)
- Time (`Date.now`, `performance.now`)
- Randomness (`Math.random`, `crypto.*` non-deterministic)

**Defense:** Predicate purity is the foundational property. Calls to unregistered functions might do anything — read files, mutate global state, return non-deterministic values. The lifter can't statically infer purity; it has to be DECLARED via the pure-function registry. If a function isn't in the registry, the lifter conservatively assumes it's impure and rejects.

**Closure over reassignable bindings:**
- Closure over `let` or `var` that can be reassigned

**Defense:** Same predicate yields different results depending on call timing — breaks determinism. Same principle as mutations: predicates must be pure functions of explicit inputs. Closure over `const` is allowed because const values are statically resolvable; closure over `let`/`var` isn't.

**Recursion:**
- A predicate body that references itself (directly or indirectly)

**Defense (v1):** SMT solvers vary in support for inductive definitions. Z3 has limited recursion support; some kit-supplied solvers may have none. v1 forbids recursion in predicate bodies; v2+ may allow it as a per-kit extension if the kit's solver supports inductive definitions. For v1, recursion in production code is fine — the predicate just can't recurse in its own body.

### 6.3 Edge cases

**`?.` and `??`:**

Both lift trivially as sugar:
- `obj?.field` → `obj === null || obj === undefined ? undefined : obj.field`
- `value ?? fallback` → `value === null || value === undefined ? fallback : value`

The lifter desugars at lift time. The canonicalizer normalizes the resulting ternary expressions.

**Tuple destructuring in lambda params:**

Allowed for v1 only when the tuple type has fixed arity and named-positions:
```ts
property("...", forAll<[Int, Int]>(([a, b]) => a + b > 0));
```

Object destructuring NOT allowed in v1 — too many edge cases with optional fields, default values, rest patterns.

**Calls to production-code functions:**

Allowed only if the called function is itself in the pure registry (or has been kit-analyzed as pure). The lifter does NOT recurse into the called function's body — it consults the registry's symbolic-range contract for that function. This is what makes verification compositional: a function's invariants compose with its callers' invariants via the registry's pure contracts.

**Generic functions:**

```ts
property("...",
  forAll<Int>(x => identity<Int>(x) === x)
);
```

Allowed if the called function is registry-listed and the kit supplies a symbolic-range contract parameterized over the type variable.

## 7. Property Anchoring API

**The exported API from `sugar/ir`:**

```ts
// Core property declaration
export function property(
  name: string,
  formula: () => boolean
): void;

// Anonymous assertion (uses caller location for name)
export function assert(formula: () => boolean): void;

// Quantifiers
export function forAll<T>(predicate: (x: T) => boolean): boolean;
export function exists<T>(predicate: (x: T) => boolean): boolean;

// Logical connectives
export function implies(antecedent: boolean, consequent: boolean): boolean;
export function iff(left: boolean, right: boolean): boolean;
```

**All of these are no-ops at runtime.** Their bodies throw a sentinel error "sugar IR builder called at runtime — should be lifted, not executed" if invoked outside the lifter. The bundler tree-shakes them out of production builds; they exist only as type-level + AST-level metadata.

**`property` vs `assert`:**

- `property("name", ...)` — named, queryable via `sugar list-properties`, addressable for memento storage. Use this for invariants you want explicit.
- `assert(...)` — anonymous, generated name from `<file>:<line>:<col>`. Use sparingly; the named form is generally preferred because the name is what shows up in counterexamples and audit trails.

**Composition:**

Properties can compose by referencing each other through their named identifiers. The lifter resolves cross-property references at lift time:

```ts
// in a.invariant.ts
property("aIsNonNeg", forAll<Int>(x => x >= 0));

// in b.invariant.ts
import { ref } from 'sugar/ir';
property("bDependsOnA", ref("aIsNonNeg") && forAll<Int>(x => x + 1 > 0));
```

`ref("name")` is a kit-supplied builder that injects a reference to another property's IR formula. The canonicalizer resolves these into a single combined formula at canonicalization time.

## 8. Implication

**Both forms are accepted; the lifter normalizes:**

- Native form: `!a || b`
- Builder form: `implies(a, b)`

Both produce the canonical IR `Implies(a, b)`. The canonicalizer's NNF pass may further normalize `Implies` to `Or(Not(a), b)` for hash purposes; see `2026-04-29-ast-canonicalizer.md` §5.

**Why both:**

LLMs trained on idiomatic JavaScript will write `!hasError || isValid` naturally. Developers writing in formal-math style will reach for `implies(a, b)`. Forcing one form would either confuse LLMs (if `implies` is required) or look ugly (if `!a || b` is required for a property called `aImpliesB`).

**Worked example:**

```ts
property("validatedInputProducesNonNegOutput",
  forAll<Input>(input =>
    implies(
      isValid(input),
      compute(input) >= 0
    )
  )
);

// Equivalent — same IR after lift+canonicalize
property("validatedInputProducesNonNegOutput",
  forAll<Input>(input =>
    !isValid(input) || compute(input) >= 0
  )
);
```

## 9. The Lifter

**The lifter is a single visitor pass over `tsc`'s `Program` AST.** It walks each `.invariant.ts` file, descends into `property` and `assert` call expressions, and lifts the formula argument's AST into IR.

**Top-level architecture:**

```ts
// Pseudocode for the lifter's entry point
function liftProject(project: TsProject): IrFormulaSet {
  const formulas: IrFormula[] = [];
  for (const file of project.files()) {
    if (!file.path.endsWith('.invariant.ts')) {
      // Reject cross-anchoring violations (see §3)
      assertNoSugarProperty(file);
      continue;
    }
    for (const propertyCall of findPropertyCalls(file)) {
      const formulaAst = propertyCall.arguments[1]; // the formula expression
      const ir = liftExpression(formulaAst, file.typeChecker);
      formulas.push({
        name: extractName(propertyCall),
        formula: ir,
        sourceLocation: file.path + ':' + propertyCall.line,
      });
    }
  }
  return { formulas };
}
```

**Per AST-node lift rules** (visitor pattern):

| TS AST node | Lift rule |
|---|---|
| `BinaryExpression` `&&` | `And(lift(left), lift(right))` |
| `BinaryExpression` `\|\|` | `Or(lift(left), lift(right))` |
| `BinaryExpression` `===` | `Eq(lift(left), lift(right))` |
| `BinaryExpression` `!==` | `Not(Eq(lift(left), lift(right)))` |
| `BinaryExpression` `<`, `<=`, `>`, `>=` | `Lt`, `Lte`, `Gt`, `Gte` per operator |
| `BinaryExpression` `+`, `-`, `*`, `/`, `%` | `Add`, `Sub`, `Mul`, `Div`, `Mod` per operator |
| `PrefixUnaryExpression` `!` | `Not(lift(operand))` |
| `PrefixUnaryExpression` `-` | `Negate(lift(operand))` |
| `ConditionalExpression` (ternary) | `If(lift(cond), lift(then), lift(else))` |
| `CallExpression` `xs.every(...)` | `ForAll(sort(xs), lift(arrowFn))` |
| `CallExpression` `xs.some(...)` | `Exists(sort(xs), lift(arrowFn))` |
| `CallExpression` `forAll<T>(...)` | `ForAll(sort(T), lift(arrowFn))` |
| `CallExpression` `exists<T>(...)` | `Exists(sort(T), lift(arrowFn))` |
| `CallExpression` `implies(a, b)` | `Implies(lift(a), lift(b))` |
| `CallExpression` `iff(a, b)` | `Iff(lift(a), lift(b))` |
| `CallExpression` registry fn | `Apply(fnName, args.map(lift))` |
| `Identifier` lambda param | `Var(name)` |
| `Identifier` const-bound | `Const(value)` (resolved at lift time) |
| `PropertyAccessExpression` `x.field` | `Project(lift(x), 'field')` |
| `OptionalChainingExpression` `x?.field` | desugar to `If(IsNull(lift(x)), Undefined, Project(lift(x), 'field'))` |
| `BinaryExpression` `??` | desugar to `If(IsNull(lift(left)), lift(right), lift(left))` |
| `NumericLiteral` | `Num(value)` |
| `StringLiteral` | `Str(value)` |
| `TrueKeyword` / `FalseKeyword` | `Bool(true)` / `Bool(false)` |
| `NullKeyword` / `UndefinedKeyword` | `Null` / `Undefined` |
| All other AST nodes | **Reject** with diagnostic naming the construct |

**Error reporting:**

The lifter emits diagnostics using `tsc`'s standard `Diagnostic` interface so they integrate with editors, build tools, and CI. Every rejection includes:
- The AST node's source location (file:line:col)
- The specific rule violated (`"for...of loops are not allowed in IR; use .every() instead"`)
- A suggested rewrite, if one exists

**Type-checker integration:**

The lifter consults `tsc`'s `TypeChecker` for:
- Sort resolution on lambda params (§5)
- Symbol resolution on identifiers (distinguishing lambda param from imported const)
- Function-call resolution (looking up the called function in the pure registry)
- Const-value resolution (resolving compile-time constants for inlining)

This is why the lifter's input is a `tsc.Program`, not a raw AST. The lifter takes advantage of all of `tsc`'s type-checking work; it doesn't reimplement it.

## 10. The Lowerer

**The lowerer takes an `IrFormula` and produces TypeScript source.** Its purpose: regenerate readable invariant declarations from stored mementos, for human review and for cross-version IR migration.

**Round-trip property:**

```
forall formula: IrFormula. lift(lower(formula)) ≡ formula (modulo canonicalization)
```

The lowerer must produce TS source that lifts back to the same canonical IR. The canonicalizer normalizes both forms during equivalence checking, so syntactic differences (e.g., `!a || b` vs `implies(a, b)`) are tolerated as long as they canonicalize identically.

**Output style:**

The lowerer emits invariants in the **builder form** (`forAll`, `exists`, `implies`) rather than the native form. Reasons:
- Builder form is more verbose but more uniform — the lowerer doesn't need to choose between idioms
- Builder form maps 1:1 with IR node names — easier to debug regenerated source
- Native-form sugar can be produced by a separate pretty-printer pass if desired

**Use cases:**

- `sugar show <property-name>` displays the IR formula as TS source
- Cross-version migration: spec v1 → v2 may add new IR node types; the lowerer regenerates v1 mementos as v2 source so they can be re-lifted under the new spec
- Debugging: counterexample reports show the lowered IR alongside the violated path

The lowerer is **not** for production authoring. Humans author in `.invariant.ts` files using whatever idiom they prefer; the lifter normalizes.

## 11. Built-in Contracts: Same Format as User Invariants

**There is no separate "registry API."** The lifter has ONE input format: the `.invariant.ts` file. Built-in contracts for the language's standard library are shipped AS `.invariant.ts` files. Third-party library contracts are stored AS `.invariant.ts` files. User invariants are AS `.invariant.ts` files. Auto-generated invariants from commit-time intent are AS `.invariant.ts` files.

**Four contract layers, one primitive:**

| Layer | Location | Author |
|---|---|---|
| Kit built-ins | `node_modules/sugar-ts/builtins/*.invariant.ts` | Framework, ships with the kit |
| Third-party libraries | `<project>/.sugar/contracts/*.invariant.ts` | LLM-generated on-demand or hand-written |
| User invariants | `<project>/src/**/*.invariant.ts` | User-authored or generated from intent |
| Auto-generated from intent | `<project>/src/**/*.invariant.ts` (same as user) | LLM #2 at commit time |

**The lifter loads all four layers identically.** It walks the project's tsconfig-included files plus the kit's built-ins directory plus any registered third-party contract directories. Every `.invariant.ts` file produces the same kind of IR. The lifter doesn't distinguish "this is a kit built-in" from "this is user-authored" — the IR is uniform.

**Why this is the only architecturally sound model:**

We can't modify Node.js source. We can't modify standard-library implementations. We can't modify third-party packages. The constraint-by-design rule applies to ALL of them: the artifact under verification stays bit-identical, and constraints attach externally as separate files.

The kit's built-in catalog is just constraint-by-design applied to the host language's standard library. Same mechanism as user code; different scope. There is no special-case API for "describing built-ins" because there doesn't NEED to be one — the invariant file format already covers it.

**Worked example: `parseInt` built-in contract**

The TS kit ships this file:

```ts
// node_modules/sugar-ts/builtins/parseInt.invariant.ts
import { property, forAll, exists, implies } from 'sugar/ir';
import type { Int, StringSort } from 'sugar/sorts';

// parseInt's possible return range: any Int including 0, plus NaN sentinel
property("parseIntReturnsIntOrNaN",
  forAll<StringSort>(s =>
    isInt(parseInt(s)) || isNaN(parseInt(s))
  )
);

// Specific behavior for "0" input
property("parseIntZeroStringReturnsZero",
  parseInt("0") === 0
);

// Negative inputs
property("parseIntNegativeStringReturnsNegative",
  forAll<Int>(n =>
    implies(n < 0, parseInt(String(n)) === n)
  )
);

// Empty string
property("parseIntEmptyStringIsNaN",
  isNaN(parseInt(""))
);

// The crucial existence claim that drives the divide-by-zero counterexample:
property("parseIntCanReturnZero",
  exists<StringSort>(s => parseInt(s) === 0)
);
```

The prover reads this catalog at startup. When the user's code calls `parseInt(userInput)`, the prover knows from `parseIntCanReturnZero` that the return value can be 0. That's how the divide-by-zero counterexample (§14) is found.

**Worked example: third-party library contract (LLM-generated)**

The user imports `lodash`. Their code calls `_.shuffle(arr)`. The kit doesn't know lodash. At lift time, the lifter encounters `_.shuffle` and finds no contract. Two possible policies:

- **Strict:** reject the lift. User must provide a contract.
- **Generative:** invoke LLM #2 with a "write a contract for this function" template, store the result.

For v1, **strict** is the default. The LLM iteration loop produces a contract:

```ts
// .sugar/contracts/lodash-shuffle.invariant.ts
import { shuffle } from 'lodash';
import { property, forAll } from 'sugar/ir';

property("shufflePreservesLength",
  forAll<unknown[]>(arr => shuffle(arr).length === arr.length)
);

property("shufflePreservesElements",
  forAll<unknown[]>(arr =>
    arr.every(x => shuffle(arr).some(y => y === x))
  )
);
```

This file is committed to the project. Future commits get verification of `_.shuffle` callsites against these contracts. The LLM only writes the contract once; it persists.

**The dual-consumer story:**

The same `.invariant.ts` file is read by:

1. **Lifter:** "Is this call legal in the IR subset?" — yes if a contract exists, no otherwise. This is automatic from the load-the-invariant-files mechanism.
2. **Prover:** "What's the symbolic value range of this call's return?" — derived from the invariants attached to the function. Universal claims constrain; existential claims expose possible values.

There is no separate "registry data structure." The invariant set IS the registry. The lifter and prover both read the same files for different purposes.

**Per-language kit obligation:**

Each kit MUST ship a built-in catalog of `.invariant.<lang>` files covering the host language's standard library. The TS kit ships invariants for:

- All `Math.*` pure functions
- All `Number.*` static methods (`parseInt`, `parseFloat`, `isFinite`, `isInteger`, `isNaN`)
- All `String.prototype` pure methods (`charAt`, `charCodeAt`, `slice`, `substring`, `length`, `indexOf`, `includes`, `startsWith`, `endsWith`, `toLowerCase`, `toUpperCase`)
- All `Array.prototype` pure methods (`every`, `some`, `find`, `findIndex`, `includes`, `indexOf`, `length`, `at`, `slice`, `concat`, `flat`, `flatMap`, `map`, `filter`, `reduce`)
- Bare globals: `parseInt`, `parseFloat`, `isNaN`, `isFinite`

The Rust kit ships invariants for `Vec`, `Option`, `Result`, the prelude. The COBOL kit ships invariants for COBOL's intrinsic functions. Each kit's built-in catalog is its primary value-add — the part the kit author writes once and every project using the kit gets forever.

**This unifies the implementation-fungibility story:** ProvegIt ships its own Go built-in catalog as `.invariant.go` files. ProverIt ships Rust's as `.invariant.rs` files. The implementations are fungible; the contract format (`.invariant.<lang>` files) is identical across reimplementations because that's what the spec mandates.

**The dual-consumer story, walked through:**

The shitty LLM emits this invariant:
```ts
property("nonZeroDenominator",
  forAll<Int>(d => implies(d !== 0, divide(numerator, d) is defined))
);
```

Code path #1 (callsite from earlier code): all callers pass non-zero values that pass through a runtime check. Prover walks code path #1 symbolically. At the call to `divide`, the symbolic value of `d` is "non-zero per upstream check." The invariant `d !== 0` holds. Pass.

Code path #2 (new callsite added later):
```ts
const userInput = process.argv[2];
const divisor = parseInt(userInput);
const result = divide(amount, divisor);
```

Prover walks code path #2 symbolically. At the call to `parseInt`, the prover consults the registry. `parseInt`'s `returnRange.canBe` includes `'zero'`. The symbolic value of `divisor` includes 0.

Prover then evaluates the existing invariant `d !== 0` against the new callsite's symbolic input range. The range includes 0. Counterexample: `process.argv[2] === "0"` → `parseInt("0") === 0` → invariant violated.

Commit rejected. Counterexample reported.

**The bug is caught by:**
- An invariant written by a previous LLM that didn't know command-line parsing existed
- Against a code path that didn't exist when the invariant was written
- With ZERO new invariant authoring required

**This is what makes the framework so cheap to extend:**

Adding `parseInt` knowledge to the kit is a registry entry. Every commit forever benefits. Every new callsite of `parseInt` gets symbolic-range-aware verification. Per-kit registry maintenance is the leverage point — kit authors write `parseInt`'s contract once, and every codebase using the TS kit gets the benefit.

**Registry structure:**

The TS kit ships with a baseline registry covering:
- All `Math.*` pure functions (`abs`, `max`, `min`, `floor`, `ceil`, `round`, `sign`, `pow`, `sqrt`, `log`, `exp`, `sin`, `cos`, `tan`)
- All `Number.*` static methods (`isFinite`, `isInteger`, `isNaN`, `parseInt`, `parseFloat`)
- All `String.prototype` pure methods (`charAt`, `charCodeAt`, `slice`, `substring`, `length`, `indexOf`, `includes`, `startsWith`, `endsWith`, `toLowerCase`, `toUpperCase`)
- All `Array.prototype` pure methods (`every`, `some`, `find`, `findIndex`, `includes`, `indexOf`, `length`, `at`, `slice`, `concat`, `flat`, `flatMap`, `map`, `filter`, `reduce`)
- Bare globals: `parseInt`, `parseFloat`, `isNaN`, `isFinite`

Kit-extension: project-specific registries can be loaded via `.sugar/registry.ts`. Same shape; merged at lift time.

## 12. Curry-Howard Alignment

**The TypeScript type system IS the type system of the IR.** This is mechanism, not analogy.

- A TS type `(x: Int) => boolean` IS a unary predicate over the sort `Int`.
- A TS expression `x > 0` (with `x: Int`) IS a proof attempt that the predicate holds.
- The TypeScript compiler IS the type-checker for IR formulas.
- The lifter IS the projection from "type-checked TS expression" to "formal IR formula."

**Implication:** The lifter only runs on expressions that `tsc` has already type-checked. If the expression has a type error, `tsc` rejects it; the lifter never sees it. By the time the lifter runs, every expression is already known to be well-typed in the host language.

**Practical consequences:**

```ts
// tsc rejects this — string + number is not allowed in strict mode
property("...",
  forAll<Int>(x => x + "hello") // tsc error: type mismatch
);
```

```ts
// tsc accepts this; lifter sees a well-typed predicate body
property("...",
  forAll<Int>(x => x > 0)
);
```

**`property` and `assert` are typed to enforce predicate shape:**

```ts
export function property(
  name: string,
  formula: () => boolean
): void;
```

The second argument MUST be a function returning `boolean`. `tsc` rejects properties whose body returns anything else. This forces every property body to be a well-formed predicate at the TS-type level before the lifter runs.

**Why this matters:**

The framework gets the entire TypeScript type system for free as the FIRST GATE of validity. A property that fails type-checking can't even reach the lifter. This eliminates a whole class of nonsense invariants the LLM might emit — "property body returns void" or "property body returns Promise" — at zero implementation cost.

The lifter is the SECOND gate: it enforces the IR subset. Together, type-checking + lifting produces an invariant that's both well-typed AND in-subset.

## 13. Verification Cadence: git commit

**The git commit IS the verification boundary.** Pre-commit hook runs:

1. `sugar generate --staged --intent <commit-message>`
2. `sugar prove`

Both must pass for the commit to land.

**Why git commit specifically:**

- **Smallest unit of state with a hash and audit trail.** Git commits have SHA-1/SHA-256 identity; they're addressable; they're the natural unit of history.
- **Mechanically unbypassable when installed as a hook.** Pre-commit hooks run before the commit object is created. Skipping requires `--no-verify`, which is a deliberate user action, not an accident.
- **The natural termination point for an LLM's iteration loop.** When an LLM is iterating on a fix-then-test-then-commit cycle, the commit is the moment it stops iterating. The gate fires AT that moment.
- **The first gate the work has to pass through.** Build time is too late (the LLM has moved on). CI is too late (the broken state already exists in someone's branch). Git commit is the FIRST gate.
- **Universal.** Every developer (human or LLM) commits. There is no developer workflow that skips commit.

**`sugar init` installs the pre-commit hook:**

```sh
$ sugar init
Installing pre-commit hook at .git/hooks/pre-commit...
Done. Future commits will run `sugar generate` and `sugar prove`.
```

The hook script is one line:
```sh
#!/bin/sh
sugar generate --staged --intent "$(cat .git/COMMIT_EDITMSG 2>/dev/null || echo)" \
  && sugar prove
```

**`sugar generate`:**

Synthesizes new invariants from a unit of work. Two flavors:

- `--staged` (default for commit hook): reads the staged diff, the new tests in the diff, the old tests still in the codebase, and the intent text. Invokes LLM #2 with the one-size-fits-all template. Writes invariants to `.invariant.ts` files in the same directories as the modified production-code files. Stores mementos for the generated invariants.

- `--legacy <path>` (for retrospective adoption): reads the existing code at `<path>`, no diff. Optionally reads a human-supplied description. Invokes LLM #2 to generate invariants ABOUT the existing code. Writes to `<path-dir>.invariant.ts`. Stores mementos. The existing code is NEVER modified.

**`sugar prove`:**

Verifies the codebase against all existing invariants. Read-only operation:

1. Walks all `.invariant.ts` files. Lifts each into an IR formula.
2. Walks the production code's AST symbolically (shadow AST walking; see §14).
3. For each function, evaluates ALL applicable invariants against ALL reachable symbolic states.
4. If any invariant produces a counterexample, prove fails with the counterexample reported.
5. If all invariants hold, prove succeeds. Optionally writes a signed verification memento.

**The commit gate's failure modes:**

| Failure | Cause | LLM iteration response |
|---|---|---|
| Generate fails | LLM #2 produces invariants that don't lift (out-of-subset constructs) | Lifter feedback drives LLM #2 retry |
| Prove fails | Existing invariant violated by new code path | LLM #1 or human modifies code to satisfy invariant |
| Prove fails | New invariant impossible to satisfy | LLM #2 weakens or removes invariant |
| Verification timeout | Solver doesn't converge | Prover surfaces "unverified — solver timeout" — kit-policy decides if that's a failure or a soft warning |

**Bypass:**

`git commit --no-verify` bypasses the hook. This is a deliberate, audited action. The commit lands without verification; the next `sugar prove` run (e.g., in CI) will catch any violations. The audit trail records that the commit was unverified.

## 14. Shadow AST Walking

**Shadow AST walking is the prover's mechanism for catching failures from existing invariants on new code paths, without requiring new invariants.**

**The mechanism:**

1. Read all `.invariant.ts` files; lift each property into an IR formula.
2. For each production-code function, build a CFG (control flow graph).
3. For each path in the CFG (each combination of branch decisions), build a symbolic state — a path condition expressing the constraints under which that path is taken plus the symbolic values of all live variables.
4. For each invariant applicable to the function (where "applicable" means "the invariant references the function's name or an upstream/downstream symbol"), evaluate the invariant against each symbolic state.
5. If any state satisfies the path condition AND violates the invariant, that's a counterexample.
6. The prover reports: (state, witness inputs, violated invariant, file:line of the invariant, file:line of the violating code).

**Why "shadow":**

The prover doesn't run the code. It walks a SHADOW interpretation of the code's AST — a symbolic interpretation in which values are not concrete bytes but symbolic expressions. The shadow walk explores all paths simultaneously; the SMT solver determines satisfiability of the resulting constraints.

**The compounding mechanism:**

Each invariant attaches to a function's input/output behavior, not to a specific code path through the function. When the function gains a new branch, the new branch's symbolic state is automatically subjected to all of the function's existing invariants. No new invariant required.

**Worked example (recurring divide-by-zero):**

```ts
// src/math.ts (UNCHANGED across all commits)
export function divide(n: number, d: number): number {
  return n / d;
}
```

```ts
// src/math.invariant.ts (written in commit #42, never modified after)
import { divide } from './math';
import { property, forAll, implies } from 'sugar/ir';

property("divideRequiresNonZeroDenominator",
  forAll<Int>(n =>
    forAll<Int>(d =>
      implies(d !== 0, isFinite(divide(n, d)))
    )
  )
);
```

**Commit #43 — adds a callsite from upstream-validated input:**

```ts
// src/billing/calculate.ts
function calculateRate(amount: Int, count: Int): Real {
  if (count === 0) return 0;
  return divide(amount, count);
}
```

Prover walks `calculateRate`. Two paths:
- Path A: `count === 0` → returns 0, no `divide` call. Invariant inapplicable. Pass.
- Path B: `count !== 0` (path condition: `count !== 0`) → calls `divide(amount, count)`. Symbolic value of `d` is `count` with path condition `count !== 0`. Invariant `d !== 0` evaluated against this state: `count !== 0` is given. Implication satisfied. Pass.

Commit #43 lands.

**Commit #44 — adds a NEW callsite from command-line input:**

```ts
// src/cli/configure.ts
import { divide } from '../math';

function configureRate(args: string[]): number {
  const amount = parseInt(args[0]);
  const count = parseInt(args[1]);
  return divide(amount, count);
}
```

Prover walks `configureRate`. One path:
- Path: calls `divide(amount, count)`. Symbolic value of `count` from `parseInt(args[1])`. Registry: `parseInt`'s `returnRange.canBe` includes `'zero'`. Symbolic value of `count` is `Int including 0`.
- Invariant `divideRequiresNonZeroDenominator` evaluated. `count` could be 0. Counterexample: `args = ['100', '0']`.

Commit #44 rejected. Diagnostic:

```
✗ Property `divideRequiresNonZeroDenominator` violated at src/cli/configure.ts:5:10
  Counterexample: args[1] = "0" → parseInt("0") = 0 → divide(amount, 0) violates d !== 0
  Invariant declared at src/math.invariant.ts:7

Suggested fixes:
  1. Add a guard at the callsite: `if (count === 0) throw new Error(...)`.
  2. Strengthen the precondition: include input validation in `configureRate`.
  3. Modify the invariant if the function is meant to be total over zero (returns sentinel).
```

**LLM #1 (or human) iterates.** Commit #44 fails. They add the guard. Re-commit. Pass.

**Compounding value:**

Commit #42 wrote one invariant. Commits #43 through #infinity get verification of every callsite of `divide`, automatically. The invariant set didn't grow; the code did. Coverage tracked code, not authoring.

**Software ages backwards:**

- Commit #42: codebase has 1 invariant. Verification covers what `divide` was called with at commit #42.
- Commit #43: codebase still has 1 invariant. Verification covers all callsites of `divide` as of commit #43, including the new one.
- Commit #44: codebase still has 1 invariant. Verification covers all callsites as of commit #44, including the rejected violator.
- Commit #100: codebase still has 1 invariant. Verification has caught 9 attempted violations, all rejected at commit time.

The codebase has 1 invariant for `divide`'s entire history. The codebase's actual correctness — measured as "no commit landed that called `divide` with a possibly-zero denominator" — is mechanically perfect, forever.

## 15. The Three-Step Unit of Work (Prospective)

**For a prospective change (bug fix, new feature), the unit of work is three steps:**

1. **Make the change.** Modify the production code. The LLM (or human) does this.
2. **Write the test(s).** Pin the new behavior or prevent regression. The LLM (or human) does this.
3. **Generate the invariants.** The framework does this at commit time, via `sugar generate`.

**Tests are the highest-value intent source:**

A test says "for input X, output is Y." That's an existential point — concrete, executable, version-controlled. An invariant says "for ALL inputs in the domain, output satisfies P." That's the universal curve through the test points.

**The framework's job is to turn the test cases into the generalized invariant.** The LLM #2 invocation reads:
- The diff (what changed)
- The new tests in the diff (concrete examples of intent)
- The old tests still in the codebase (constraints that must still hold)
- The intent text (commit message, prompt log, ticket body — additional context)

And synthesizes invariants that pass all the tests, are consistent with the diff, and capture the intent's universal generalization.

**Worked example:**

User prompt to LLM #1: `"fix the off-by-one in date validator for leap years"`

LLM #1 produces:

```ts
// src/dates/validator.ts (modified)
export function isLeapYear(year: number): boolean {
  if (year % 400 === 0) return true;
  if (year % 100 === 0) return false;
  return year % 4 === 0;
}
```

```ts
// src/dates/validator.test.ts (added)
import { isLeapYear } from './validator';

test('Feb 29, 2024 is a valid leap year', () => {
  expect(isLeapYear(2024)).toBe(true);
});

test('Feb 29, 2100 is NOT a leap year', () => {
  expect(isLeapYear(2100)).toBe(false);
});

test('Feb 29, 2000 is a leap year (divisible by 400)', () => {
  expect(isLeapYear(2000)).toBe(true);
});
```

User runs `git commit -m "fix off-by-one in leap year validator"`.

Pre-commit hook fires `sugar generate --staged --intent "fix off-by-one in leap year validator"`.

LLM #2 receives:
- Diff: the change to `isLeapYear`
- New tests: three test cases for years 2024, 2100, 2000
- Intent: "fix off-by-one in leap year validator"

LLM #2 synthesizes:

```ts
// src/dates/validator.invariant.ts (generated)
import { isLeapYear } from './validator';
import { property, forAll, iff } from 'sugar/ir';

property("leapYearGregorianRule",
  forAll<Int>(year =>
    iff(
      isLeapYear(year),
      year % 400 === 0 || (year % 4 === 0 && year % 100 !== 0)
    )
  )
);
```

This invariant generalizes the test cases. It says: for any year, `isLeapYear` returns true if and only if the Gregorian leap-year rule is satisfied. The three test cases are point-instances of this universal claim.

`sugar prove` verifies the diff against this invariant. Pass. Commit lands.

**A future commit attempts to "optimize" `isLeapYear`:**

```ts
// src/dates/validator.ts (modified)
export function isLeapYear(year: number): boolean {
  return year % 4 === 0; // simplified, but wrong
}
```

Prover walks the new function. Invariant `leapYearGregorianRule` evaluated. Counterexample: year = 2100. The simplified function returns `true` (2100 % 4 === 0), but the Gregorian rule says false. Invariant violated.

Commit rejected. The "optimization" can't land. The original semantics are preserved by the invariant generated months ago from the test cases.

## 16. Legacy Adoption (Retrospective)

**For pre-existing code (legacy import flavor), the framework supports retrospective invariant generation without modifying the existing artifact.**

**The flow:**

1. Existing code lives at `src/legacy/` (or anywhere). It has been in production for N years. It is bit-identical to whatever it was before Sugar arrived.
2. `sugar generate --legacy src/legacy/ --intent "what does this code do (human description)"`
3. LLM #2 reads the existing code's AST and the human's description.
4. LLM #2 generates invariants ABOUT the existing code: "the function returns a non-negative integer," "the function is deterministic in its inputs," "the function preserves the sort order of its array argument," etc.
5. Generated invariants are written to `src/legacy/<file>.invariant.ts`.
6. A human reviews the generated invariants. They accept, reject, or edit.
7. Accepted invariants land as mementos. They become part of the verification corpus for all future commits.
8. The existing code is NEVER modified. Removing the `.invariant.ts` files leaves the legacy code untouched.

**Why this matters for the COBOL/Rust/Java story:**

A 40-year-old COBOL banking system has been running in production for decades. The audit trail spans every line. Modifying the COBOL — even adding a comment — changes the artifact's hash, breaks audit chains, triggers re-certification. In regulated industries this can void compliance posture entirely.

Sugar's retrospective flow works on this artifact untouched. Invariants are written in `*.invariant.cobol` files (per-language-kit-standard-compliant). The existing COBOL stays bit-identical. Accumulated invariants tighten the envelope around the legacy artifact without ever modifying it.

Same architecture for Rust, Java, Python, anywhere. The kit's lifter handles the host language; the constraint files are external; the existing artifact is preserved.

**Iterative adoption:**

Legacy adoption is gradual:
- Day 1: add invariants for one critical function.
- Day 30: add invariants for one critical module.
- Day 365: add invariants for the whole subsystem.

At every point, the codebase is partially verified. There's no "all or nothing" cliff. Each invariant added increases coverage; each violation rejected during ongoing commits proves the invariant's value.

## 17. Cut List (v1)

The following are out of scope for v1:

- **Recursion in predicate bodies.** A predicate body that references itself (directly or indirectly). v1 forbids; v2+ may allow as per-kit extension if the kit's solver supports inductive definitions.
- **Higher-order quantifiers.** Quantifiers over functions (∀f. P(f)) or over predicates (∀p. P(p)) are out. v1 quantifies only over sort-typed values.
- **Type-class polymorphism in IR.** Generic functions in production code can be called from invariants only if the kit supplies a symbolic-range contract for each instantiation; v1 doesn't synthesize contracts across type parameters.
- **Effect tracking beyond the pure registry.** The pure-function registry is binary: pure or impure. v1 doesn't model "pure modulo logging" or "deterministic but consults a constant config." Add such functions to the registry only if their full effect can be modeled as pure.
- **Existential quantifier elimination via Skolemization in the IR.** The IR retains existentials; the prover handles Skolemization internally if its solver requires it. The IR is at the FOL level; the prover may transform deeper.
- **Subtype reasoning.** A function expecting `Real` accepting `Int` because `Int <: Real`. v1 requires explicit type-level coercion (`x: Real = anInt`) in the invariant body.
- **Numeric tolerance in equality.** Floating-point `===` is exact. v1 has no `approximatelyEqual` builder. Use `Math.abs(a - b) < epsilon` if needed; that lifts cleanly.
- **Cross-file property composition beyond `ref`.** v1 supports `ref("name")` for explicit cross-file invariant reference. Implicit composition (e.g., "all invariants in `src/billing/` apply to `src/billing/`") is out.
- **Auto-discovery of `.invariant.ts` files outside the project root.** v1 walks only the project's `tsconfig.json`-included files. External invariant libraries are not loaded.

## 18. v2+ Roadmap

- **Recursion via inductive definitions.** Per-kit extension if solver supports.
- **Higher-order quantifiers.** Quantifiers over predicates and functions.
- **Effect tracking.** Beyond the pure/impure binary; "pure modulo specific I/O" categories.
- **Subtype reasoning.** Implicit `Int → Real` via subsort declaration in the IR library.
- **Approximate equality.** `approxEq(a, b, epsilon)` builder for floating-point invariants.
- **Cross-file composition by directory convention.** Auto-apply `dir/.invariant.ts` to all production code in `dir/`.
- **External invariant libraries.** Importable invariant packages (e.g., `sugar-invariants-finance`).
- **IDE integration.** LSP-level "show me invariants for this function" hovers, "verify this change" code lens, "generate invariants" code action.
- **Counterexample-driven test generation.** Prove fails → automatically generate a regression test from the counterexample.

## 19. Verification of This Spec Itself

**This spec is itself a candidate for self-application.** When Sugar is dogfooded against its own implementation:

- The lifter implementation in `src/ir/lift/` becomes a unit of work.
- The lifter's tests in `src/ir/lift/*.test.ts` become the existential intent.
- Sugar's own `sugar generate` produces invariants about the lifter (e.g., "the lifter's output IR validates against the canonicalizer's IrFormula type," "the lifter rejects all OUT-list constructs").
- Sugar's own `sugar prove` verifies the lifter against those invariants.

The framework verifies itself. The verification of the verifier is the strongest form of dogfooding the architecture supports.

## Appendix A: Worked Examples

### A.1 Pythagorean theorem invariant

```ts
// src/geometry/triangle.ts
export function hypotenuse(a: number, b: number): number {
  return Math.sqrt(a * a + b * b);
}
```

```ts
// src/geometry/triangle.invariant.ts
import { hypotenuse } from './triangle';
import { property, forAll, implies } from 'sugar/ir';
import type { Real } from 'sugar/sorts';

property("hypotenusePythagorean",
  forAll<Real>(a =>
    forAll<Real>(b =>
      implies(
        a >= 0 && b >= 0,
        hypotenuse(a, b) * hypotenuse(a, b) === a * a + b * b
      )
    )
  )
);

property("hypotenuseNonNegative",
  forAll<Real>(a =>
    forAll<Real>(b =>
      hypotenuse(a, b) >= 0
    )
  )
);

property("hypotenuseSymmetric",
  forAll<Real>(a =>
    forAll<Real>(b =>
      hypotenuse(a, b) === hypotenuse(b, a)
    )
  )
);
```

### A.2 Sort preservation invariant

```ts
// src/util/sortable.ts
export function sortAsc(xs: number[]): number[] {
  return [...xs].sort((a, b) => a - b);
}
```

```ts
// src/util/sortable.invariant.ts
import { sortAsc } from './sortable';
import { property, forAll, implies } from 'sugar/ir';
import type { Real } from 'sugar/sorts';

property("sortAscMonotonic",
  forAll<Real[]>(xs =>
    sortAsc(xs).every((x, i, arr) => implies(i > 0, arr[i - 1] <= x))
  )
);

property("sortAscPreservesLength",
  forAll<Real[]>(xs => sortAsc(xs).length === xs.length)
);

property("sortAscPreservesElements",
  forAll<Real[]>(xs =>
    xs.every(x => sortAsc(xs).some(y => y === x))
  )
);
```

### A.3 Validation predicate invariant

```ts
// src/auth/email.ts
export function isValidEmail(s: string): boolean {
  return /^[^@]+@[^@]+\.[^@]+$/.test(s);
}
```

```ts
// src/auth/email.invariant.ts
import { isValidEmail } from './email';
import { property, forAll, implies } from 'sugar/ir';
import type { StringSort } from 'sugar/sorts';

property("validEmailContainsAt",
  forAll<StringSort>(s =>
    implies(isValidEmail(s), s.includes('@'))
  )
);

property("validEmailContainsDot",
  forAll<StringSort>(s =>
    implies(isValidEmail(s), s.includes('.'))
  )
);
```

## Appendix B: Reference Grammar

```
formula ::= forAll<sort>(λparam.body)
          | exists<sort>(λparam.body)
          | implies(formula, formula)
          | iff(formula, formula)
          | array.every(λparam.body)
          | array.some(λparam.body)
          | binaryFormula
          | unaryFormula
          | atomic

binaryFormula ::= body && body | body || body | body === body | body !== body
                | body < body  | body <= body | body > body  | body >= body

unaryFormula ::= !body

atomic ::= identifier | numericLiteral | booleanLiteral | stringLiteral
        | identifier(args)        // call into pure registry
        | identifier.field         // member access on parameter
        | identifier?.field        // optional member access
        | identifier ?? identifier // nullish coalescing
        | (cond ? a : b)           // ternary

body ::= formula | atomic | (formula)

sort ::= Int | Real | Bool | StringSort | userBrandedType

args ::= ε | atomic | atomic, args
```

## Appendix C: One-Size-Fits-All LLM #2 Template

The framework invokes LLM #2 with this template at commit time. Variables in `{{}}` are substituted at runtime.

```
You are writing invariants for a TypeScript codebase using the Sugar framework.

Below is a code diff. Below that is the test code added or modified in the same
diff. Below that is a description of the developer's intent for this change.

Your task: write IR invariants in TypeScript that:
- Pass for all the listed tests (the tests are existential examples of intent)
- Are consistent with the diff's intended semantics
- Capture properties the modified function should satisfy for ALL inputs in the domain
- Use only the IR subset (specified below)

Output: TypeScript source for a `.invariant.ts` file. Do not output anything else.

== IR SUBSET CONSTRAINTS ==

Allowed:
- Operators: ===, !==, <, <=, >, >=, &&, ||, !, +, -, *, /, %
- Optional chaining: ?.
- Nullish coalescing: ??
- Ternary: cond ? a : b
- Quantifiers: xs.every(x => P(x)), xs.some(x => P(x))
              forAll<T>(x => P(x)), exists<T>(x => P(x))
- Calls into the registry: Math.abs, Math.max, Math.min, parseInt, etc.
- Calls into in-scope production-code functions (must be pure)
- Number, boolean, string, null, undefined literals
- Lambda params, member access on params

Forbidden (compile-time error):
- async/await, generators, Promise
- for/while/do loops (use .every / .some instead)
- Mutations (=, ++, --, .push, etc.)
- try/catch/throw
- this, new, prototype access, classes
- Side-effecting calls (anything not in the registry)
- Closure over mutable bindings (let/var)
- Recursion in predicate bodies

== DIFF ==
{{diff}}

== TESTS (existential intent) ==
{{tests}}

== INTENT (universal context) ==
{{intent_text}}

== TARGET FILES ==
{{file_paths_for_invariant_files}}

Output the .invariant.ts source. Use the API:
  import { property, forAll, exists, implies, iff } from 'sugar/ir';
  property("name", formula);
```

## Appendix D: Compatibility Notes

- TypeScript version: 5.0+. The lifter uses APIs introduced in TS 5.0 for type-checker integration.
- Node version: 20+. The lifter and prover require modern V8 for `JSON.stringify` RFC 8785 conformance (see `2026-04-29-ast-canonicalizer.md` §3.2.2.3).
- Module resolution: NodeNext. The lifter walks ESM module graphs to resolve cross-file references.
- Strict mode: Required. The lifter assumes `tsc --strict` so type-checker output is reliable.

## Appendix E: Glossary

- **Lifter:** the AST → IR projector. Single visitor pass over `tsc.Program`.
- **Lowerer:** the IR → AST regenerator. Round-trip companion to the lifter.
- **Sort:** an IR type. Primitives (`Int`, `Real`, `Bool`, `StringSort`) plus user-defined branded types.
- **Property:** a named invariant declaration in a `.invariant.ts` file.
- **Predicate:** the body of a property — a TypeScript expression returning `boolean`.
- **Pure-function registry:** the kit-supplied catalog of functions the lifter and prover trust as pure.
- **Symbolic range:** a registry-supplied description of what values a function's call site might produce.
- **Shadow AST walking:** the prover's symbolic interpretation of code AST against existing invariants.
- **Generate:** the framework command that produces invariants from (diff, tests, intent).
- **Prove:** the framework command that verifies invariants against code via shadow AST walking.
- **Memento:** a content-addressed evidence record stored in the memento store.
- **Producer:** an LLM, prover, or analyzer that emits mementos. Identified by `producedBy`.

---

**Spec status:** v1 canonical. Ready for lifter implementation.

**Next:** implement `src/ir/lift/` to spec. Implementation prompt to follow as `2026-04-29-lifter-implementation-prompt.md`.
