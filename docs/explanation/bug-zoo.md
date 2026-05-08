# Bug Zoo: Executable Bug Species

Bug Zoo is ProvekIt's executable evidence that bug classes have portable
semantic shape.

The claim is not that TypeScript, Java, C#, Go, and Rust fail in the same way at
runtime. They do not. A TypeScript `TypeError`, a Java `NullPointerException`, a
C# `NullReferenceException`, and a cross-FFI precondition failure are different
host-language events. The claim is that each kit gives a homomorphism from its
native evidence into canonical truth. When those projections preserve the same
missing obligation, the shape CID is addressable: ProofIR for a contract
boundary, or a LinkBundle receipt for a cross-kit call edge.

For the null-boundary species, that shared shape is:

```text
maybe_null(name) => non_null(name)
```

and the canonical ProofIR boundary is:

```text
neq(name, null)
```

## The Two Steps

Each zoo specimen starts with a host-only lab, then separates discovery from
verification. The lab has no `.provekit` project; it only shows that the
ordinary host code still passes native checks.

1. **Language discovery.** The host language uses its own toolchain and kit.
   Each exhibit/fixed harness is a tiny ProvekIt project with
   `.provekit/config.toml` selecting a surface and
   `.provekit/lift/<surface>/manifest.toml` naming the native RPC lifter.
   The zoo invokes `provekit mint` for lift exhibits and `provekit link` for
   cross-kit link exhibits; the CLI resolves the surface and drives the native
   lifter or linker.
2. **Proof verification.** The normal project gate is `provekit prove`. Bug Zoo
   owns a self-contained runner under `bug-zoo/`: it receives canonical Bug Zoo
   ProofIR or LinkBundle output from the CLI result, hashes it, compares it to
   checked-in witness bytes, checks required equivalences across surfaces and
   languages, and invokes `provekit prove --formula` for scoped implication
   receipts. The exhibit signal is red when the implication is missing; the
   fixed signal is green when the paired source closes it.

In shorthand: each language proves `k_lang(I) = t`, where `k_lang` is the
language compiler as a ProvekIt kit/lifter, `I` is source, and `t` is witnessed
output: an addressable ProofIR shape CID or LinkBundle receipt CID. When
different domains land on the same shape CID, the bug has a portable signature
independent of its host-language syntax, exception type, or call boundary.
Each native surface maps through a structure-preserving homomorphism into the
correctness object; the proof layer checks whether the mapped obligation
commutes with equivalent surfaces or closes under the fixed witness.

## Current Receipts

The current zoo includes:

- `BZ-SHAPE-005`: null-boundary equivalence. Java, TypeScript, and C# carry
  different native exhibits that all lift to
  `maybe_null(name) => non_null(name)`, then route red lab-null and green
  fixed-non-null obligations through `provekit prove --formula`.
- `BZ-SHAPE-006`: value-scope escape. Java carries JUnit and Spring exhibits
  that both witness a point value and prove that `42` must not discharge a
  `>= 43` requirement.
- `BZ-SHAPE-007`: polyglot link obligation. A Go cgo caller reaches a Rust
  callee contract; `provekit link` emits the red `unprovable-obligation`
  LinkBundle when the caller witness cannot satisfy the callee precondition,
  and the fixed fixture links clean.

The null-boundary species exposes:

```text
maybe_null(name) => non_null(name)
```

and the same ProofIR CID:

```text
blake3-512:0d611d8478a205ff040e7d0bcf6c21b12051340ecc5f00c3953af632b23fc01e069b4ad8a8699869163e135b9fde85792eba6acc54cd75cb3d3cc6a40a99ded4
```

That CID is the address of the shape. The source languages disagree; the
projected boundary does not. The proof signal is also live: the lab null
witness is rejected against every lifted non-null requirement, and every fixed
surface discharges the non-null implication through the Rust CLI.

The value-scope species exposes:

```text
eq(value, 42) => gte(value, 43)
```

There the receipt is the `provekit prove --formula` result: exhibit witnesses
for 42 produce the red signal, while fixed witnesses for 43 produce the green
signal. The point is that a native value witness is useful evidence only inside
the value scope it actually observed.

The polyglot species exposes:

```text
post_caller => pre_callee
```

There the receipt is a LinkBundle rather than a ProofIR exhibit. The Go kit
reports the cgo call edge, the Rust kit reports the callee contract, and the
Rust CLI proves whether the bridge has enough evidence to discharge the callee
precondition.

## Run It

From the repository root:

```sh
cargo run --manifest-path bug-zoo/Cargo.toml -- --all
```

You can also run each discovery step directly:

```sh
pnpm exec tsx bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/tools/ts-boundary-discover.ts zod bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/typescript/exhibit/zod/harness

dotnet run --project implementations/csharp/Provekit.BugZoo/Provekit.BugZoo.csproj -- discover csharp-linq bug-zoo/species/BZ-SHAPE-005-null-boundary-equivalence/csharp/exhibit/linq-where/harness
```

Those commands show the first phase: the language compiler/kit maps source to a
witnessed bug output. The `provekit-bug-zoo` runner is the lab harness for the
second phase: proving that output lands on the expected addressable shape or
receipt CID for the specimen.

## Why This Matters

Bug Zoo turns the broad ProvekIt thesis into receipts:

- ordinary code passes ordinary host checks;
- each language's own compiler/kit maps source to a witnessed missing edge;
- canonical ProofIR and LinkBundle receipts make equivalent bug shapes
  addressable after projection;
- point witnesses stay inside the value scope they actually observed;
- fixed artifacts close the edge only if re-lift or re-link verifies the closure.

It is not a patch archive and not a benchmark of historical remediations. It is
a laboratory for the substrate claim: bug classes are tractable to universal
semantic shapes once projected below language syntax.
