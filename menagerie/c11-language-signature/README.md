# C11 Language Signature Memento

C is emitted as a content-addressed algebra over contracts. Here is the algebra.

The minted C11 `LanguageSignatureMemento` is:

`blake3-512:fdfb3b7d6b5f034be120b2bb566931c0cfc7de4375cde62c37116475deaeba849549a277b3c36e2d42b9080845103604c18f0986f64f326d2cdcb73d60ceb558`

The carrier is the function contract space: `FunctionContractMemento`, predicate terms, and WP-propagated contract values. A lifted C function body is a term over this signature. Evaluation of that term propagates weakest preconditions and returns a contract memento.

The full component CID list is in `component-cids.json`. The mint log is in `cids.tsv`. The signed development catalog is under `catalog/`.

## Sorts

Core C11 sorts:

| Sort | Role |
| --- | --- |
| `Stmt` | Carrier sort for statement-level terms |
| `Expr` | Expression terms |
| `LValue` | Assignable storage designators |
| `Int` | Signed integer values |
| `Ptr` | Pointer values |
| `Bool` | Branch truth values |
| `Unit` | Unit input and output |

Helper sorts are minted for operation arities and effect signatures: `FnContract`, `FieldName`, `ListOfStmt`, `ListOfExpr`, `Addr`, `Value`, `Reason`, `Bottom`.

## Operations

| Operation | Arity | Result | Contract meaning |
| --- | --- | --- | --- |
| `skip` | `Unit` | `Stmt` | state unchanged |
| `seq` | `Stmt x Stmt` | `Stmt` | wp(first, wp(second, post)) |
| `if` | `Bool x Stmt x Stmt` | `Stmt` | cond ? wp(then_branch, post) : wp(else_branch, post) |
| `while` | `Bool x Stmt` | `Stmt` | loop invariant holds and cond is false at exit |
| `for` | `Stmt x Bool x Stmt x Stmt` | `Stmt` | seq(init, while(cond, seq(body, step))) |
| `switch` | `Int x ListOf<Stmt>` | `Stmt` | case-dispatched WP join over arms |
| `call` | `FnContract x ListOf<Expr>` | `Stmt` | callee pre under bound args implies callee post under caller state |
| `return` | `Expr` | `Stmt` | bind function out value and exit current body |
| `break` | `Unit` | `Stmt` | exit nearest enclosing switch or loop |
| `continue` | `Unit` | `Stmt` | jump to next loop iteration |
| `deref` | `Ptr` | `LValue` | lvalue at ptr |
| `member` | `LValue x FieldName` | `LValue` | field lvalue projection |
| `add` | `Int x Int` | `Int` | mathematical integer addition when no overflow holds |
| `sub` | `Int x Int` | `Int` | mathematical integer subtraction when no overflow holds |
| `mul` | `Int x Int` | `Int` | mathematical integer multiplication when no overflow holds |
| `eq` | `Int x Int` | `Bool` | integer equality comparison |
| `lt` | `Int x Int` | `Bool` | integer less-than comparison |
| `le` | `Int x Int` | `Bool` | integer less-than-or-equal comparison |
| `and` | `Bool x Bool` | `Bool` | short-circuit conjunction |
| `or` | `Bool x Bool` | `Bool` | short-circuit disjunction |
| `not` | `Bool` | `Bool` | boolean negation |
| `assign` | `LValue x Expr` | `Stmt` | store value into target and update state |
| `neg` | `Int` | `Int` | integer arithmetic negation when no overflow holds |
| `opaque` | `Unit` | `Stmt` | stable placeholder for non-lifted or intentionally opaque cursor kinds |
| `decl` | `Expr x Expr` | `Stmt` | bind local name to initializer before continuing |
| `case` | `Expr x Stmt` | `Stmt` | switch case arm body guarded by value |
| `default` | `Stmt` | `Stmt` | switch default arm body |
| `label` | `Expr x Stmt` | `Stmt` | statement label with body |
| `goto` | `Expr` | `Stmt` | control transfer to label target |
| `do` | `Stmt x Bool` | `Stmt` | do body once, then loop while cond |
| `cast` | `Expr` | `Expr` | C cast expression preserving lifted child value |
| `array-subscript` | `Expr x Expr` | `LValue` | array element lvalue projection |
| `conditional` | `Bool x Expr x Expr` | `Expr` | ternary expression selected by cond |
| `compound-literal` | `Expr` | `Expr` | compound literal expression payload |
| `init-list` | `ListOfExpr` | `Expr` | initializer list expression payload |
| `string-literal` | `Unit` | `Expr` | string literal token payload elided at this layer |
| `char-literal` | `Unit` | `Expr` | character literal token payload elided at this layer |
| `float-literal` | `Unit` | `Expr` | floating literal token payload elided at this layer |
| `imaginary-literal` | `Unit` | `Expr` | imaginary literal token payload elided at this layer |
| `null` | `Unit` | `Expr` | null pointer constant expression |
| `generic-selection` | `Expr x ListOfExpr` | `Expr` | C11 generic selection expression |
| `stmt-expr` | `Stmt` | `Expr` | GNU statement expression payload |
| `addr-label` | `Expr` | `Expr` | GNU address-of-label expression |
| `unexposed-stmt` | `ListOfStmt` | `Stmt` | libclang unexposed statement with lifted child sequence |
| `unexposed-expr` | `ListOfExpr` | `Expr` | libclang unexposed expression with lifted child sequence |
| `binary-operator` | `Expr x Expr` | `Expr` | fallback binary operator when no core C11 operator is selected |
| `unary-operator` | `Expr` | `Expr` | fallback unary operator when no core C11 operator is selected |

## Generated Cursor Kind Operations

Additional operations below are generated from `enum CXCursorKind` for cursor kinds that do not fit the hand-curated core. Non-lifted declarations, attributes, preprocessing cursors, and non-C-family extensions map to `opaque`.

## Equations

Minted flow-control laws:

```text
seq(seq(a, b), c) = seq(a, seq(b, c))
seq(skip, a) = a
seq(a, skip) = a
if(true, a, b) = a
if(false, a, b) = b
if(p, a, a) = a
while(false, a) = skip
for(init, cond, step, body) = seq(init, while(cond, seq(body, step)))
and(false, b) = false
and(true, b) = b
or(true, b) = true
or(false, b) = b
not(not(p)) = p
```

## Effect Signatures

| Effect signature | Operations | Equations |
| --- | --- | --- |
| `MemRead` | `read : Addr -> Value` | none |
| `MemWrite` | `write : Addr x Value -> Unit` | `read(addr) after write(addr, v) = v` |
| `IO` | `input : Unit -> Value`, `output : Value -> Unit` | none |
| `Trap` | `trap : Reason -> Bottom` | none |

## Maude Discharge

The `seq-assoc` and `if-idemp` obligations are in:

```text
maude/seq_assoc.ir.json
maude/if_idemp.ir.json
```

They lower through `provekit-ir-maude` to:

```text
maude/seq_assoc.maude
maude/if_idemp.maude
```

`maude` is not installed in this environment, so the fixture verdict is saved in `maude/expected-reduce-output.txt`.

Expected results:

| Law | Verdict |
| --- | --- |
| `seq-assoc` | The two reduce queries have the same normal form modulo the native Maude `assoc` attribute on `seq`; search succeeds |
| `if-idemp` | Both reduce queries normalize to `A`; search succeeds |

CeTA note: `seq` associativity is encoded as a Maude builtin assoc attribute, so that law bypasses the TRS gate. The remaining equations are oriented left to right in the fixture. No CeTA certificate was generated here because Maude and CeTA are not installed.

## Foo Example

Source:

```c
static int foo(int x) {
    if (x == 0)
        return -22;
    return x;
}
```

Algebra term:

```text
seq(if(eq(x, 0), return(neg(22)), skip), return(x))
```

The term is also recorded structurally in `example/foo.term.json`.

Branch-sensitive WP value:

```text
pre: true
post: result = ite(x == 0, -22, x)
effects: []
```

That value is recorded in `example/foo.expected-wp-contract.json`.

Real lifter output:

```text
pre: true
post: result = ite(x == 0, -22, x)
effects: []
```

The JSON-RPC output from `provekit-lift-c-collectors-defensive` is saved in `example/foo.lift-rpc.jsonl`, and the emitted `function-contract` declaration is saved in `example/foo.contract.json`. The collectors-defensive lifter now emits the branch-sensitive early-return contract directly.

`provekit-walk-c` was also built and run. It emits WP call-chain contracts, but this example has no internal calls, so `example/foo.walk-c-contract.json` contains only a `function-effects` declaration and no `function-contract` WP chain.

## Verification Recipe

From the repository root:

```sh
cd implementations/rust
cargo build -p provekit-cli -p provekit-ir-compiler-maude
cd ../..
make -C implementations/c/provekit-lift-c-collectors-defensive
make -C implementations/c/provekit-walk-c
./menagerie/c11-language-signature/mint.sh
implementations/rust/target/debug/provekit-ir-maude < menagerie/c11-language-signature/maude/seq_assoc.ir.json > menagerie/c11-language-signature/maude/seq_assoc.maude
implementations/rust/target/debug/provekit-ir-maude < menagerie/c11-language-signature/maude/if_idemp.ir.json > menagerie/c11-language-signature/maude/if_idemp.maude
```

If Maude is installed:

```sh
cd menagerie/c11-language-signature/maude
maude seq_assoc.maude
maude if_idemp.maude
```

The `mint.sh` script uses `--unsigned`. Current minter guardrails require a lexical `dev` path component for unsigned catalogs, so the script passes `dev/../catalog` while writing the actual catalog at `catalog/`. Production signing with the foundation v0 key is a follow-up.

## Scope

This is the core C11 signature, not exhaustive C11. Extending it with more sorts, more operators, more equations, and the full undefined behavior catalog is mint-more-mementos work, not new substrate machinery.

References:

- `protocol/specs/2026-05-09-algorithm-memento-protocol.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`
- `protocol/specs/2026-05-10-equational-portfolio-extension.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

T Savo
