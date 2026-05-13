# provekit-ir-compiler-x86-64

`provekit-ir-compiler-x86-64` is the x86-64 executable target rung of the ProvekIt realizer family. It implements ORP v0.2 `compile` mode for the term stratum:

```text
compile : Term * x86-64:sysv -> ConcreteCode | Refusal
```

The input is a ProofIR term, an AST over operation CIDs, not a contract. The output is Intel syntax x86-64 SysV assembly that can be passed to `gcc` or `as`.

This is the executable-target dual of `provekit-ir-compiler-coq`: Coq realizes IR into proof source, while this crate realizes IR terms into runnable code.

## API

The existing `IrCompiler` trait in `provekit-ir-compiler` is shaped around formula compilation, so this crate provides a small `TermCompiler` trait:

```rust
pub trait TermCompiler {
    fn compile_term_json(&self, ir: &serde_json::Value) -> Result<String, CompileError>;
}
```

`X8664Compiler` also implements `IrCompiler` for registry compatibility. In that path, the emitted assembly is returned as `CompiledFormula.body`, with an empty preamble and no free variables.

The binary is `provekit-ir-x86-64`:

```sh
cat tests/fixtures/foo.term.json | provekit-ir-x86-64 > foo.s
gcc harness.c foo.s -o harness
```

## Core Subset

The current hand-mapped subset covers:

```text
seq, if, while, return, call, break, continue, skip,
eq, lt, le, add, sub, mul, neg, and, or, not,
variable reference, integer literal, deref, assign
```

The mapping is intentionally direct:

| ProofIR operation | x86-64 realization |
| --- | --- |
| `seq(a, b)` | Emit `a` followed by `b`. |
| `if(c, a, b)` | Compile `c` to `eax`, `cmp eax, 0`, branch with `je`. |
| `while(c, b)` | Loop head label, condition compare, exit branch, body, back edge. |
| `return(e)` | Compile `e` to `eax`, then `ret`. |
| `call(f, args)` | SysV integer argument registers, stack alignment, direct `call`. |
| `break` | Jump to the innermost loop exit label. |
| `continue` | Jump to the innermost loop head label. |
| `skip` | Emit no instruction. |
| `eq`, `lt`, `le` | `cmp` plus `sete`, `setl`, or `setle`, then `movzx`. |
| `add`, `sub`, `mul`, `neg` | `add`, `sub`, `imul`, `neg`. |
| `and`, `or`, `not` | Normalize to zero or one, then `and`, `or`, or `sete`. |
| `var`, `const` | First-occurrence SysV argument register or integer immediate. |
| `deref` | Load `DWORD PTR [address]`. |
| `assign` | Store into a variable register or `DWORD PTR [address]`. |

The core subset is hand mapped. The full operation set is mint-more-realizations, not new machinery.

## Refusal Behavior

Unsupported operations return a refusal through `CompileError`; the compiler does not emit partial assembly or an opaque placeholder. Non-integer constants, too many integer parameters for the current core ABI slice, unsupported call targets, unsupported assignment targets, and nontrivial loop invariant payloads are refused.

`while` lowers only when the term has no invariant payload or carries the unit shape. A real invariant memento needs a discharge path before it can be compiled without silently dropping an obligation.

## Byte Determinism

Output is byte deterministic for byte-equal input JSON and the same target descriptor. Labels are allocated in traversal order. Variables are assigned to SysV integer argument registers by first occurrence order. The `tests/byte_for_byte.rs` test keeps this invariant explicit.

## Round Trip Story

ORP v0.2 treats `compile` mode as a deterministic homomorphism from the term algebra to target code. In LSP terms, this crate is the implementation body for a draft `LanguageMorphismMemento`:

```text
ProofIR C11 term algebra -> x86-64:sysv assembly
```

The draft spec is in `specs/algebra_to_x86_64.spec.json`.

The intended theorem is:

```text
contract(lift_x86_64(compile_x86_64(t))) = contract(t)
```

When composed with a C lifter, this gives the round trip:

```text
lift C function -> ProofIR term -> compile to x86-64 -> lift x86-64
```

The lifted result should carry the same contract, modulo accepted representation morphisms. Comparing `lift(native-compiler-x86)` with `lift(compile-to-x86)` is also a differential miscompile detector: disagreement points to either a compiler bug, a lifter bug, or a missing morphism discharge.

This is the ORP v0.2 term-stratum story from `protocol/specs/2026-05-10-realizer-protocol-v2.md`, aligned with papers 13 and 14 on languages as content-addressed algebras and portable correctness bundles.

## Verification

From `implementations/rust`:

```sh
cargo build -p provekit-ir-compiler-x86-64
cargo test -p provekit-ir-compiler-x86-64
cargo clippy -p provekit-ir-compiler-x86-64 -- -D warnings
```

The smoke fixture lowers `tests/fixtures/foo.term.json` to `tests/fixtures/foo.expected.s`. The ignored test assembles that assembly with `gcc` and runs a harness that checks `foo(0) == -22` and `foo(42) == 42`.

## Follow Up

The next steps are the full C11 operation set, a real register allocator, a stack-frame model for local variables, richer type widths, optimized instruction selection, minted morphism receipts, and automatic realization table generation from accepted operation mementos.

T Savo
