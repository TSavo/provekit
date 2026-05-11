# Ruby Language Signature

Draft ProvekIt operation algebra for Ruby source lifted by `provekit-lift-ruby-source`.

This catalog intentionally models a conservative Ruby subset. Every operation is namespaced with `ruby:` and unsupported syntax is refused by the lifter rather than represented as an unknown or skip node.

## Operations

The draft includes source-unit wrapping, sequencing, assignment, conditionals, loops, returns, raises, calls, receiver sends, indexing, Ruby variable-cell reads, boolean short-circuit operators, ternary expressions, arithmetic, comparison, bitwise, and unary expressions.

## Effects

The Ruby source lifter emits the canonical ProvekIt effect wire shapes:

- `{"kind":"reads","target":"..."}`
- `{"kind":"writes","target":"..."}`
- `{"kind":"io"}`
- `{"kind":"panics"}`
- `{"kind":"unresolved_call","name":"..."}`
- `{"kind":"opaque_loop","loopCid":"blake3-512:..."}`

Effects are sorted by the canonical `Effect::sort_key()` ordering before emission.
