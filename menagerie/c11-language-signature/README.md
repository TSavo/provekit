# C11 Language Signature Memento

C is emitted as a content-addressed algebra over contracts. Here is the algebra.

The minted C11 `LanguageSignatureMemento` is:

`blake3-512:a27e0770973e891baf139fea6e121ea14a474618738438fbb90edcc98dcd25b686e4fb0e1958e31736a00f07baaff2ef9c1d1db45cac45f0b0dfd5c0a7ddb86f`

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
| `sizeof_expr` | `Expr` | `Int` | C sizeof expression; operand is structurally present but unevaluated except for VLA semantics |
| `sizeof_type` | `Expr` | `Int` | C sizeof type form; type operand is unevaluated |
| `alignof_expr` | `Expr` | `Int` | C alignment query over expression type; operand is unevaluated |
| `alignof_type` | `Expr` | `Int` | C alignment query over type operand |
| `typeof_expr` | `Expr` | `Expr` | GNU typeof expression form; operand is type-read and unevaluated |
| `typeof_type` | `Expr` | `Expr` | GNU typeof type form |
| `offsetof` | `Expr x Expr` | `Int` | C offsetof query; type and designator are unevaluated structural operands |
| `builtin_types_compatible_p` | `Expr x Expr` | `Bool` | GNU __builtin_types_compatible_p over unevaluated type operands |
| `builtin_choose_expr` | `Bool x Expr x Expr` | `Expr` | GNU __builtin_choose_expr; controlling expression is evaluated at compile time and the selected branch is reachable |
| `generic-selection` | `Expr x ListOfExpr` | `Expr` | C11 generic selection expression |
| `stmt-expr` | `Stmt` | `Expr` | GNU statement expression payload |
| `addr-label` | `Expr` | `Expr` | GNU address-of-label expression |
| `asm-link-edge` | `Expr x Expr x Expr x Expr x Expr x Expr x Expr x Expr x ListOfExpr x ListOfExpr x ListOfExpr` | `Stmt` | inline assembly link edge; the C term names an assembly input and the linker composes it with the x86-64 lifter result |
| `div` | `Int x Int` | `Int` | integer division expression |
| `mod` | `Int x Int` | `Int` | integer remainder expression |
| `shl` | `Int x Int` | `Int` | integer left shift expression |
| `shr` | `Int x Int` | `Int` | integer right shift expression |
| `bit_and` | `Int x Int` | `Int` | integer bitwise and expression |
| `bit_or` | `Int x Int` | `Int` | integer bitwise or expression |
| `bit_xor` | `Int x Int` | `Int` | integer bitwise xor expression |
| `gt` | `Int x Int` | `Bool` | integer greater-than comparison |
| `ge` | `Int x Int` | `Bool` | integer greater-than-or-equal comparison |
| `ne` | `Int x Int` | `Bool` | integer not-equal comparison |
| `comma` | `Expr x Expr` | `Expr` | comma expression evaluates lhs then yields rhs |
| `bit_not` | `Int` | `Int` | integer bitwise complement expression |
| `addr_of` | `LValue` | `Ptr` | address-of expression yielding a pointer to target |
| `pre_inc` | `LValue` | `Expr` | prefix increment expression yielding the updated value |
| `post_inc` | `LValue` | `Expr` | postfix increment expression yielding the previous value |
| `pre_dec` | `LValue` | `Expr` | prefix decrement expression yielding the updated value |
| `post_dec` | `LValue` | `Expr` | postfix decrement expression yielding the previous value |
| `plus` | `Int` | `Int` | unary plus expression preserving value |
| `unexposed-stmt` | `ListOfStmt` | `Stmt` | libclang unexposed statement with lifted child sequence |
| `unexposed-expr` | `ListOfExpr` | `Expr` | libclang unexposed expression with lifted child sequence |
| `binary-operator` | `Expr x Expr` | `Expr` | fallback binary operator when no core C11 operator is selected |
| `unary-operator` | `Expr` | `Expr` | fallback unary operator when no core C11 operator is selected |
| `bop_add` | `Expr x Expr` | `Expr` | C binary + expression; operand evaluation order is not sequenced |
| `bop_sub` | `Expr x Expr` | `Expr` | C binary - expression with ordered operand roles |
| `bop_mul` | `Expr x Expr` | `Expr` | C binary * expression; operand evaluation order is not sequenced |
| `bop_div` | `Expr x Expr` | `Expr` | C binary / expression with ordered operand roles |
| `bop_mod` | `Expr x Expr` | `Expr` | C binary % expression with ordered operand roles |
| `bop_shl` | `Expr x Expr` | `Expr` | C binary << expression with ordered operand roles |
| `bop_shr` | `Expr x Expr` | `Expr` | C binary >> expression with ordered operand roles |
| `bop_bitand` | `Expr x Expr` | `Expr` | C binary & expression; operand evaluation order is not sequenced |
| `bop_bitor` | `Expr x Expr` | `Expr` | C binary | expression; operand evaluation order is not sequenced |
| `bop_bitxor` | `Expr x Expr` | `Expr` | C binary ^ expression; operand evaluation order is not sequenced |
| `bop_eq` | `Expr x Expr` | `Bool` | C binary == comparison; operand evaluation order is not sequenced |
| `bop_ne` | `Expr x Expr` | `Bool` | C binary != comparison; operand evaluation order is not sequenced |
| `bop_lt` | `Expr x Expr` | `Bool` | C binary < comparison with ordered operand roles |
| `bop_le` | `Expr x Expr` | `Bool` | C binary <= comparison with ordered operand roles |
| `bop_gt` | `Expr x Expr` | `Bool` | C binary > comparison with ordered operand roles |
| `bop_ge` | `Expr x Expr` | `Bool` | C binary >= comparison with ordered operand roles |
| `bop_logand` | `Bool x Bool` | `Bool` | C && short-circuit expression; left is evaluated before right |
| `bop_logor` | `Bool x Bool` | `Bool` | C || short-circuit expression; left is evaluated before right |
| `bop_comma` | `Expr x Expr` | `Expr` | C comma expression sequence-points first before second |
| `uop_neg` | `Expr` | `Expr` | C unary - expression |
| `uop_lognot` | `Expr` | `Bool` | C unary ! expression |
| `uop_deref` | `Expr` | `LValue` | C unary * dereference expression |
| `uop_bitnot` | `Expr` | `Expr` | C unary ~ expression |
| `uop_addr_of` | `LValue` | `Ptr` | C unary & address-of expression |
| `uop_pre_inc` | `LValue` | `Expr` | C prefix increment sequence-pointed update |
| `uop_post_inc` | `LValue` | `Expr` | C postfix increment sequence-pointed update |
| `uop_pre_dec` | `LValue` | `Expr` | C prefix decrement sequence-pointed update |
| `uop_post_dec` | `LValue` | `Expr` | C postfix decrement sequence-pointed update |
| `uop_plus` | `Expr` | `Expr` | C unary + expression |
| `compound_assign_add` | `LValue x Expr` | `Expr` | C compound assignment += expression; the lvalue is evaluated once and the implied combiner is bop_add |
| `compound_assign_sub` | `LValue x Expr` | `Expr` | C compound assignment -= expression; the lvalue is evaluated once and the implied combiner is bop_sub |
| `compound_assign_mul` | `LValue x Expr` | `Expr` | C compound assignment *= expression; the lvalue is evaluated once and the implied combiner is bop_mul |
| `compound_assign_div` | `LValue x Expr` | `Expr` | C compound assignment /= expression; the lvalue is evaluated once and the implied combiner is bop_div |
| `compound_assign_mod` | `LValue x Expr` | `Expr` | C compound assignment %= expression; the lvalue is evaluated once and the implied combiner is bop_mod |
| `compound_assign_shl` | `LValue x Expr` | `Expr` | C compound assignment <<= expression; the lvalue is evaluated once and the implied combiner is bop_shl |
| `compound_assign_shr` | `LValue x Expr` | `Expr` | C compound assignment >>= expression; the lvalue is evaluated once and the implied combiner is bop_shr |
| `compound_assign_bitand` | `LValue x Expr` | `Expr` | C compound assignment &= expression; the lvalue is evaluated once and the implied combiner is bop_bitand |
| `compound_assign_bitor` | `LValue x Expr` | `Expr` | C compound assignment |= expression; the lvalue is evaluated once and the implied combiner is bop_bitor |
| `compound_assign_bitxor` | `LValue x Expr` | `Expr` | C compound assignment ^= expression; the lvalue is evaluated once and the implied combiner is bop_bitxor |

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
