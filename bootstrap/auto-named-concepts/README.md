# Auto-named anonymous concepts

`bootstrap/scripts/auto_name_anonymous_concepts.py` is an operator workflow for
annotated source produced by `provekit bind --rewrite annotate`.

The script walks an annotated source root, finds anonymous concept tags, asks a
placeholder LLM namer for semantic kebab-case names, edits those source comment
tags in place, and writes `bootstrap/auto-named-concepts/receipt.json`.

The driver only instantiates existing primitives:

- annotated-source comments emitted by the bind rewrite path
- the existing human annotation pickup path in the next bind
- a receipt under `bootstrap/auto-named-concepts`

It does not add substrate, specs, memento types, kit behavior, or pipeline code.
The LLM agent never touches substrate. It edits source comments only, such as:

```rust
// concept: UNNAMED-CONCEPT-1
// provekit:concept[blake3-512:...](UNNAMED-CONCEPT-1)
pub fn deposit_then_balance(balance: i64, amount: i64) -> i64 {
    balance + amount
}
```

After the script proposes `deposit-then-balance`, the next bind reads the
human-annotation tier from the existing source comment path:

```rust
// concept: deposit-then-balance
// provekit:concept[blake3-512:...](deposit-then-balance)
pub fn deposit_then_balance(balance: i64, amount: i64) -> i64 {
    balance + amount
}
```

Run it on a prepared annotated source tree:

```sh
python3 bootstrap/scripts/auto_name_anonymous_concepts.py /path/to/annotated-source --llm-mode stub
```

For tests and fixtures, `--llm-mode deterministic` uses the same local fake namer
and derives names from the anchor function. Operators review the resulting diff
and receipt before committing.
