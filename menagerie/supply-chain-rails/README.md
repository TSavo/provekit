# Supply Chain Rails

Supply Chain Rails is the Menagerie destination for proof-carrying
artifact admission.

Its core claim is narrower and stronger than artifact authentication:

```text
an authentic conventional-green release can still fail contract admission
```

A package maintainer may still sign a bad release. The package name may be
right. The registry may be right. The tarball hash may match. A real
`slsa-verifier` VSA check and a real `in-toto-verify` pipeline check may be
green. That proves useful provenance and process predicates, not the preserved
behavioral contract the consumer relies on.

That is the precise problem statement:

```text
SLSA VSA verifies the tarball's declared provenance/process summary.
This in-toto layout verifies the packaging step and product digest.
ProvekIt rejects admission when a preserved contract cannot lower into evidence.
```

The exhibit should not caricature SLSA or in-toto as broken. Their receipts are
allowed to be valid. They simply discharge different predicates than the one the
consumer needs for safe admission.

## Honesty Boundary

This exhibit is npm-shaped, not a complete model of the npm ecosystem. The
artifact is a real `npm pack` tarball, and ProvekIt invokes real
`slsa-verifier` and `in-toto-verify` binaries. The registry, maintainer keys,
and attestations are repository-owned fixtures so the receipt chain is
reproducible.

That means the exhibit does not claim that ProvekIt currently models every npm
attack surface. It does not model full npm dependency resolution, lifecycle
script semantics, registry provenance, package aliases, bundled dependency
behavior, or every possible in-toto layout. The npm lifter maps enough package
rails to carry the contract admission story. The JavaScript lowerer now parses
JavaScript and emits AST-backed evidence for `runtime.no-env-secret-read`,
including refusal spans and unsupported-semantics receipts. It is still not a
general-purpose JavaScript verifier beyond that documented coverage envelope.

The path to remove those caveats is tracked in:

- [#498](https://github.com/TSavo/provekit/issues/498): make the npm inspector
  a complete package semantic model.
- [#499](https://github.com/TSavo/provekit/issues/499): promote the JavaScript
  lowerer beyond the current `runtime.no-env-secret-read` envelope into a
  general JavaScript evidence engine.

Until those land, the claim is conditional and receipt-shaped: for this
package-shaped release, this contract set, these accepted lowerers, and this
admission policy, conventional receipts stay green while ProvekIt rejects the
release on the rail whose contract evidence fails.

The private signing keys under `authenticated-betrayal/packages/*/attestations`
are repository-owned fixtures. They are intentionally checked in so the native
SLSA and in-toto receipts are reproducible without a hidden signing ceremony;
they are not trusted project credentials, and they are not the evidence ProvekIt
accepts for package admission.

ProvekIt asks the admission question as a set of DAG queries:

```text
does the new release extend the previous contractSetCid?
does every preserved contract have an accepted lowered witness .proof?
does the observed artifact match the pinned binaryCid?
does the CI result witness name this exact input closure?
does policy accept the signers, lowerers, witnesses, and transition?
```

The first exhibit is authenticated betrayal. Its user-facing rhythm is
green-red, not red-green:

```text
safe-json 1.4.1 -> safe-json 1.4.2
conventional receipt rails: green
ProvekIt admission receipt: red
```

`1.4.1` is admitted because its package identity, binary identity, contract set,
lowered witnesses, and CI input closure compose. `1.4.2` is published by the
same fixture maintainer with conventional provenance green, but it introduces a
forbidden behavior.

The walkthrough must first show the problem as conventional package receipts
see it:

```text
right name, right maintainer, right version shape, right provenance, right build
```

Then it must show the ProvekIt result:

```text
authentic artifact, inadmissible contract graph
```

The attacker then has only visible choices:

| Choice | Conventional receipt result | ProvekIt receipt |
| --- | --- | --- |
| Keep the old contract set and lie about behavior | green | ORP lowerer refuses or emits a counterexample instead of a witness `.proof` |
| Weaken the contract set to match the new behavior | green | `oldSet subset newSet` fails for the claimed compatible update |
| Substitute bytes after admission | maybe green if metadata is replayed | observed `binaryCid` differs from the admitted artifact |
| Change policy or signer assumptions | usually invisible | `policyCid` or authority memento changes |

This is the rails metaphor. Admission is not one lock. It is a vector of
independently addressable tripwires:

```text
(contractSetCid, witnessCid, binaryCid, policyCid)
```

Pinning any vector gives an alarm bell on that rail. Pinning the full vector
makes compatible-looking betrayal structurally visible.

## Exhibit Shape

The exhibit is specified as receipts, not as a whitepaper:

- [authenticated-betrayal/specimen.yaml](authenticated-betrayal/specimen.yaml)
  names the claims, rails, mutations, and expected receipts.
- [authenticated-betrayal/walkthrough/README.md](authenticated-betrayal/walkthrough/README.md)
  defines the CLI-first tour and the proof artifacts each step must show.

The poisoned npm package tarball is the artifact under test. It is not the
proof. The proof is the receipt chain that shows conventional green for the
fixture's package-shaped rails, then the specific ProvekIt rail that turns red.
PyPI/PIP can get a sibling exhibit later; this one is npm-shaped all the way
down.

The npm lifter does not author behavioral contracts from package metadata. It
maps package rails, then delegates the contract surface to the existing
TypeScript lifter. In this exhibit `contracts.ts` carries the source
annotations, `provekit-lift-ts` emits the ProofIR contracts, and the npm lifter
wraps those contracts with package release identity, authorities, and witness
demands.

The runner follows the Bridgeworks pattern: it shells through the Rust
`provekit` CLI for every evidentiary step. The destination owns exhibit
orchestration and fixture expectations. The CLI owns package identity
inspection, ProofIR validation, contract-set checks, ORP lowering, witness
minting, `.proof` bytes, and binary CID checks.

If a walkthrough script has to call `npm`, `slsa-verifier`, `in-toto-verify`,
`sha512sum`, or a local helper directly to prove the central claim, the exhibit
has failed. Those artifacts may exist as native package inputs. The receipts
shown to the visitor must be emitted or inspected by `provekit`, and the
package inspector is responsible for invoking the native receipt tools.

## Receipt Contract

Every green or red result in this destination must have a receipt:

| Receipt | Proves |
| --- | --- |
| package attestation receipt | the release claims a contract set, previous contract set, binary CID, and signer |
| contract-set extension receipt | the claimed compatible update preserves every previous contract |
| ORP lower witness receipt | the host artifact satisfies one demanded contract under an accepted lowerer |
| ORP lower refusal receipt | the host artifact cannot satisfy a demanded contract and carries a counterexample when available |
| binary identity receipt | the observed bytes match the admitted `binaryCid` |
| policy admission receipt | the verifier accepted the signers, lowerers, witnesses, and transition rules |

No generated witness source should be checked in as evidence. Lowered witnesses
are produced at runtime by `provekit lower`, then minted into `.proof` only when
they succeed. Refusals are receipts too: a failed lower result is the red proof
that the preserved contract could not be discharged.

SLSA VSA and in-toto verifier receipts remain in the package-shaped receipt set
as necessary context, not sufficient admission. The ProvekIt admission receipt
is the first receipt that asks whether the authentic artifact still satisfies
the preserved behavioral contract set.

## Run Surface

```sh
cargo run --manifest-path menagerie/supply-chain-rails/Cargo.toml -- --all
```

The interactive walkthrough lives under
`authenticated-betrayal/walkthrough/`. Native artifacts stay native, ProvekIt
carries the portable contract claims, and every step names the receipt that
proves or rejects the claim.
