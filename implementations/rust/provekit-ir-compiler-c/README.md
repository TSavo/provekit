# provekit-ir-compiler-c

`provekit-ir-compiler-c` is the C executable-target rung of the ProvekIt
realizer family. It implements ORP v0.2 `compile` mode for the ProofIR term
stratum:

```text
compile : Term * c:c11 -> ConcreteCode | Refusal
```

The input is a ProofIR term, an AST over operation CIDs, not a contract. The
output is complete C11 source. C is structured enough that the realization
homomorphism is nearly transparent: most term operations lower directly to the
matching C expression or statement form.

This is dual to `provekit-ir-compiler-coq`: Coq realizes IR into prover input,
while this crate realizes IR terms into runnable C.

## CLI

```sh
cat ../../menagerie/c11-language-signature/example/foo.term.json | cargo run -p provekit-ir-compiler-c --bin provekit-ir-c
```

The binary reads a ProofIR term JSON document from stdin and writes C11 source
to stdout.

## API

The crate exposes `compile_c`, `CCompiler`, and a small `TermCompiler` trait:

```rust
pub trait TermCompiler {
    fn compile_term_json(&self, ir: &serde_json::Value) -> Result<String, CompileError>;
}
```

`CCompiler` also implements `IrCompiler` for registry compatibility. In that
path, the emitted C source is returned as `CompiledFormula.body`, with an empty
preamble and no free variables.

## Core Subset

The current hand-mapped subset covers:

```text
seq, if, while, return, call, break, continue, skip,
eq, lt, le, add, sub, mul, neg, and, or, not,
variable reference, integer literal, deref, assign
```

| ProofIR operation | C11 realization |
| --- | --- |
| `seq(a,b)` | Emit `a` followed by `b`. |
| `if(c,t,e)` statement | `if c { t } else { e }`. |
| `if(c,t,e)` expression | `(c) ? (t) : (e)`. |
| `while(c,b)` | `while c { b }`. |
| `return(e)` | `return (e);`. |
| `call(f,args)` | `f(args)`. |
| `break`, `continue` | `break;` and `continue;` in a loop. |
| `skip` | `;`. |
| variable reference | The validated C identifier. |
| integer or bool literal | The literal, with bools encoded as `0` or `1`. |
| `eq`, `lt`, `le` | `==`, `<`, `<=`. |
| `add`, `sub`, `mul`, `neg` | `+`, `-`, `*`, unary `-`. |
| `and`, `or`, `not` | `&&`, `||`, `!`. |
| `deref(p)` | `(*(p))`. |
| `assign(lv,e)` | `lv = (e);` as a statement, `(lv = (e))` as an expression. |

The core subset is hand-mapped. The full operation set is
mint-more-realizations, not new machinery: add an operation-CID table entry,
its lowering, and the corresponding morphism discharge receipt.

## Byte Determinism

Output is byte-deterministic for byte-equal input JSON and the same target
descriptor. Function names are derived deterministically from the envelope or
source file stem. Parameters follow first read order. Labels are not needed for
the C structured subset. The `byte_for_byte` test compiles fixtures twice and
checks exact byte equality; the smoke test checks `foo.term.json` against a
checked-in C source fixture.

## Round Trip Story

ORP v0.2 treats `compile` mode as a deterministic homomorphism from the term
algebra to target code. In LSP terms, this crate is the implementation body for
a draft `LanguageMorphismMemento`:

```text
ProofIR C11 term algebra -> c:c11 source
```

The draft spec is in `specs/algebra_to_c.spec.json`.

The intended C round trip is:

```text
contract(lift_from_c(compile_to_c(t))) = contract(t)
```

Because target C and the source term algebra share the C11 signature, the
representation morphism is the identity for the current contract projection.
Composing this realizer with the existing `c-collectors-defensive` lifter gives
the clean sanity check: `c -> algebra -> c` should preserve the contract CID.

The same composition also explains cross-language routes. For example,
`lift-from-rust` followed by `compile-to-c` is cross-compiling Rust to C at the
ProofIR term stratum while carrying the same contract through accepted receipts.

References:

- `protocol/specs/2026-05-10-realizer-protocol-v2.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

## Refusal Behavior

Unsupported operations fail closed with `CompileError::UnsupportedPredicate`.
Malformed terms fail with `CompileError::MalformedIr`. Unsupported constant
sorts fail with `CompileError::UnsupportedSort`. The compiler does not emit
opaque placeholders, does not silently drop invariants, and refuses unsafe C
identifiers instead of rewriting term variables.

## Follow-up Work

- Add mappings for the full C11 operation catalog.
- Carry declarations and type widths through the term envelope.
- Add richer ABI bindings for multiple functions and external calls.
- Mint the `algebra -> c` morphism receipt.
- Mint the round-trip identity receipt for `lift-from-c` composed with
  `compile-to-c`.

Sign-off: T Savo
