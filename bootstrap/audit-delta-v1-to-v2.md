# Audit Delta v1 to v2

## Summary

| Outcome | v1 | v2 | Delta |
| --- | ---: | ---: | ---: |
| handles-fully | 217 | 497 | 280 |
| handles-partially-with-loss-record | 309 | 261 | -48 |
| refuses-with-typed-reason | 583 | 351 | -232 |

- Total items audited v2: 1109
- Success metric: handles-fully v2 497 vs required 434; refuses v2 351 vs fallback ceiling 200.
- Success metric result: pass
- Target check handles-fully plus partial: 758/1109 (68.3%) vs 90.0%.

## Per-gap-class delta

| Class | v1 | v2 | Delta | Resolution PR |
| --- | ---: | ---: | ---: | --- |
| `Expr::Let` | 0 | 7 | 7 | #955 |
| `Expr::Macro` | 0 | 25 | 25 | #955 |
| `abi-attribute-not-carried` | 5 | 5 | 0 | #953 |
| `block-without-tail` | 0 | 17 | 17 | post-D5 residual |
| `complex-generic` | 26 | 0 | -26 | #956 |
| `ffi-call` | 44 | 0 | -44 | #946 |
| `ffi-call-unresolved-effect` | 0 | 204 | 204 | #946 |
| `function-not-found` | 0 | 11 | 11 | post-D5 residual |
| `impl-associated-const-not-lowered` | 1 | 0 | -1 | post-D5 residual |
| `impl-associated-type-not-lowered` | 12 | 12 | 0 | #953 |
| `impl-generics-not-carried` | 2 | 0 | -2 | post-D5 residual |
| `let-binding` | 175 | 0 | -175 | #946 |
| `nested-item` | 3 | 0 | -3 | #956 |
| `procedural-macro` | 254 | 249 | -5 | #953 |
| `residual-term-emitter` | 0 | 1 | 1 | post-D5 residual |
| `return-type-byte-vec` | 0 | 13 | 13 | #946 |
| `return-type-option` | 0 | 16 | 16 | #946 |
| `return-type-result` | 0 | 65 | 65 | #946 |
| `return-type-user-defined` | 0 | 131 | 131 | #946 |
| `return-type-vec` | 0 | 1 | 1 | #946 |
| `statement-macro` | 5 | 10 | 5 | #953 |
| `term-emitter-unsupported` | 64 | 0 | -64 | #955 |
| `trait-path-truncated` | 33 | 106 | 73 | #953 |
| `type-inference-assumed-bool` | 0 | 11 | 11 | #955 |
| `type-inference-assumed-int` | 0 | 10 | 10 | #955 |
| `type-sort-opaque` | 1 | 0 | -1 | post-D5 residual |
| `unsupported-boolean-if` | 0 | 1 | 1 | post-D5 residual |
| `unsupported-let-pattern` | 0 | 27 | 27 | post-D5 residual |
| `unsupported-literal` | 0 | 160 | 160 | post-D5 residual |
| `unsupported-return-type` | 267 | 4 | -263 | #946 |
| `unsupported-stmt-binary` | 0 | 1 | 1 | post-D5 residual |
| `unsupported-stmt-call` | 0 | 25 | 25 | post-D5 residual |
| `unsupported-stmt-method-call` | 0 | 34 | 34 | post-D5 residual |
| `unsupported-stmt-while` | 0 | 1 | 1 | post-D5 residual |
| `unsupported-value-cast` | 0 | 2 | 2 | post-D5 residual |
| `unsupported-value-closure` | 0 | 46 | 46 | post-D5 residual |
| `unsupported-value-for-loop` | 0 | 1 | 1 | post-D5 residual |
| `unsupported-value-if` | 0 | 13 | 13 | post-D5 residual |
| `unsupported-value-loop` | 0 | 1 | 1 | post-D5 residual |
| `unsupported-value-range` | 0 | 1 | 1 | post-D5 residual |
| `unsupported-value-return` | 0 | 4 | 4 | post-D5 residual |
| `unsupported-value-unsafe` | 0 | 1 | 1 | post-D5 residual |
| `vec-macro-desugared-to-array` | 0 | 17 | 17 | #946 |

## Newly-emerged gap classes

- `block-without-tail`: 17
- `function-not-found`: 11
- `residual-term-emitter`: 1
- `unsupported-boolean-if`: 1
- `unsupported-let-pattern`: 27
- `unsupported-literal`: 160
- `unsupported-stmt-binary`: 1
- `unsupported-stmt-call`: 25
- `unsupported-stmt-method-call`: 34
- `unsupported-stmt-while`: 1
- `unsupported-value-cast`: 2
- `unsupported-value-closure`: 46
- `unsupported-value-for-loop`: 1
- `unsupported-value-if`: 13
- `unsupported-value-loop`: 1
- `unsupported-value-range`: 1
- `unsupported-value-return`: 4
- `unsupported-value-unsafe`: 1

## Refused floor

Residual refused classes remain:

- `unsupported-literal`: 160
- `unsupported-value-closure`: 46
- `unsupported-stmt-method-call`: 34
- `unsupported-let-pattern`: 27
- `unsupported-stmt-call`: 25
- `block-without-tail`: 17
- `unsupported-value-if`: 13
- `function-not-found`: 11
- `unsupported-value-return`: 4
- `unsupported-return-type`: 4
- `unsupported-value-cast`: 2
- `unsupported-value-for-loop`: 1
- `unsupported-value-range`: 1
- `unsupported-stmt-while`: 1
- `unsupported-boolean-if`: 1
- `unsupported-value-unsafe`: 1
- `unsupported-stmt-binary`: 1
- `unsupported-value-loop`: 1
- `residual-term-emitter`: 1

## Resolution PR map

- #946: D2 return sort, let binding, and call/method-call lifting.
- #953: D3 accepted named-loss classes for macros, trait paths, associated types, ABI attributes, and statement macros.
- #955: D4 term-emitter expression and statement coverage.
- #956: D5 generic and nested-item handling.
