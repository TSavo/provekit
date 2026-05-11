# provekit-lift-jvm-bytecode

`provekit-lift-jvm-bytecode` is the JVM bytecode lift adapter for the
`jvm-bytecode` surface. The first slice accepts deterministic Jasmin text, the
same bytecode surface emitted by `provekit-ir-compiler-jvm-bytecode`, and emits
ProofIR function-contract documents over JVM local-slot addresses.

This is intentionally a bytecode-domain lifter, not a Java source lifter.
Instruction order is the path order, locals are addressed as `localN`, and the
symbolic terms use JVM operation names such as `jvm:iadd`, `jvm:icmp_eq`, and
`jvm:invokestatic`.

## Smoke

```sh
cargo test -p provekit-lift-jvm-bytecode
```

The round-trip smoke compiles the checked C11 `foo` term through the JVM
realizer and immediately relifts the emitted Jasmin into a JVM bytecode
contract.
