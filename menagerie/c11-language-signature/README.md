# C11 Language Signature Memento

C is emitted as a content-addressed algebra over contracts. Here is the algebra.

The minted C11 `LanguageSignatureMemento` is:

`blake3-512:c942ba70e4b701e139a46590116f5cdc16ab41db277e80e54c01f23e4a7cf6241d4431c60473409cb3f0b61ce27f593071c1c224291f04c61aa04b4773764945`

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
| `skip` | `Unit` | `Stmt` | Identity statement, unchanged state |
| `seq` | `Stmt x Stmt` | `Stmt` | WP composition |
| `if` | `Bool x Stmt x Stmt` | `Stmt` | Branch-selected WP |
| `while` | `Bool x Stmt` | `Stmt` | Loop invariant plus false condition at exit |
| `for` | `Stmt x Bool x Stmt x Stmt` | `Stmt` | Desugars to init plus while |
| `switch` | `Int x ListOf<Stmt>` | `Stmt` | Case-dispatched WP join |
| `call` | `FnContract x ListOf<Expr>` | `Stmt` | CCP composition at call site |
| `return` | `Expr` | `Stmt` | Bind output and exit |
| `break` | `Unit` | `Stmt` | Exit enclosing switch or loop |
| `continue` | `Unit` | `Stmt` | Jump to next loop iteration |
| `deref` | `Ptr` | `LValue` | Requires non-null valid pointer, effect `MemRead` |
| `member` | `LValue x FieldName` | `LValue` | Field projection |
| `add` | `Int x Int` | `Int` | No signed overflow, otherwise `Trap` |
| `sub` | `Int x Int` | `Int` | No signed overflow, otherwise `Trap` |
| `mul` | `Int x Int` | `Int` | No signed overflow, otherwise `Trap` |
| `eq` | `Int x Int` | `Bool` | Integer equality |
| `lt` | `Int x Int` | `Bool` | Integer less than |
| `le` | `Int x Int` | `Bool` | Integer less than or equal |
| `and` | `Bool x Bool` | `Bool` | Short-circuit conjunction |
| `or` | `Bool x Bool` | `Bool` | Short-circuit disjunction |
| `not` | `Bool` | `Bool` | Boolean negation |
| `assign` | `LValue x Expr` | `Stmt` | Store, effect `MemWrite` |
| `neg` | `Int` | `Int` | No signed overflow, otherwise `Trap` |

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
