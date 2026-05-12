# provekit-lift-asm-aarch64

`provekit-lift-asm-aarch64` is the machine-code rung of the ProvekIt lifter family. It speaks the `pep/1.7.0` JSON-RPC protocol over stdin and stdout with `--rpc`, reads AArch64 assembly or object files, recovers a small control-flow term, propagates path weakest preconditions, and emits `FunctionContractMemento` bodies.

The C lifter family works over typed C lvalues and AST walkers. This lifter is one level lower: the input is a flat instruction stream plus labels and branch edges, and the semantic carrier is registers, NZCV flags, and memory.

## Disassembly Choice

The crate parses `.s`, `.S`, and `.asm` files directly. For object input it shells out to `objdump -d` and parses the resulting instruction stream. I did not add `yaxpeax-arm` or `capstone` because neither is already present in the workspace lockfile, and this worktree runs with network access restricted. The direct parser covers the checked-in smoke test and ordinary compiler assembly; the `objdump` path is a conservative object-file fallback.

## Core Instruction Subset

The hand-written semantics table covers:

- Moves: `mov`, `movz`, `movk`
- Arithmetic: `add`, `adds`, `sub`, `subs`, `cmp`, `cmn`
- Logic: `and`, `orr`, `eor`
- Shifts: `lsl`, `lsr`, `asr`
- Memory: `ldr`, `ldrb`, `str`, `strb`
- Branches and calls: `b`, `bl`, `br`, `blr`, `ret`
- Conditional branches: `cbz`, `cbnz`, `tbz`, `tbnz`
- Flag branches: `b.eq`, `b.ne`, `b.lt`, `b.ge`, `b.gt`, `b.le`, `b.cs`, `b.hs`, `b.cc`, `b.lo`, `b.mi`, `b.pl`, `b.vs`, `b.vc`, `b.al`
- Traps: `brk`, `svc`

Each instruction maps to a symbolic state transformer. For example, `adds Xd, Xn, Xm` emits `Xd_out = bvadd64(Xn, Xm)`, updates `N`, `Z`, `C`, and `V`, and preserves wrap behavior through named bitvector constructors. `ldr` emits read-validity and alignment preconditions plus a memory-read effect. `str` emits write-validity and alignment preconditions plus a memory-write effect.

The core subset is hand-written. The full AArch64 semantics should be generated from ARM's official ASL sources, using Machine Readable Architecture, `mra_tools`, `asli`, or `sail-arm`. That follow-up mints more semantics; it should not require new lifter machinery.

## Control Flow

The lifter structures common compiler patterns:

- Forward conditional branch over a block as `if`
- Direct `b` to a forward label as a structured jump within the function
- `bl` as a call effect
- `ret` as a return path

Backward branches are recognized as loops. This first slice records a refusal when a loop body needs an invariant memento, instead of inventing one. Computed `br` and `blr` are refused because they require indirect branch target recovery.

## Algebra Placement

The `aarch64` language signature shares the flow-control sub-algebra with C11. The operations `seq`, `if`, `while`, `switch`, `call`, `return`, `break`, `continue`, and `skip` have the same shape in the draft signature spec. `cbz` and `b.ne` therefore recover the same operation as C's `if`; the difference is only the carrier and primitive operations.

The payoff is the C-to-AArch64 morphism. Lift the C contract, lift the compiled AArch64 contract, and the homomorphism obligation is compiler correctness for that slice.

## Verification

From `implementations/rust`:

```sh
cargo build -p provekit-lift-asm-aarch64
cargo test -p provekit-lift-asm-aarch64
cargo clippy -p provekit-lift-asm-aarch64 -- -D warnings
```

Smoke fixture:

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"lift","params":{"workspace_root":"provekit-lift-asm-aarch64","source_paths":["tests/fixtures/foo.s"],"surface":"asm-aarch64","options":{"layer":"all"}}}' | cargo run -q -p provekit-lift-asm-aarch64 -- --rpc
```

The smoke contract for `foo` has postcondition shape:

```text
w0_out = ite(w0 == 0, -22, w0)
```

## References

- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`

T Savo
