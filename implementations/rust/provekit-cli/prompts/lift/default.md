# `provekit lift`: propose contracts authored in the kit

You are reading a source file and proposing one or more **ContractDecl**
candidates: pre/post/inv triples that capture each public function's
behavioral guarantees. You write them in the **ProvekIt kit's
authoring API** for the host language (Rust kit, TypeScript kit). The
kit collector desugars to IR; the IR-JSON appears in the wire field
below for the validator's convenience.

You never write SMT-LIB. You never write raw IR-JSON outside the wire
field. The kit is the only authoring surface humans use, so it's the
only authoring surface you use.

This is the LLM-assisted lift path. The mechanical lift adapters
(proptest, contracts, kani, etc.) ran first and minted what they
could; you fill the gap by reading prose comments, JSDoc, type
signatures, and the function's intent.

## Input

- **Source file**: `{{source_file_path}}`
- **Source contents**:
  ```
  {{source_file_contents}}
  ```
- **Function name** (optional filter; empty = propose for all
  public functions): `{{function_name}}`
- **Existing contracts**: `{{existing_contracts}}`: don't propose
  these again unless you're replacing them with a strictly tighter
  version.
- **Previous rejection**: `{{previous_rejection}}`

## Worked example

Source:

```rust
/// Returns the length of `s`. Always non-negative.
pub fn length(s: &str) -> usize { s.len() }
```

Kit code (Rust):

```rust
use provekit::ir_symbolic::{contract, ge, num, make_var, ContractArgs};

contract(
    "length_nonneg",
    ContractArgs {
        pre:  None,
        post: Some(ge(make_var("out"), num(0))),
        inv:  None,
        out_binding: Some("out".into()),
    },
);
```

The desugared IR-JSON for `post`:

```
{"kind":"atomic","name":">=","args":[{"kind":"var","name":"out"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}
```

## Wire format

A JSON **array** of ContractCandidate objects (same shape as `must`,
but multiple candidates per call):

```json
[
  {
    "name": "<snake_case>",
    "pre":  "<IR-JSON or omit>",
    "post": "<IR-JSON or omit>",
    "inv":  "<IR-JSON or omit>",
    "out_binding": "out",
    "provenance": { ... }
  },
  ...
]
```

Each candidate is independently validated and minted; one bad
candidate doesn't sink the others.

## How to choose what to lift

1. **Public surface area first**: exported functions, methods on
   public classes, entry points. Internal helpers don't ship.
2. **Read the doc comments**: JSDoc / Rustdoc often state the
   invariant in prose. Translate to kit code.
3. **Type signatures are weak constraints**: `fn len(s: &str) -> usize`
   yields `post: ge(out, num(0))`. State it.
4. **Input domain restrictions**: if the function panics on negative
   input, that's a `pre`: `pre: Some(ge(arg, num(0)))`.
5. **Conservation laws**: ledgers, queues, stacks: anything with an
   "in equals out" reading is an `inv` quantified over the relevant
   domain.

## Kit primitive cheatsheet

(Same as `must.md`; not duplicated here. See that prompt for the full
desugaring table.)

## Calibration

For each candidate, `provenance.confidence` should reflect honest
estimate of survival through validation + Z3. Over-confidence is
recorded by the lattice.

## Output

Return **only** the JSON array. Even a single candidate must be
`[{...}]`. No prose, no Markdown fences.
