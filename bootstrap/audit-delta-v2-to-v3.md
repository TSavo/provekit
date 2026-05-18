# Audit Delta v2 to v3

## Summary

| Outcome | v2 | v3 | Delta |
| --- | ---: | ---: | ---: |
| handles-fully | 497 | 507 | 10 |
| handles-partially-with-loss-record | 261 | 324 | 63 |
| refuses-with-typed-reason | 351 | 278 | -73 |

- Total items audited v3: 1109
- Success metric: handles-fully v3 507 vs required 994; refuses v3 278 vs fallback ceiling 200.
- Success metric result: miss
- Target check handles-fully plus partial: 831/1109 (74.9%) vs 90.0%.

## Per-gap-class delta

| Class | v2 | v3 | Delta | Resolution PR |
| --- | ---: | ---: | ---: | --- |
| `Expr::Let` | 7 | 8 | 1 | #955 |
| `Expr::Macro` | 25 | 0 | -25 | #955 |
| `abi-attribute-not-carried` | 5 | 5 | 0 | #953 |
| `block-without-tail` | 17 | 21 | 4 | post-D5 residual |
| `closure-captures-environment` | 0 | 23 | 23 | post-D5 residual |
| `ffi-call-unresolved-effect` | 204 | 277 | 73 | #946 |
| `function-not-found` | 11 | 12 | 1 | post-D5 residual |
| `impl-associated-type-not-lowered` | 12 | 12 | 0 | #953 |
| `let-binding-mutability` | 0 | 28 | 28 | post-D5 residual |
| `macro-not-expanded` | 0 | 58 | 58 | post-D5 residual |
| `procedural-macro` | 249 | 0 | -249 | #961 |
| `residual-term-emitter` | 1 | 11 | 10 | post-D5 residual |
| `return-type-byte-vec` | 13 | 14 | 1 | #946 |
| `return-type-option` | 16 | 21 | 5 | #946 |
| `return-type-result` | 65 | 99 | 34 | #946 |
| `return-type-user-defined` | 131 | 150 | 19 | #946 |
| `return-type-vec` | 1 | 2 | 1 | #946 |
| `statement-macro` | 10 | 0 | -10 | #953 |
| `trait-path-truncated` | 106 | 42 | -64 | #953 |
| `type-inference-assumed-bool` | 11 | 17 | 6 | #955 |
| `type-inference-assumed-int` | 10 | 21 | 11 | #955 |
| `unsupported-boolean-if` | 1 | 1 | 0 | post-D5 residual |
| `unsupported-let-pattern` | 27 | 19 | -8 | post-D5 residual |
| `unsupported-literal` | 160 | 163 | 3 | post-D5 residual |
| `unsupported-return-type` | 4 | 4 | 0 | #946 |
| `unsupported-stmt-binary` | 1 | 1 | 0 | post-D5 residual |
| `unsupported-stmt-call` | 25 | 0 | -25 | post-D5 residual |
| `unsupported-stmt-method-call` | 34 | 0 | -34 | post-D5 residual |
| `unsupported-stmt-while` | 1 | 1 | 0 | post-D5 residual |
| `unsupported-value-cast` | 2 | 5 | 3 | post-D5 residual |
| `unsupported-value-closure` | 46 | 0 | -46 | post-D5 residual |
| `unsupported-value-for-loop` | 1 | 1 | 0 | post-D5 residual |
| `unsupported-value-if` | 13 | 32 | 19 | post-D5 residual |
| `unsupported-value-loop` | 1 | 1 | 0 | post-D5 residual |
| `unsupported-value-range` | 1 | 1 | 0 | post-D5 residual |
| `unsupported-value-return` | 4 | 4 | 0 | post-D5 residual |
| `unsupported-value-unsafe` | 1 | 1 | 0 | post-D5 residual |
| `vec-macro-desugared-to-array` | 17 | 0 | -17 | #946 |

## Newly-emerged gap classes

- none.

## Refused floor

Residual refused classes remain:

- `unsupported-literal`: 163
- `unsupported-value-if`: 32
- `block-without-tail`: 21
- `unsupported-let-pattern`: 19
- `function-not-found`: 12
- `residual-term-emitter`: 11
- `unsupported-value-cast`: 5
- `unsupported-value-return`: 4
- `unsupported-return-type`: 4
- `unsupported-value-for-loop`: 1
- `unsupported-value-range`: 1
- `unsupported-stmt-while`: 1
- `unsupported-boolean-if`: 1
- `unsupported-value-unsafe`: 1
- `unsupported-stmt-binary`: 1
- `unsupported-value-loop`: 1

## Resolution PR map

- #946: D2 return sort, let binding, and call/method-call lifting.
- #953: D3 accepted named-loss classes for trait paths, associated types, ABI attributes, and statement macros.
- #955: D4 term-emitter expression and statement coverage.
- #956: D5 generic and nested-item handling.
- #961: procedural macro invocations carried as concept operations.
