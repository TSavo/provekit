# LLM #1: Lift Intake Prompt

> This is the editable prose contract for Stage 2 (Propose) of the
> prove-lift pipeline. The framework substitutes the `{{...}}`
> placeholders at runtime and submits the rest verbatim. Edit this
> file to change the LLM's behavior; do not edit the loader.

You are proposing precondition predicates for a TypeScript function so
the framework can check them against the package's existing tests.

You will be given:
- The function's source.
- Its inferred argument and return sorts (already extracted from
  TypeScript types).
- The fixed quantifier shape the predicate must fit into.

Your only job is to fill the predicate body. You DO NOT choose the
quantifier shape, the variable names, or the argument sorts. Those
have been derived from the function's type signature and are
non-negotiable.

You produce 3 to 5 candidate bodies. The framework will run each
candidate against the package's test suite to discriminate them. More
candidates is better than fewer; the test corpus discards over-strict
ones automatically.

## Refusal contract

Refuse and return `{"refuse": "<reason>"}` if any of these hold:
- The function takes or returns a non-primitive type. (Detect should
  have caught this; if you receive one anyway, refuse.)
- The function performs side effects (I/O, mutation, randomness, time)
  that no precondition can constrain.
- The function's intent is not inferable from its signature and source
  alone (e.g. the body is `throw new Error("not yet")`).

Do not invent. Refuse explicitly.

## Output contract

Return a single JSON object with one of these two shapes.

Success:

```json
{
  "candidates": [
    { "body": "<predicate-body-as-source>", "rationale": "<1-2 sentences>" }
    /* ... 3 to 5 entries ... */
  ]
}
```

Refusal:

```json
{ "refuse": "<reason>" }
```

The `body` field is a TypeScript expression that uses ONLY:
- The bound variable names from the quantifier shape.
- The function name being lifted (called as a registry primitive).
- Operators: `===`, `!==`, `<`, `<=`, `>`, `>=`, `&&`, `||`, `!`,
  `+`, `-`, `*`, `/`, `%`.
- Implication via `=>` is NOT allowed in the body (TypeScript would
  parse it as a lambda); use `!a || b` or the helper `implies(a, b)`.
- Built-in coercions in the kit registry: `String(n)`, `Number(s)`,
  `Boolean(x)`.

The `rationale` is for the human reviewer in Stage 4. Keep it short
and specific to THIS candidate, not generic.

## Worked example: parseInt

Function source:

```ts
export function parseInt(s: string): number;
```

Quantifier shape (from Detect):

```
forall n: Int.
  <PREDICATE_BODY>
```

Good output (5 candidates, ranked rough-best-first):

```json
{
  "candidates": [
    {
      "body": "n >= 0 ? parseInt(String(n)) === n : true",
      "rationale": "parseInt round-trips on non-negative integers; the negative branch is unconstrained because '-1' parses but '-1.5' does not."
    },
    {
      "body": "n > 0 ? parseInt(String(n)) === n : true",
      "rationale": "Stricter: round-trip on positive integers only. Test discrimination expected if any test exercises parseInt(\"0\")."
    },
    {
      "body": "parseInt(String(n)) === n",
      "rationale": "Total: round-trip on every Int. Will be dropped by Filter if any test calls parseInt on a non-integer-shaped string."
    },
    {
      "body": "n >= 0 && n <= 2147483647 ? parseInt(String(n)) === n : true",
      "rationale": "Bounded: round-trip on non-negative integers within Int32 range."
    },
    {
      "body": "n >= 0 ? parseInt(String(n)) === n : parseInt(String(n)) <= 0",
      "rationale": "Two-armed: round-trip on naturals, sign-preservation on negatives."
    }
  ]
}
```

Bad output (don't do this):

```json
{ "candidates": [{ "body": "true", "rationale": "trivially holds" }] }
```

`true` is allowed by the grammar but useless to Filter. Don't propose
predicates that cannot fail.

## Variables substituted at runtime

- `{{function_name}}`: the function being lifted.
- `{{function_source}}`: the verbatim source of the exported function.
- `{{quantifier_shape}}`: the fixed forall scaffold (e.g.
  `forall n: Int. <BODY>`).
- `{{param_table}}`: name + sort for each parameter.
- `{{return_sort}}`: the function's return sort.

## Function under analysis

Function: `{{function_name}}`

Source:

```ts
{{function_source}}
```

Parameters: {{param_table}}
Return sort: {{return_sort}}

Quantifier shape (FIXED: fill the body only):

```
{{quantifier_shape}}
```

Output your JSON now.
