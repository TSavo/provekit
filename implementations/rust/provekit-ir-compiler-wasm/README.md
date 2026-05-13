# provekit-ir-compiler-wasm

`provekit-ir-compiler-wasm` is an ORP v0.2 `compile`-mode realizer for the
ProofIR term stratum. It is the executable-target dual of
`provekit-ir-compiler-coq`: Coq emits prover input for contracts, while this
crate emits runnable WebAssembly text for terms.

The target is WebAssembly WAT. WebAssembly is a stack machine, so the
realization homomorphism uses the usual push-operands-then-op convention:
compile each argument left to right, then emit the target instruction.

## CLI

```sh
cat ../../menagerie/c11-language-signature/example/foo.term.json | cargo run -p provekit-ir-compiler-wasm --bin provekit-ir-wasm
```

The binary reads a ProofIR term JSON document from stdin and writes a WAT
module to stdout.

## Core Subset

This crate hand-maps the current core subset:

| ProofIR operation | WAT realization |
| --- | --- |
| `seq(a,b,...)` | emit each statement in order, dropping expression values in statement position |
| `if(c,t,e)` statement | `c`, `if`, `t`, `else`, `e`, `end` |
| `if(c,t,e)` expression | `c`, `if (result i32)`, `t`, `else`, `e`, `end` |
| `while(c,b)` | `block`, `loop`, `c`, `i32.eqz`, `br_if`, `b`, `br`, `end`, `end` |
| `return(e)` | `e`, `return` |
| `call(f,args...)` | args left to right, `call $f` |
| `break`, `continue` | `br` or `br_if` to the active loop labels |
| `skip` | no instructions |
| variable | `local.get` |
| integer or bool literal | `i32.const` |
| `eq`, `lt`, `le` | `i32.eq`, `i32.lt_s`, `i32.le_s` |
| `add`, `sub`, `mul`, `neg` | `i32.add`, `i32.sub`, `i32.mul`, `0 x i32.sub` |
| `and`, `or`, `not` | `i32.and`, `i32.or`, `i32.eqz` |
| `deref(p)` | `p`, `i32.load` with exported memory |
| `assign(x,v)` | `v`, `local.set $x` for local variables |
| `assign(p,v)` | `p`, `v`, `i32.store` otherwise |

The core subset is hand-mapped. The full operation set is
mint-more-realizations, not new machinery: add an operation-CID table entry,
its lowering, and the corresponding morphism discharge receipt.

## Determinism

WAT output is byte-deterministic. Function parameters, local declarations, and
module sections are emitted in sorted, stable order. The `byte_for_byte` test
compiles the `foo` term twice and checks both outputs against
`fixtures/foo.wat`.

## ORP v0.2 Role

ORP v0.2 defines:

```text
compile : Term * Target -> ConcreteCode | Refusal
```

This crate implements that mode for target `wasm-wat`. It compiles only terms,
not contracts. The term is already the implementation at the operation-CID
level, so the compiler is a deterministic algebra-to-target morphism rather
than synthesis from a boundary obligation.

The round-trip story follows the protocol and papers 13 and 14:

1. Lift source code into a ProofIR term and contract projection.
2. Compile the term to WASM through this realizer.
3. Lift the emitted WASM back to ProofIR with a WASM lifter.
4. Compare the projected contracts through the accepted receipts.

When both lifter and compiler are discharged homomorphisms, their composition
preserves the contract. The shipped WASM can also be cross-checked against the
WASM emitted by a native toolchain for the same source.

References:

- `protocol/specs/2026-05-10-realizer-protocol-v2.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

## Refusal Behavior

Unsupported operations fail closed with a compile error. The compiler does not
emit opaque placeholders and does not silently reinterpret operations outside
the mapped subset.

## Follow-up Work

- Add mappings for the full C11 operation catalog.
- Carry integer width through variables and operations for complete i64 typed
  lowering.
- Expand the memory model beyond a single exported MVP memory.
- Add component model exports and imports.
- Mint the draft `LanguageMorphismMemento` once the proof receipts are ready.

Sign-off: T Savo
