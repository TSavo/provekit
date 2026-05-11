# Rust Language Signature Memento

Rust is emitted as a content-addressed algebra over contracts. Here is the algebra.

The minted Rust `LanguageSignatureMemento` is:

`blake3-512:6e96976ee181cc32de6dfb326b9b9a96e5f47b7ba8afef9606d93cee15984fc1c81de78491da094a788bf50725b26824e22b16d79a5b80dc76cd169c59aa844c`

The carrier is the function contract space: `FunctionContractMemento`, predicate terms, and WP-propagated contract values. A lifted Rust function body is a term over this signature. Evaluation of that term propagates weakest preconditions and returns a contract memento.

The full component CID list is in `component-cids.json`. The mint log is in `cids.tsv`. The unsigned development catalog is under `catalog/`.

## Sorts

Core Rust sorts:

| Sort | Role |
| --- | --- |
| `Stmt` | Carrier sort for statement-level terms |
| `Expr` | Expression terms |
| `Place` | Assignable Rust places |
| `Int` | Integer values, with debug overflow modeled as `Panic` |
| `Bool` | Branch truth values |
| `Unit` | Unit input and output |
| `Ref` | Safe borrowed references |
| `RawPtr` | Unsafe raw pointers |
| `Lifetime` | Lifetime and region values |
| `Result` | Carrier for `?` over `Result` |
| `Option` | Carrier for `?` over `Option` |
| `Closure` | Closure values |
| `Slice` | Slice and array views |
| `Box` | Owned heap values |
| `MatchArm` | One match arm |
| `ListOfArm` | Ordered match arm lists |
| `Pattern` | Match and let patterns |

Helper sorts are minted for operation arities and effect signatures: `FnContract`, `FieldName`, `ListOfStmt`, `ListOfExpr`, `Addr`, `Value`, `Reason`, `Bottom`, `Sort`, `Float`, `String`.

## Operations

| Operation | Arity | Result | Contract meaning |
| --- | --- | --- | --- |
| `skip` | `Unit` | `Stmt` | Identity statement, unchanged state |
| `seq` | `Stmt x Stmt` | `Stmt` | WP composition |
| `if` | `Bool x Stmt x Stmt` | `Stmt` | Branch-selected WP |
| `while` | `Bool x Stmt` | `Stmt` | Loop invariant plus false condition at exit |
| `for` | `Stmt x Bool x Stmt x Stmt` | `Stmt` | Core shape, Rust iterator desugar is an equation |
| `switch` | `Int x ListOfStmt` | `Stmt` | LLBC SwitchInt special case, derived from `match` |
| `call` | `FnContract x ListOfExpr` | `Stmt` | CCP composition at call site |
| `return` | `Expr` | `Stmt` | Bind output and exit |
| `break` | `Unit` | `Stmt` | Exit enclosing match or loop |
| `continue` | `Unit` | `Stmt` | Jump to next loop iteration |
| `deref` | `Ref` | `Place` | Safe reference dereference, effect `MemRead` |
| `member` | `Place x FieldName` | `Place` | Field place projection |
| `add`, `sub`, `mul`, `neg` | `Int` operands | `Int` | Arithmetic, debug overflow is `Panic` |
| `eq`, `lt`, `le`, `ne`, `gt`, `ge` | `Int x Int` | `Bool` | Integer comparisons |
| `and`, `or`, `not` | `Bool` operands | `Bool` | Boolean operations |
| `assign` | `Place x Expr` | `Stmt` | Store, effect `MemWrite` |
| `panic` | `Reason` | `Bottom` | Controlled divergence, effect `Panic` |
| `try`, `try_option` | `Result` or `Option` | `Expr` | Payload or early return residual |
| `match` | `Expr x ListOfArm` | `Stmt` | Guarded WP join over arms |
| `borrow`, `borrow_mut` | `Place` | `Ref` | Shared or mutable borrow |
| `deref_raw` | `RawPtr` | `Place` | Unsafe raw dereference |
| `drop` | `Place` | `Stmt` | Drop glue, effect `Drop` |
| `await` | `Expr` | `Expr` | Async suspension, effect `Async` |
| `index` | `Slice x Int` | `Place` | Bounds checked indexing |
| `box_new` | `Expr` | `Box` | Heap allocation, effect `Alloc` |
| `closure`, `closure_call` | captures and args | `Closure` or `Stmt` | Closure construction and call |
| `cast` | `Expr x Sort` | `Expr` | Rust `as` cast |
| `move` | `Place` | `Expr` | Read and invalidate source place |

Additional walker coverage operations are minted for current `IrTerm::Ctor` names: `div`, `rem`, `bit_and`, `bit_or`, `bit_xor`, `shl`, `shr`, `bit_not`, `field`, `range`, `range_incl`, `tuple`, `array`, `array_repeat`, `len`, `ite`, `method_call`, `call_result`, `loop`, `let`, `into_iter`, `next`, `arm`, `guarded_arm`, and pattern constructors.

## Equations

Minted core laws:

```text
seq(seq(a, b), c) = seq(a, seq(b, c))
seq(skip, a) = a
seq(a, skip) = a
if(true, a, b) = a
if(false, a, b) = b
if(p, a, a) = a
while(false, a) = skip
and(false, b) = false
and(true, b) = b
or(true, b) = true
or(false, b) = b
not(not(p)) = p
```

Rust-specific laws:

```text
try(e) = match_expr(e, [arm(Ok(v), v), arm(Err(err), return(call_result(into_contract, err)))])
for pat in iter body = let it = into_iter(iter); loop(match(next(it), Some(pat) => body; continue, None => break))
if let P = e { a } else { b } = match(e, [P => a, _ => b])
while let P = e { body } = loop(match(e, [P => body; continue, _ => break]))
drop(p) = skip when p has no drop glue
deref(borrow(p)) = p, modulo aliasing
deref(borrow_mut(p)) = p, modulo aliasing
read(addr) after write(addr, v) = v
```

# DEVIATION: `try` returns a payload expression, while the primitive `match` operation is statement-valued by design. The `question-desugar` equation uses an expression-valued helper `match_expr` so the equation is sort coherent. The statement-valued `match` remains the primitive for WP over Rust match statements.

## Effect Signatures

| Effect signature | Operations | Walker effect alignment |
| --- | --- | --- |
| `MemRead` | `read : Addr -> Value` | `Reads` |
| `MemWrite` | `write : Addr x Value -> Unit` | `Writes` |
| `IO` | `input`, `output` | `Io` |
| `Panic` | `panic : Reason -> Bottom` | `Panics` |
| `Alloc` | `alloc`, `dealloc` | allocation operations |
| `Unsafe` | `unsafe_marker` | `Unsafe` |
| `Async` | `suspend` | async suspension |
| `Drop` | `drop_glue` | `Drop` |
| `UnresolvedCall` | `unresolved_call` | `UnresolvedCall` |
| `OpaqueLoop` | `opaque_loop` | `OpaqueLoop` |
| `EarlyReturn` | `early_return` | `EarlyReturn` |
| `ClosureCapture` | `closure_capture` | `ClosureCapture` |
| `PinnedReference` | `pin_invariant` | `PinnedReference` |
| `RawPointerProvenance` | `raw_ptr_provenance` | `RawPointerProvenance` |
| `AtomicAccess` | `atomic_load`, `atomic_store`, `atomic_rmw`, `atomic_cas` | `AtomicAccess` |
| `PossibleAliasing` | `possible_aliasing` | `PossibleAliasing` |

## Foo Example

Source:

```rust
fn foo(x: i32) -> i32 { if x == 0 { -22 } else { x } }
```

Algebra term:

```text
seq(if(eq(x, 0), return(neg(22)), skip), return(x))
```

Branch-sensitive WP value:

```text
pre = true
post = result = ite(eq(x, 0), -22, x)
effects = []
```

The Rust to shape morphism CID is `blake3-512:930bbc7ead57475e33e89ddeefbf9ae387c1d91efda4525ec76413f663645ada4315444dfd3a8eb9d023e26b3890c2cc5a2f5f1de460ae84d1b19dbc512b7a04`.

At the shape level, `rust foo`, `c foo`, `arm foo`, and `x86 foo` are the same algebraic shape. The discharge receipt CID is `blake3-512:4490fa0fc695594bedfe13aae3e49597279de6f67587cf12f3cadb892a9537423bd5b0a2ed0543eb013396bbf80f80f458fcf512dd38bf7878e7621815711bf2`.

## Verification Recipe

From the repository root:

```sh
cd implementations/rust
cargo build -p provekit-cli -p provekit-ir-compiler-maude
cd ../..
sh menagerie/rust-language-signature/mint.sh
cargo build --manifest-path implementations/rust/Cargo.toml -p provekit-walk
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-walk
cargo clippy --manifest-path implementations/rust/Cargo.toml -p provekit-walk -- -D warnings
```

The `mint.sh` script uses `--unsigned`. Current minter guardrails require a lexical `dev` path component for unsigned catalogs, so the script passes `dev/../catalog` while writing the actual catalog at `catalog/`. Production signing with the foundation v0 key is a follow-up.

## Scope

This is the core Rust signature, not exhaustive Rust. Extending it with more sorts, more operators, more equations, and more effect laws is mint-more-mementos work, not new substrate machinery.

References:

- `protocol/specs/2026-05-09-algorithm-memento-protocol.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`
- `docs/papers/16-after-consensus-universal-address-space.md`

T Savo
