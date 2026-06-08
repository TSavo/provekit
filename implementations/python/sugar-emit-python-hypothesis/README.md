# sugar-emit-python-hypothesis

A PEP 1.7.0 **emitter** kit for Python `hypothesis`. It takes neutral
predicate terms and emits native Python property tests using `@given` and
`hypothesis.strategies`.

The framework mapping is Python kit code. The Rust CLI only sees the
`.sugar/config.toml` registration, the `.sugar/emit/python-hypothesis`
manifest, and JSON-RPC plugin methods.

## Supported slice

This first slice emits only predicates whose generated strategies can satisfy
the assertion without guessing host semantics:

- `concept:eq`, `concept:ne`, `concept:lt`, `concept:gt`, `concept:le`,
  `concept:ge` over direct variables and integer constants.
- `concept:option-is-none` and `concept:option-is-some` over direct variables.

Unsupported predicates and term shapes are returned in
`unsupported_predicates` instead of being emitted as vacuous tests.

## Tests

```
cd implementations/python/sugar-emit-python-hypothesis
python3 -m pytest
```
