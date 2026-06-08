# Sugar
**The Correctness Suite**

> **Sugar in, `.proof` out.**

Sugar is a proof supply chain for software that already exists. It is
Git/IPFS/Sigstore for *correctness*: signed, content-addressed `.proof`
artifacts that anyone can re-verify by recomputation, without trusting the
original test runner and without re-running the original proof.

It does not ask anyone to rewrite code in a proof language. Feed it the ordinary
surface you already write — the *sugar*: tests, assertions, contracts, schemas,
validators, framework annotations, boundary and library bindings — and it turns
that evidence into portable claims, packaged as signed, content-addressed
`.proof` artifacts that other packages, tools, and languages can verify by
recomputation. "Just sugar," the surface everyone waves off as not-the-real-thing,
is exactly where correctness turns out to live. CIDs are BLAKE3-512; signatures
are Ed25519.

## The claim Sugar makes checkable

Correctness is `k(I) = t`: a program `k` applied to an input/precondition `I`
produces an output/postcondition `t`. A `.proof` is a kept promise made
checkable. The program, the input, and the result are each content-addressed
and pinned, so the claim names exactly what it is about and cannot be silently
rebound. `sugar verify` lets a stranger recompute the guarantee.

A `.proof` is discharged **two independent ways, and both must agree**:

- **CONSISTENCY.** A solver (z3) proves the lifted contract is internally
  satisfiable. A self-contradictory spec is refused without running anything.
- **WITNESS.** The code is actually run, the run is content-addressed, and
  `verify` recomputes it. A witness that does not reproduce its pinned CID is
  refused.

The numpy showcase (`examples/numpy-showcase/`) discharges both on a real
library operation. `sugar prove` reports `discharged: 2`: one obligation
"test assertions mutually consistent ... [solver 'z3' returned sat]", one
"witnessed by recompute (kit): re-ran on pinned code; assertions held; witness
CID reproduced." A `np.add(2,3)` asserted both `== 5` and `== 6` is refused
both ways: z3 finds the conjunction UNSAT, and the actual run yields `5`, so the
`== 6` witness fails.

## First principle: supra omnia, rectum (above all, correctness)

Every operation is exact, or loudly-bounded-lossy, or it refuses. Verification
*is* recomputation: trust nothing, not even the kit that produced the proof.
Where a lift, lower, or solver discharge cannot be exact, the loss is recorded,
content-addressed, and named, never silent. "Loudly-bounded-lossy" is only
honest if the bound is written down.

## The oracle trio: a `.proof` carries identity, not bodies

A `.proof` does not embed source code or test logs. It carries IDENTITY: CIDs,
loci, and signatures. Bodies are resolved on demand and recompute-verified.
This is what lets a 2909-function numpy `.proof` stay 13M instead of shipping
all of numpy inside it.

- **Source Oracle.** Given a locus plus a CID, return the on-disk source iff it
  recomputes to that CID, else refuse loudly. `recognize` and `materialize`
  both feed source through this one oracle.
  (`implementations/python/sugar-lift-python-source/src/sugar_lift_python_source/source_oracle.py`.)
- **Witness Oracle.** A witness is arbitrary signed content: a test run, a CI
  log, a poem. The kit oracle (python/java) is UNTRUSTED. Over RPC it only
  RESOLVES the witness body. The Rust CLI BLAKE3's the body itself and compares
  to the pinned CID. A wrong body for a CID (broken oracle) is distinguished
  from an honest re-run that differs (drift).
  (`implementations/rust/sugar-cli/src/witness_verify.rs`,
  `implementations/python/sugar-lift-py-tests/src/sugar_lift_py_tests/witness_oracle.py`.)
- A contract is already a CID. So the `.proof` is pure correctness identity:
  source, witness, and contract are all resolved-and-recomputed, never trusted.

## Ship a `.proof` for a whole library, no shim

`examples/numpy-vendor/` is the headline demo. The universal lifter reads
numpy's installed python source and sugar-lifts **every module-level function**
(the symbol is its qualified path, e.g. `lib._function_base_impl.rot90`) into
one lean `.proof`. On numpy 2.4.6 that is **2909 sugar members in a 13M
`.proof`, ~16s, with no code changes to numpy and no hand-written shim.** The
count is whatever the lifter finds in the installed numpy, so it tracks the
numpy version. `numpy.add` is a C ufunc with no python body, so it is simply not
among the python functions lifted; the thousands that are python all lift.

The vendor ships the `.proof` plus a separately deployed witness package
(`<cid>.witness`, the audit material). A consumer runs `sugar verify`, which
recomputes everything:

```
numpy.proof:  13M, 2909 sugar members
witness: passed blake3-512:049e169f... -> .sugar/witnesses/<cid>.witness
oracle resolved via package; rust recomputed the CID and it matched
pass
```

Run it with `examples/numpy-vendor/run.sh` (builds a venv on first run).

## The capstone: correctness is inheritable, and composition failures are caught

The important failure mode is composition. A package can pass its tests, another
package can pass its tests, and the assembled system can still contain
contradictory behavioral claims. This is no longer aspirational. There is a
working demo of catching exactly that.

A numpy vendor mints a `.proof` carrying the callsite-keyed contract
`np.add(2,3) == 5`. A consumer stages that `.proof` in `.sugar/imports/`,
asserts something about the same call, and runs `prove`:

- A consumer asserting `np.add(2,3) == 6` is **REFUSED**. It inherits numpy's
  `== 5`; z3 finds `and(== 5, == 6)` UNSAT.
- A consumer asserting `np.add(2,3) == 5` is **PROVEN**.

This works because contracts key to the CALLSITE under test (e.g.
`numpy.add#euf#...::assertion`), not to the test, and the verifier conjoins
same-named contracts across proofs before the SAT check. The consumer inherits
the vendor's correctness and is caught contradicting it.

Verified end to end:
`implementations/python/sugar-lift-py-tests/tests/test_inheritance_e2e.py`
(parametrized: `consumer-agrees-PROVEN`, `consumer-contradicts-REFUSED`) and the
unit test `cross_proof_same_named_contracts_are_conjoined` in
`implementations/rust/sugar-verifier/src/consistency.rs`.

## The shape: kits own language, the CLI owns proof

Sugar has two deliberately separate responsibilities.

**Kits own language reality.** A kit knows the language, package manager,
compiler, test framework, annotation library, and ecosystem conventions. A Rust
kit can walk Rust tests and contracts. A Java kit can read Bean Validation, JML,
Spring, JUnit, and Maven-shaped package data. A TypeScript kit can read Zod,
class-validator, fast-check, and npm-shaped package data. Kits **lift** native
evidence into ProofIR, **lower** admitted claims back into native source when a
workflow calls for it (the contract-facet of lower is an emitter, the
sugar-facet is a materializer), and resolve dependency `.proof` artifacts
through the package manager and filesystem rules of their ecosystem. Kits never
verify: they only resolve bodies over RPC.

**The CLI owns proof computation over normalized data.** The `sugar` CLI is
language-agnostic. It loads `.proof` artifacts, speaks RPC to configured kits,
normalizes claims, composes implications, conjoins same-named contracts, runs
the solver, recomputes witnesses, and reports discharge status. The CLI does not
know what a Maven classifier, npm workspace, Rust proc macro, or Spring
annotation means. The kit translates those surfaces to proof data; the CLI
computes over the proof data.

That boundary is the product. Sugar is not "a better Rust verifier" or "a
new test runner." It is the place where language-native evidence becomes a
portable, recomputable claim that survives package boundaries. For the canonical
definitions of sugar, boundary, lift, lower, concept, and contract, see
[SHARED-LANGUAGE.md](SHARED-LANGUAGE.md).

## The `.proof` DAG

A `.proof` artifact is a signed, content-addressed bundle of proof data. It can
contain contract mementos, source mementos, witness mementos, implication
witnesses, bridge attestations, materialization receipts, package inspection
claims, and policy-relevant metadata.

The graph is a DAG because every claim names the exact content it depends on:
contract CIDs, source CIDs, witness CIDs, attestation CIDs, contract-set CIDs,
proof-bundle CIDs, binary CIDs, and protocol catalog CIDs. Old facts remain true
about the old bytes that minted them. When code or claims change, new CIDs
appear. Nothing needs a central invalidation service; unchanged commitments
remain checkable by content identity.

```text
native evidence -> kit lift -> ProofIR / protocol claim
               -> signed memento -> .proof DAG -> sugar prove / verify
```

Previously minted, unchanged commitments can often be checked cheaply by CID
equality, signature checks, and graph walking. Semantic proving still happens
when claims are minted, changed, or newly composed in a way the DAG does not
already justify. Sugar does not make all proving constant-time. It amortizes
expensive proof work by making prior commitments content-addressed and reusable.

## What the CLI does

> _Naming: the project is **Sugar**. The Rust crates (`sugar-*`) and the CLI
> binary (`sugar`) have been renamed. The proofchain identity layer — kit ids,
> wire tokens, and `.proof` producer strings — still carries the `sugar`
> name, frozen on purpose: it is content-addressed, so re-minting it under the
> `sugar` name is a separate, deliberate swing. The dependency graph is sugar;
> the CID identity is still sugar. Names are sugar, CID is identity._

The canonical CLI is the Rust `sugar` binary. Run `sugar --help` for the
authoritative list; the current subcommands include:

- `sugar mint`: dispatch the configured lift plugins and write `.proof`
  artifacts. This is the verb that actually drives lifting in every example
  here.
- `sugar prove`: run the six-stage verifier. Load proofs, resolve dependency
  proofs through kits, enumerate callsites, conjoin same-named contracts, solve
  obligations, recompute witnesses, and report discharge status.
- `sugar verify`: verify a kit end to end. Lift its contract claims,
  discharge each via the solver-dispatch table, recompute witnesses with the kit
  oracle untrusted, and emit a signed per-claim receipt. This is the gate verb.
- `sugar dump`: pretty-print a `.proof` envelope (members, bodies,
  signatures).
- `sugar hash`: compute the BLAKE3-512 CID of a file or stdin.
- `sugar implicate` (alias `imp`): mint an implication memento (antecedent
  CID to consequent CID) via z3.
- `sugar compose`: the JSON-RPC transport for the canonical compose
  primitive.
- `sugar recognize`: scan source for shapes matching published sugar binding
  templates and emit tags (the reverse direction of `materialize`).
- `sugar materialize`: materialize concept-citation carriers into
  library-bound source via realize kits.
- `sugar bind`: bind concept contracts to source code (the eight-verb
  pipeline against arbitrary user code).
- `sugar emit`: emit target/framework test artifacts from neutral contract
  predicates.
- `sugar protocol` / `sugar verify-protocol`: work with protocol catalog
  evolution artifacts, and confirm the local install conforms to its embedded
  protocol-catalog CID.
- `sugar doctor`: validate a kit's config and manifest wiring before a run.
- `sugar init`: scaffold a project (`sugar.toml`, `.sugar/`, sample
  invariant, GitHub Action).

- `sugar lift`: dispatch the configured lift surface and write its ProofIR
  term JSON. `lift` stops at the lifted terms; `mint` is the verb that envelopes
  them into a signed `.proof`, which is what every example here uses.

The command surface keeps moving as protocol work lands; `sugar --help` is the
source of truth.

## Install

This repository is build-from-source today. Crates.io publishing is still future
work. The current install path is:

```sh
cargo install --path implementations/rust/sugar-cli
```

Verify the installed CLI's embedded protocol catalog:

```sh
sugar verify-protocol
```

For project setup and first runs, start with
[docs/quickstart-end-user.md](docs/quickstart-end-user.md). If you are working
on Sugar itself, see [docs/contributing/build.md](docs/contributing/build.md)
for the polyglot Make targets, system dependencies, and per-implementation build
commands.

## Run the demos

The numpy demos provision their own venv on first run.

| Demo | What it shows | Path |
|---|---|---|
| Vendor a whole library | ~2900 numpy functions sugar-lifted into one `.proof`, no shim, witness package, consumer `verify` recomputes | [examples/numpy-vendor/](examples/numpy-vendor/) |
| Discharge two ways | one operation, `numpy.add`, proven consistent (z3) AND witnessed (recompute); `discharged: 2` | [examples/numpy-showcase/](examples/numpy-showcase/) |
| Inheritance capstone | a consumer inherits numpy's contract and is refused when it contradicts it | [test_inheritance_e2e.py](implementations/python/sugar-lift-py-tests/tests/test_inheritance_e2e.py) |

## Current status

- **Canonical implementation:** the Rust CLI in
  `implementations/rust/sugar-cli`.
- **Protocol catalog:** embedded in the CLI and verified by `sugar
  verify-protocol`. The reference matrix
  ([docs/reference/per-language-status.md](docs/reference/per-language-status.md))
  tracks the catalog version it was last updated for.
- **Supported ecosystem surface:** Rust, TypeScript, Python, Java, C#, Ruby,
  Zig, Go, C++, Swift, C, and PHP have varying kit, library, lift-adapter,
  embedded-verifier, and LSP coverage. Coverage is empirical; see
  [docs/reference/per-language-status.md](docs/reference/per-language-status.md).
- **Proof artifacts:** `.proof` envelopes, signed mementos, source CIDs, witness
  CIDs, contract CIDs, attestation CIDs, contract-set CIDs, and protocol catalog
  CIDs are the durable units.
- **Executable exhibits:** [menagerie/bug-zoo/](menagerie/bug-zoo/) runs checked
  specimens where native checks pass while lifted cross-package or
  cross-language obligations fail until the missing edge is closed.
- **Self-application:** the CLI can mint proof data from its own assertions and
  tests; see
  [docs/self-application/2026-05-28-snake-eats-tail.md](docs/self-application/2026-05-28-snake-eats-tail.md).

## Start here

| Goal | Read |
|---|---|
| Run the headline demo | [examples/numpy-vendor/](examples/numpy-vendor/) |
| Install and run the CLI | [docs/quickstart-end-user.md](docs/quickstart-end-user.md) |
| Learn the vocabulary | [SHARED-LANGUAGE.md](SHARED-LANGUAGE.md) |
| Understand the product surface | [docs/explanation/product.md](docs/explanation/product.md) |
| Understand the architecture | [docs/explanation/architecture.md](docs/explanation/architecture.md) |
| Understand `.proof` and proofchains | [docs/explanation/proofchain.md](docs/explanation/proofchain.md) |
| See kit and language coverage | [docs/reference/per-language-status.md](docs/reference/per-language-status.md) |
| Publish a `.proof` artifact | [docs/how-to/publishing-a-proof.md](docs/how-to/publishing-a-proof.md) |
| Build or extend a kit | [docs/quickstart-extender.md](docs/quickstart-extender.md) |
| Compare to other tools | [docs/explanation/compared-to/](docs/explanation/compared-to/) |
| Read the paper ladder | [docs/papers/README.md](docs/papers/README.md) |

For the full docs map, see [docs/index.md](docs/index.md).

## What Sugar is not

Sugar is not a replacement for tests. Tests remain the source of much of the
evidence that kits lift.

Sugar is not a replacement for Kani, Prusti, Coq, Lean, F*, Dafny, TLA+, z3,
or other verification tools. Those tools produce evidence; Sugar gives that
evidence a portable, content-addressed, recomputable supply chain.

Sugar is not a central registry. `.proof` artifacts verify from their bytes,
CIDs, signatures, witnesses, and local policy. A server may index proof data for
convenience, but it is not the authority.

Sugar is not a promise that any current kit sees every useful contract in a
codebase. Adapter coverage is empirical. Unknown, unsupported, or lossy surfaces
must be reported honestly as residue, loss, or refusal.

## License

Source files use SPDX headers where present. A repository-level license file has
not been added yet.
