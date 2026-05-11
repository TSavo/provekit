# provekit-ir-compiler-jvm-bytecode

`provekit-ir-compiler-jvm-bytecode` is an ORP v0.2 `compile` mode realizer for
the ProofIR term stratum. It is an executable-target rung of the ProvekIt
realizer family and is dual to `provekit-ir-compiler-coq`: Coq lowers into a
prover-facing artifact, while this crate lowers ProofIR terms into runnable JVM
bytecode.

The emitted artifact is deterministic Jasmin text. Jasmin is used as the stable
surface for JVM bytecode in this crate. A future backend can replace it with a
direct `.class` writer without changing the algebra mapping.
`provekit-lift-jvm-bytecode` consumes this same Jasmin surface for the current
realizer-to-lifter smoke.

The JVM is a stack machine, so the realization homomorphism encodes the usual
push-operands-then-op convention: lower each operand left to right, then emit
the target instruction or branch shape.

## CLI

```sh
cat ../../menagerie/c11-language-signature/example/foo.term.json | cargo run -p provekit-ir-compiler-jvm-bytecode --bin provekit-ir-jvm-bytecode
```

The binary reads a ProofIR term JSON document from stdin and writes a Jasmin
`.j` source to stdout.

## Core Subset

This crate hand-maps the current core subset:

| ProofIR operation | JVM Jasmin realization |
| --- | --- |
| `seq(a,b,...)` | emit each statement in order |
| `if(c,t,e)` statement | `c`, `ifeq`, `t`, optional `goto`, `e`, end label |
| `if(c,t,e)` expression | `c`, `ifeq`, expression branch, `goto`, expression branch |
| `while(c,b)` | loop label, `c`, `ifeq`, `b`, `goto` loop label |
| `return(e)` | `e`, `ireturn` |
| `call(f,args...)` | args left to right, `invokestatic Class/f(II...)I` |
| `break`, `continue` | `goto` to the active loop labels |
| `skip` | no instructions |
| variable | `iload` from a deterministic local slot |
| integer or bool literal | `iconst`, `bipush`, `sipush`, or `ldc` |
| `eq`, `lt`, `le` | `if_icmpeq`, `if_icmplt`, `if_icmple`, normalized to 0 or 1 |
| `add`, `sub`, `mul`, `neg` | `iadd`, `isub`, `imul`, `ineg` |
| `and`, `or`, `not` | branch shapes normalized to 0 or 1 |
| `deref(p)` | `getstatic Class/memory [I`, `p`, `iaload` |
| `assign(x,v)` | `v`, `istore` for a local variable |
| `assign(p,v)` | `getstatic Class/memory [I`, `p`, `v`, `iastore` |

The core subset is hand-mapped. The full operation set is
mint-more-realizations, not new machinery: add an operation-CID table entry,
its lowering, and the corresponding morphism discharge receipt.

## Determinism

Output is byte-deterministic. Parameter slots and local slots are assigned in
sorted order, labels are minted monotonically, class and method names are
derived deterministically, and `.limit stack` is computed during emission.

The `byte_for_byte` test compiles the `foo` term twice and checks both outputs
against `tests/fixtures/foo.expected.j`.

## ORP v0.2 Role

ORP v0.2 defines:

```text
compile : Term * Target -> ConcreteCode | Refusal
```

This crate implements that mode for target `jvm-bytecode` through the Jasmin
surface syntax. It compiles only terms, not contracts. The term is already the
implementation at the operation-CID level, so the compiler is a deterministic
algebra-to-target morphism rather than synthesis from a boundary obligation.

The round-trip story follows the protocol and papers 13 and 14:

1. Lift any source language into a ProofIR term and contract projection.
2. Compile the term to JVM bytecode through this realizer.
3. Lift the emitted JVM bytecode back to ProofIR with `provekit-lift-jvm-bytecode`.
4. Compare the projected contracts through accepted receipts.

When the source lifter and this compile realizer are discharged
homomorphisms, their composition preserves the contract. The same lifted term
can also be compiled by `javac` from source and cross-checked at the lifted
contract level.

References:

- `protocol/specs/2026-05-10-realizer-protocol-v2.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

## Refusal Behavior

Unsupported operations fail closed with `CompileError::UnsupportedPredicate`.
Malformed term shapes fail with `CompileError::MalformedIr`. Unsupported
constant sorts fail with `CompileError::UnsupportedSort`. The compiler does not
emit opaque placeholders and does not silently reinterpret operations outside
the mapped subset.

## Smoke Test

The non-ignored smoke test compiles `tests/fixtures/foo.term.json` to the
checked-in Jasmin fixture. An ignored test can assemble and run the result when
`jasmin` or `krak2`, `javac`, and `java` are installed. It checks:

```text
foo(0) = -22
foo(42) = 42
```

If the external toolchain is unavailable, the ignored test self-skips.

## Follow-up Work

- Add mappings for the full C11 operation catalog.
- Replace the static int-memory model with a typed JVM reference model.
- Add a deterministic direct `.class` writer and constant-pool builder.
- Track reference types and object layouts instead of only the i32 core.
- Add optimization passes once the morphism receipt covers them.
- Mint the `LanguageMorphismMemento` and attach a
  `MorphismDischargeReceipt`.

Sign-off: T Savo
