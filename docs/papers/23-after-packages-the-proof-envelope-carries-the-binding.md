# After Packages: The Proof Envelope Carries the Binding

*Paper 22 named the cross-library migration as a source transformation. This paper says what a package looks like once a substrate exists for it to participate in. The shape is not code-plus-three-files. The shape is code-plus-one-envelope, and the envelope carries everything: contracts, evidence, effects, realizations, bindings, discharge receipts, signatures. The substrate does not need a separate file for the binding; the binding is a claim, claims live in proof envelopes, and the substrate already knows how to consume claims. The package manifest gains one line. The distribution channel gains zero. The author signs once.*

## 1. The claim

A package, ideally, ships two things: code and a proof envelope. The proof envelope is a single signed artifact carrying every claim the package makes about itself: the contracts its functions satisfy, the evidence those contracts were lifted from, the effects its operations declare, the realizations its operations bind to, the bindings from its surface ops to concept hub ops, the discharge receipts that justify the contracts, and the signatures that authenticate the whole bundle.

The package manifest carries one pointer:

```json
{
  "provekit": "./provekit.proof"
}
```

That is the entire structural extension at the package-distribution tier. The library author authors and signs the envelope. The publish pipeline ships it alongside the code. The consumer's tooling reads what it needs. Every cross-X axis the substrate dissolves rides this one artifact through whichever package registry already distributes the language's libraries.

## 2. Three rules that discriminate the shape

A binding from a surface operation to a concept hub operation looks superficially like an annotation. "This function corresponds to this concept" sounds like metadata. It is not. Three rules discriminate.

```
content-addressed != verified
annotation != contract
binding claim != annotation
```

A CID stable-names some bytes. That fact is mechanical: hash the canonical encoding, record the hash, the hash is the address. Stable identity does not imply the bytes have been validated against any consumer's schema. Content-addressing is a property of the bytes; verification is a property of what some consumer did with the bytes.

An annotation is documentary: prose, a note, a rationale. The substrate's honesty gradient (the field-naming discipline of #856) places annotations at the documentary tier. A documentary field cannot be discharged. A contract is dischargeable: a solver can compose it, refute it, or accept it. Contracts and annotations are different tiers; field names cannot collapse them without lying.

A binding claim is the third case. "This surface op realizes this concept hub op" is not prose. It is a claim with semantic content. Lift the surface op; you get a content-addressed `(V, A, C, ≤)` tuple. Lift the concept hub op; you get another tuple. The binding claim asserts that there is a morphism between them that preserves the operation's algebra. That assertion is dischargeable: lift, compare, accept or refuse. The binding lives at the solver-facing tier, not the documentary tier.

This is what discriminates the binding from "what your code looks like" in any weak documentary sense. The binding has semantic teeth. It earns its admission to the substrate the same way every other claim does: by being dischargeable.

## 3. What the proof envelope actually contains

The envelope is a typed catalog of mementos, each with a CID, each signed.

```
provekit.proof
  FunctionContractMemento        (paper 14, paper 17)
  EvidenceMemento                (paper 16)
  EffectOccurrence / effect-set  (paper 12)
  RealizationPlanMemento         (paper 18)
  ConceptBindingMemento          (this paper's load-bearing entry)
  LibraryRealizationProfile      (the per-library shape, if needed)
  PromotionDecisionMemento       (admissibility decisions, paper 19)
  ProofRunMemento                (when this proof ran)
  StageReceipt                   (per-stage signed result)
  signatures / attestations      (the author's key)
```

Each entry is a typed memento family. Each family has a per-family registry consumer (paper 19's admissibility spine). The substrate's generic loader indexes the envelope by `(kind, CID)`; the per-family registry parses, validates, and exposes typed queries. No memento family is admissible until some consumer can parse, validate, index, and reject malformed content. The proof envelope is the union of admissible mementos for one package.

The `ConceptBindingMemento` is the binding claim. It carries:

- The CID of the lifted surface operation (e.g., `pg.query(sql, args)`'s `(V, A, C, ≤)` tuple)
- The CID of the concept hub operation it binds to (e.g., `concept:sql-query`)
- The morphism that proves the algebra is preserved (or the loss-record if the binding is loudly-bounded-lossy at this site)
- The admission tier (`Authored`, `Inferred`, `Generated`, or `Self-Attested`)
- The signature

It sits inside the same envelope as the function contracts the same library publishes. The author signs once; every claim in the envelope inherits the signature.

## 4. Why not a separate `.sugar` artifact

The temptation to make `.sugar` a standalone file is real. The author writes binding claims by hand more often than they write contracts; sugar feels lighter; "ship the sugar separately from the proof" sounds like a clean separation of concerns. Resist it.

A standalone `.sugar` file would be content-addressed bytes alongside the package. The CID would be stable. The publisher could sign it. So far, it looks substrate-shaped.

The failure mode shows up at the consumer. The verifier reads `.proof` and discharges contracts. It does not validate `.sugar`. The realize side reads `.sugar` and emits target source. It does not check `.sugar` against `.proof`. The discharger has no path to compose binding claims against contract claims because the artifacts live in separate signing roots. The substrate registry indexes `.sugar` opaquely; per-family validation either does not happen, or the loader invents its own validation outside the per-family-registry discipline.

This is precisely the failure mode of #856's Bridge C cautionary tale, at deliverable scale. The HTTP sugar cells invented `concept_bindings` inside opaque content; the CIDs were valid; the loader could not read them; the substrate accumulated noise. The fix was to spec `ConceptBindingMemento` first, build the consumer registry, then mint cells that conform. The same discipline applies one tier up: do not invent a parallel substrate-facing artifact when an existing memento family covers the claim.

The corrected shape collapses the temptation. The binding is a memento family. Memento families live in proof envelopes. One envelope per package. One signature root. One admissibility surface. One honesty gradient. One discipline.

## 5. Sugar survives as authoring source

The corrected shape does not eliminate the developer's wish for an easier authoring format. `.sugar` survives, but as authoring source, not substrate state.

```
bindings.sugar     (optional, human-authored, build-time)
   |
   |  publish pipeline compiles + signs
   v
provekit.proof     (durable, content-addressed, substrate-facing)
```

The analogy is `.ts` -> `.js` or `.scss` -> `.css`. The library author can write binding claims in a DSL the substrate's tooling provides. The publish step compiles those into `ConceptBindingMemento`s, packs them into the envelope, signs, and uploads. The published artifact is the proof envelope. The sugar source can be checked into the library's repo for the author's convenience and is irrelevant to the consumer.

A future paper may name the sugar DSL and freeze its grammar. This paper does not. The point is: sugar is authoring UX; the substrate's truth is the compiled signed memento inside the envelope.

## 6. Four consumers, one envelope

The envelope serves four classes of consumer, each reading what it needs and ignoring the rest.

**Verify.** Queries the envelope for `FunctionContractMemento`s and the discharge receipts that justify them. The verifier discharges; if every contract has a clean receipt, the package's correctness bundle is sound. This is paper 14's universal-correctness-bundle role.

**Realize.** Queries the envelope for `ConceptBindingMemento`s and `RealizationPlanMemento`s. Given a concept-hub citation in a consumer's source, the realize side asks "which library can present this concept at the target surface?" The envelope answers. The realization is the emitted source.

**Migrate.** Queries the envelope twice: once for the source-library binding, once for the target-library binding. Computes the morphism between source and target bindings at every hub-citation site in the consumer's code. Preserves the contracts the source proof discharged. Emits the patch and the receipt. This is paper 22's mechanism, grounded in this paper's envelope shape.

**Audit.** Queries the whole envelope. Follows signature chains. Cross-references the proof-run mementos against the discharge receipts against the source CIDs in the package. Produces a complete provenance trail for any claim the package made about itself.

All four read from the same artifact. No artifact swap, no cross-file admissibility check, no parallel signing root. One envelope is the discipline.

## 7. The package manifest as substrate pointer

The package manifest gets one line. In `package.json`:

```json
{
  "name": "better-sqlite3",
  "version": "11.0.0",
  "main": "lib/index.js",
  "provekit": "./provekit.proof"
}
```

In `pyproject.toml`:

```toml
[tool.provekit]
envelope = "provekit.proof"
```

In `Cargo.toml`:

```toml
[package.metadata.provekit]
envelope = "provekit.proof"
```

In every package manifest format, one entry pointing at the envelope. The tooling reads the manifest, finds the pointer, loads the envelope. The substrate's distribution problem reduces to whatever the language's package distribution already solved.

A package may, of course, ship without a proof envelope. The substrate's tools degrade: realize cannot bind the package's surface ops to any concept hub; migrate cannot move callsites through this package's surface; verify cannot discharge anything the package claims it satisfies. The package still works as code. It just does not participate in the substrate. This is the default state of every package today. The transition is opt-in: the author writes claims, the publish pipeline seals them, the envelope ships.

## 8. The four admission paths, named at the package tier

Paper 21 §6 named three paths a binding can enter the catalog. This paper completes the list at the package-distribution tier.

**Authored.** The library author writes the binding claims, compiles them, signs them, ships the envelope. The signature is the author's key. The admission tier is `Authored`. The realize quality is the author's choice.

**Self-Attested.** The library author ships an envelope whose binding claims have not been independently discharged; the author signs the assertion that the bindings hold. The substrate treats this as a stronger claim than third-party Inferred but weaker than third-party Discharged. The consumer's verifier policy decides whether to trust Self-Attested without independent discharge.

**Third-party (Inferred).** A third party (a community contributor, a research project, a substrate maintainer) writes binding claims about a library whose author has not shipped its own envelope. The third party publishes a separate package, signed by the third party's key, that names the target library's CIDs and asserts the bindings. The package manifest has its own `provekit` pointer; consumers who install both packages get both envelopes; the substrate consumes the bindings with their admission tier transparently named.

**Generated.** A language model proposes binding claims; the substrate discharges them; the discharged claims enter the catalog with `Generated` admission. The substrate's honesty gradient ensures the receipt names the generation method; the consumer's verifier policy decides whether to weight Generated claims as equal to Authored, lower, or refused outright.

The four paths produce the same shape: signed mementos in proof envelopes, distributed by registries. The admission tier is named at every step. No path is privileged at the substrate layer. The consumer chooses what they trust by configuring their verifier.

## 9. Distribution rides existing infrastructure

The substrate has no distribution channel of its own. It has no central registry. It has no governance surface. It has, instead, the union of every existing package registry: npm, PyPI, crates.io, RubyGems, Maven Central, Hex, NuGet, CPAN, every language's package distribution mechanism that has accumulated trust, content-addressing, signing, and federation for decades.

The proof envelope is signed bytes. Signed bytes are what registries already distribute. The publish step is:

1. Library author writes claims (optionally via sugar source)
2. Build pipeline compiles claims into mementos, packs them into the envelope, signs the envelope
3. The envelope is included in the package tarball
4. `npm publish` (or `cargo publish`, `pip upload`, etc.) ships the tarball
5. Consumers' `npm install` (etc.) puts the envelope in `node_modules/<pkg>/provekit.proof`
6. The substrate's tooling reads the envelope from disk

No new infrastructure. No new registry. No new governance question. The cypherpunk shape: the catalog is the union of every package author's signed claims, distributed through whichever channel already moves the package, federated by the existing trust infrastructure of the language's ecosystem.

The competition shifts to authorship. Library authors who ship sound envelopes accumulate trust with consumers who care about substrate participation. Library authors who refuse get third-party Inferred bindings; the third party's signature is on those bindings; the consumer decides whether they trust the third party more than the library author's silence. The lock-in dissolution mechanism from paper 22 §7 grounds out here: package-author keys are the trust currency, and the trust is on bindings, not on the right to ship a package.

## 10. Bootstrap: the third-party envelope as catalog seed

Paper 22 §7 names the developer-keyboard pitch: refusals at your callsites become commits to your competitor. This paper grounds the timeline: a third-party Inferred envelope can ship today, before any library author has heard of ProvekIt.

The bootstrap shape:

1. A contributor writes a binding envelope for an existing library (e.g., `@provekit/bindings-better-sqlite3`)
2. The envelope claims the library's surface ops map to specific concept hub CIDs
3. The contributor signs and publishes the envelope as a standalone npm package
4. Consumers `npm install @provekit/bindings-better-sqlite3` alongside `npm install better-sqlite3`
5. The substrate's tooling discovers two `provekit.proof` files in `node_modules/`; both bind operations in the better-sqlite3 surface; both are signed; the consumer's verifier policy decides which to trust

When the library author later ships their own envelope inside `better-sqlite3/`, the same surface ops have two competing envelopes: the third-party and the official. The consumer's policy decides. Typically the official wins, the third-party deprecates itself, but neither artifact mechanically supersedes the other; both remain in npm history, signed by their respective keys, available to anyone who wants to verify the migration history of their codebase across decade boundaries.

The catalog is bootstrapped by anyone willing to sign a claim. The catalog graduates when authors take over. Both modes coexist; the substrate does not pick.

## 11. What this does not solve

The envelope-as-package-extension does not dissolve every package-distribution concern.

It does not dissolve **key management.** The author must hold a key. The author must keep the key safe. The author's revocation must propagate. These are existing problems for signed packages; the substrate inherits them rather than solves them. PGP, sigstore, signify, minisign, every existing answer works; the choice is the ecosystem's, not the substrate's.

It does not dissolve **version skew.** A consumer pinning `pg@9.0.0` consumes the envelope at that version; a consumer pinning `pg@10.0.0` consumes a different envelope. Migrations between versions are migrations between different envelopes by the same author. The substrate handles this the same way it handles any cross-version axis (paper 21 §3, the cross-version case). Per-version envelopes do not eliminate the developer's choice of which version to pin.

It does not dissolve **typosquatting and malicious packages.** A malicious envelope is still a signed claim; the substrate's verifier accepts it if the consumer's policy trusts the signer. The package distribution layer's existing defenses (reputation, manual review, downstream audit) carry the same weight here as they do for code.

It does not dissolve **the cost of authoring.** A library author must write the binding claims, or accept third-party bindings about their library. Either path involves human labor. The substrate makes the labor productive (each binding is reusable across every consumer of the library) but does not eliminate it.

It does not dissolve **performance characterization.** The envelope carries contracts; contracts carry effect signatures; effect signatures admit cost annotations only weakly. Two libraries can satisfy the same contracts and have wildly different performance. The substrate names the contracts as equal at the algebraic tier; the application's benchmarks remain the developer's job.

The substrate is honest about its scope: it dissolves the work of being commensurable. It does not dissolve the work of choosing.

## 12. Empirical anchor

This paper's claim is empirical at three sites, with shipped status mixed.

**Site one: shipped memento families.** Paper 14's universal-correctness-bundle work landed `.proof` envelopes for the menagerie exhibits. PR #873 (Stage 2 of paper 22) shipped `AggregateSummaryMemento`, `ConceptSiteMemento`, `PromotionDecisionMemento`, `HaltMemento`, `RefusalMemento`, and `LossRecordMemento` as typed Rust structs in `provekit-ir-types`. PR #875 (the witness experiment) shipped `WitnessMemento` plus a per-family `WitnessRegistry` consumer in `libprovekit` (parse, validate, index by `(subject, fixture_state_cid)`, reject malformed). Those mementos are not hypothetical; they are in `main` today.

**Pending memento families.** `ConceptBindingMemento` and `LibraryRealizationProfile` are NOT yet shipped as typed structs in `provekit-ir-types` or backed by registry consumers in `libprovekit`. #858 is the still-open spec for the `LibraryRealizationProfile` shape; `ConceptBindingMemento` is the binding claim this paper names but has no typed implementation today. The §3 envelope catalog is the trajectory we are committing to as we ship those families. Until they land, a `.proof` envelope can carry binding intent only as untyped JSON cells, which is the failure mode #856 names; we should not call those typed-registry-backed mementos before they are.

**Site two: the cross-library receipt landed.** PR #872 (Stage 1) shipped the SQL concept-shape catalog (`concept:sql-query`, `concept:sql-execute`) and TS realize kits for `better-sqlite3` and `pg` keyed by `library_tag` (Bridge E PR #867's mechanism). PR #873 (Stage 2) shipped the async-rewrite engine and produced the receipt at root CID `blake3-512:9faa22b51d6bb08e166a0ebd99bf95a21ab3ea61951c6f420840c68fb985d7f523a5bbfc72888d82d1269d4cc50303f8a243f978b76836ada8fe343f6ba88910`. The natural next packaging step is to extract the bootstrap kits as `@provekit/bindings-better-sqlite3` and `@provekit/bindings-pg` standalone npm packages, each with a `provekit.proof` envelope, each signed. That extraction is the empirical demo of §10's bootstrap shape (still future).

**Site three: the runtime-witness anchor.** PR #875 (the witness experiment) committed a fixture sqlite at `examples/migrate-demo/users-better-sqlite3/fixture.sqlite` with `fixture_state_cid: blake3-512:295e0fd280088fc1e5e00d7bade11a2bf850c932180622e28f2fc92e64f97cd5bd757a73acf07f888b7c523e8efb65d8f0d01d50bc02740e5d771e750485d8f4`, plus the substrate's first runtime-observed signed claims: four `WitnessMemento`s observing the row-shape for `concept:sql-query` callsites against that fixture. The witnesses re-discharge byte-equal under fresh re-runs. This is the substrate's first claim grounded in observed runtime behavior against a content-addressed environment, lighting up paper 19's empirical-contract discharge as a fifth admission path: Witnessed.

**Still future.** The cross-language round-trip (TypeScript -> Python sqlite3 / aiosqlite, paired with a Python-side witness against the same `fixture_state_cid`) is in flight at the time of this revision. When it lands, this paper's §6 four-consumer claim becomes runnable across languages.

Until `ConceptBindingMemento` and `LibraryRealizationProfile` ship as typed families with their registries, the paper's claim at §3 is the substrate's design articulated cleanly; the named-but-not-yet-implemented families are the shape we are committing to as we mint them. The honesty gradient (#856) is the discriminator: a binding cell sitting opaquely in `.proof` content is at the documentary tier until a typed registry exists to parse, validate, index, and reject malformed bytes.

## 13. Closing line

The package ships what your code does, and what it looks like to do it, in one signed envelope.

That is the substrate's contribution to package distribution. Not a new file. Not a new registry. Not a new authority. One envelope, the same shape every package now carries, signed by the author, distributed by whichever channel already moves the package, federated by the existing infrastructure of the language's ecosystem. The catalog is the union of every author's claims, and the catalog has no host.
