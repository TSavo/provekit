# Supply Chain Rails

Supply Chain Rails is the Menagerie destination for proof-carrying
artifact admission.

Its core claim is stronger than artifact authentication:

```text
an authentic compatible-looking release cannot silently betray its contracts
```

A package maintainer may still sign a bad release. The package name may be
right. The registry may be right. The tarball hash may match. A real
`slsa-verifier` VSA check and a real `in-toto-verify` pipeline check may be
green. That only proves the package's provenance and process predicates.

That is the problem statement:

```text
SLSA does not catch it.
in-toto does not catch it.
ProvekIt does.
```

The exhibit should not caricature SLSA or in-toto as broken. Their receipts are
allowed to be valid. They simply discharge different predicates than the one the
consumer needs for safe admission.

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
ordinary supply-chain receipts: green
ProvekIt admission receipt: red
```

`1.4.1` is admitted because its package identity, binary identity, contract set,
lowered witnesses, and CI input closure compose. `1.4.2` is published by the
same legitimate maintainer with ordinary provenance green, but it introduces a
forbidden behavior.

The walkthrough must first show the problem as a package manager sees it:

```text
right name, right maintainer, right version shape, right provenance, right build
```

Then it must show the ProvekIt result:

```text
authentic artifact, inadmissible contract graph
```

The attacker then has only visible choices:

| Choice | Conventional supply-chain result | ProvekIt receipt |
| --- | --- | --- |
| Keep the old contract set and lie about behavior | green | ORP lowerer refuses or emits a counterexample instead of a witness `.proof` |
| Weaken the contract set to match the new behavior | green | `oldSet subset newSet` fails for the claimed compatible update |
| Substitute bytes after admission | maybe green if metadata is replayed | observed `binaryCid` differs from the admitted artifact |
| Reuse old CI evidence | maybe green if job name passes | CICP input-closure CID differs from the accepted result witness |
| Change policy or signer assumptions | usually invisible | `policyCid` or authority memento changes |

This is the rails metaphor. Admission is not one lock. It is a vector of
independently addressable tripwires:

```text
(contractSetCid, witnessCid, binaryCid, ciInputClosureCid, policyCid)
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
proof. The proof is the receipt chain that shows conventional green, then the
specific ProvekIt rail that turns red. PyPI/PIP can get a sibling exhibit later;
this one is npm all the way down.

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
minting, `.proof` bytes, binary CID checks, and CICP checks.

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
| CICP result/reuse receipt | CI evidence applies to this exact source, lockfile, toolchain, policy, catalog, and witness input closure |
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
