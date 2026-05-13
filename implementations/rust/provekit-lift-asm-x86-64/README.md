# provekit-lift-asm-x86-64

`provekit-lift-asm-x86-64` is the machine-code rung of the ProvekIt lifter family. It speaks the `pep/1.7.0` JSON-RPC protocol on stdin and stdout, accepts `.s` assembly files and ELF `.o` object files, disassembles x86-64, recovers a small structured control-flow term, propagates weakest-precondition style path facts, and emits `FunctionContractMemento` declarations.

The C lifters work over typed lvalues and AST nodes. This lifter works one level lower: the AST is a flat instruction stream plus a CFG, and the carrier is registers, RFLAGS, and memory.

## Disassembler choice

This crate shells out to LLVM `objdump` with Intel syntax. For `.s` inputs it first invokes `clang -target x86_64-linux-gnu -c -x assembler -m64` and then disassembles the resulting ELF object. This keeps the lifter off raw byte parsing for variable-width x86 instructions.

`iced-x86` is the preferred pure-Rust follow-up because it has full decoder coverage and avoids toolchain dependence. This worktree did not have the crate cached, and network access is restricted, so using the system disassembler was the reliable path for this slice.

## Core subset

The current hand-written semantics table covers:

`mov`, `movzx`, `movsx`, `lea`, `add`, `sub`, `inc`, `dec`, `and`, `or`, `xor`, `shl`, `shr`, `sar`, `cmp`, `test`, `push`, `pop`, `call`, `ret`, `leave`, `jmp`, `je`, `jne`, `jl`, `jle`, `jg`, `jge`, `jz`, `jnz`, `js`, `jns`, `nop`.

The core subset is hand-written. The full x86-64 semantics should be auto-generated from a formal spec, such as sail-x86, the K Framework k-x86 semantics, or the Stanford x86sem corpus. That follow-up should mint more semantics, not new machinery. `iced-x86` can provide the decoder base for that path, and VEX or pyvex can provide a richer semantic comparison oracle if wired in later.

Each instruction entry in `src/lib.rs` has a short reference comment naming the Intel SDM instruction and the intended future formal source.

## Control flow

The lifter builds a CFG from objdump addresses. It handles the non-optimized patterns used by the smoke fixture:

- forward conditional branch over a block as `if`
- direct `jmp` as a structured jump when the target remains inside the function
- `call` as a call effect
- `ret` as return
- `leave` as stack-frame teardown

Backward branches are recognized as loops, but this slice refuses them unless a loop invariant memento is available. Indirect `jmp` and indirect `call` are refused because the target set is not known. Irreducible control flow is also refused.

## Algebra slot

The draft language signature is `x86-64:sysv`. It shares the flow-control sub-algebra with C11 and aarch64: `seq`, `if`, `while`, `switch`, `call`, `return`, `break`, `continue`, and `skip` have the same shape and should dedupe by CID once minted. `jne` lowers to the same control operation as C `if` and ARM `b.ne`; what differs is the ISA primitive operation that updates or reads the condition state.

The payoff is the C-to-x86-64 morphism: compiler correctness becomes a discharged homomorphism between the C11 signature and this machine signature.

## Verification

From `implementations/rust`:

```sh
cargo build -p provekit-lift-asm-x86-64
cargo test -p provekit-lift-asm-x86-64
cargo clippy -p provekit-lift-asm-x86-64 -- -D warnings
```

The smoke fixture is `tests/fixtures/foo.s`. It lifts:

```asm
foo:
    test    edi, edi
    jne     .Lret
    mov     eax, -22
    ret
.Lret:
    mov     eax, edi
    ret
```

Expected contract shape: `edi == 0` implies `eax_post == 0xffffffea`, and `edi != 0` implies `eax_post == edi`. The function has no memory, IO, or trap effects.

References:

- `docs/explanation/c-lifter-family.md`
- `docs/superpowers/specs/2026-05-08-c-lifter-family-design.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

T Savo
