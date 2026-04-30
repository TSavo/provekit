# ProvekIt: The Semantic Envelope That Was Missing

> Author: shared session 2026-04-29 (T + Claude). The architectural punch
> line, sharpened. Why this stack exists, why nothing else does what it
> does, and what falls out for semver, supply chain, and library
> distribution.

## The load-bearing claim

> **ProvekIt is the first content-addressed envelope for semantic
> correctness claims about code, composing across language and library
> boundaries via propertyHash CIDs.**

That sentence justifies every architectural choice underneath it. If
it's true, the rest of the framework follows. If it's false, the
framework has no novelty to defend.

The remainder of this spec defends that sentence — by enumerating what
the claim is *not* saying (so we don't overclaim), what it *is* saying,
and what falls out as operational consequence.

## What this claim is NOT saying

The claim does not say signing is new, trees are new, or that nobody
has composed signatures before. All of those exist and have for
decades. Specifically:

- **Signing exists, institutionally.** Apple notarization, Microsoft
  Authenticode, sigstore, Debian package signing, npm provenance, Node
  release signatures, kernel.org signatures, Intel SGX attestations,
  TUF, in-toto, secure boot. Each of these is a real institutional
  signing layer, with revocable keys, audit logs, and skin in the game.
- **Merkle trees and content-addressing are not new.** Content-addressed
  storage is older than BitTorrent. Git's commit DAG is content-
  addressed. IPFS exists. Bitcoin's chain is a Merkle tree.
- **In-domain composition exists.** SLSA composes provenance attestations
  about a build pipeline. in-toto chains build steps. TUF composes
  signing roles. Apple secure boot composes UEFI → kernel → kext. Linux
  IMA chains file signatures. sigstore + Rekor compose attestations into
  a transparency log. Each of these is a real composable signing tree
  *within its domain*.
- **Refinement-typed languages exist.** F*, Liquid Haskell, Idris, Coq,
  Lean — all express semantic claims about code, often verifiable.

The claim is sharper than any of the above and orthogonal to all of them.

## What this claim IS saying

Three things existing systems do not do, that ProvekIt does:

### 1. Operates at the semantic layer

Existing systems all operate **above** the semantic layer:

- **SLSA** signs *who built it*: provenance metadata about the build
  pipeline.
- **sigstore / Authenticode / Apple notarization** sign *who published
  it*: artifact-publisher identity binding.
- **TUF** signs *which version is current*: distribution integrity.
- **in-toto** signs *the build steps*: supply-chain attestation.
- **secure boot / IMA** sign *which bytes get to execute*: tamper
  detection and trusted execution.
- **type systems** check *types within a compilation unit*: a verdict
  is computed but never published as a content-addressed leaf that
  other systems can compose with.
- **semver** *labels* what changed between versions: a social contract
  with no machinery to verify the label.

None of these say "this function preserves this property." None publish
"the propertyHash for `forall x, abs(parseInt(x)) >= 0` is satisfied by
this code." That layer — content-addressed semantic claims about
behavior — has no envelope today.

ProvekIt's contribution is signing at the semantic layer. The
propertyHash CID is the leaf type that didn't exist before.

### 2. Neutral cross-domain composition

Even within "things that compose," every existing system picks itself
as the trust root:

- SLSA assumes the build pipeline is the spine.
- sigstore assumes the sigstore PKI is the anchor.
- Apple's secure boot chain assumes Apple's CA.
- npm provenance assumes GitHub OIDC + npm.
- Refinement-typed languages assume one solver, one logic, one
  language ecosystem.

If you want to combine an Apple notarization, an Intel SGX attestation,
a kernel.org tarball signature, a sigstore bundle, your tsc verdict,
your biome verdict, and your unit-test passes into ONE proofHash for a
single binary, no system today does that. They each demand to be the
root.

The envelope's contribution is **neutrality**. ProvekIt has no PKI, no
trust root, no governance over what counts as a valid leaf. A CID is a
CID. Whether the bytes are an Apple `.p7` signature, a SLSA in-toto
attestation, a sigstore bundle, a Z3 verdict, or a tsc result — the
merkle math is identical.

### 3. The "stop at hashes" discipline

Existing trees verify end-to-end *within* their domain. ProvekIt's
discipline says: **don't re-verify; attach.**

If you trust the Node TSC, take their release tarball signature CID as
a leaf in your proofHash. Don't rebuild V8 to check. The leaf inherits
the institutional weight — the Node TSC is accountable, with revocable
keys and a documented release process — and your tree just composes
around it.

This is the only way the chain reaches arbitrarily deep. SLSA stops at
the build host. sigstore stops at the artifact. Apple stops at the OS.
ProvekIt stops at hashes from independent trust roots, and that lets
the chain extend through:

```
your binary's proofHash
  ↓ leaf: tsc memento (you signed, against your config)
  ↓ leaf: biome memento
  ↓ leaf: vitest memento
  ↓ leaf: package-lock.json (signed by npm provenance + GitHub OIDC)
  ↓ leaf: Node 24.x.y.z (signed by Node release team)
    ↓ V8 (referenced inside Node's signed source)
    ↓ ECMA-262 (Ecma International's process)
    ↓ glibc 2.39 (signed by GNU)
    ↓ Linux 6.x (signed by kernel.org)
    ↓ Intel microcode (signed by Intel)
    ↓ silicon (Intel's attestation chain, ultimately physics)
```

Each leaf inherits the meaning of its signer's institutional process.
The proofHash inherits everything underneath it without re-doing any of
the work those signers already did.

## Why nobody assembled this before

Not because it was hard cryptography (it wasn't), but because it
required three things together:

1. **A canonical form for invariants** so the same property in TS,
   Rust, Go, C++ converges to one hash. That requires an IR + a
   canonicalizer that are language-neutral. Type systems aren't
   neutral; refinement types aren't language-portable; SMT-LIB is
   close but isn't a content-addressed leaf format.

2. **Universal-claim leaves.** Every existing signing system signs
   *existential* things ("this artifact exists, signed by X"). Code
   invariants are *universal* ("forall x, P(x) holds"). A universal
   claim is a different leaf type — its body is the canonical form of
   the property, its verdict is a content-addressed check result, and
   its trust comes from whoever signed the verdict, not from whoever
   produced the artifact.

3. **The discipline of stopping at hashes.** Refinement-typed languages
   try to verify everything end-to-end. ProvekIt deliberately doesn't.
   You stop at the leaf signature; you trust the signer of that leaf to
   have done their job; you compose by CID. This sounds like giving up,
   but it's what lets the chain reach physics. End-to-end verification
   does not scale across institutional boundaries.

## Cross-library semantic correctness composition

The corollary to semantic-level leaves: **propertyHashes compose across
library boundaries.**

A library author signs N invariants. The library's catalog memento
holds those N propertyHashes. The catalog's CID is what
`package.json`'s `provekit.proofHash` field carries.

A consumer of that library imports K of the N propertyHashes by CID.
The consumer authors *bridge mementos* — content-addressed edges that
say "at this call site in my code, I depend on the library's
`<propertyHash-X>` being upheld."

The consumer's binary's proofHash now claims three things, all signed,
all composing into one root hash:

- The consumer's own code upholds its own invariants.
- The library upholds its declared invariants (referenced by CID, not
  re-verified — the library author signed for that).
- The consumer's usage respects the library's invariants (every call
  site has a bridge to a propertyHash that's still in the library's
  catalog).

No existing system does this. Type systems check shape across the
boundary; ABI checkers detect structural changes; refinement-typed
languages can express semantic claims but don't compose across language
or library boundaries. Bridge mementos are the primitive that makes
cross-library semantic composition mechanical.

## Operational consequence: semver derived, not declared

Today's semver is a social contract. The library author *labels* a bump
MAJOR / MINOR / PATCH based on their judgment of whether it's breaking.
There's no machinery to verify the label. People mislabel constantly:
patch releases that break consumers, major bumps that didn't need to be
major. Consumers can't tell from the version string what actually
changed at the semantic level.

With propertyHash composition, **the bump's category is computed from
the catalog diff, not declared by the author.**

The migrate workflow does this today:

| What changed at the propertyHash level | Computed semver shape |
|----------------------------------------|------------------------|
| All propertyHashes unchanged           | patch (no semantic change) |
| Strengthened (acceptance set shrinks)  | patch / minor depending on direction |
| Weakened (acceptance set grows)        | minor / major depending on consumer bridges |
| Retired (propertyHash removed)         | **major** — punch list names every consumer-side bridge that breaks |
| Added (new propertyHash)               | minor (pure addition, backward compat) |

So semver labels stop being declarations and become **verifiable
predicates over the propertyHash diff.** Anyone can compute the true
category. The library author's chosen number becomes either matching
the computed answer or wrong.

### The stronger move: kill the version string

`"deps": {"foo": "^1.2.3"}` is a coarse hint. The real binding is the
catalog memento CID. The minimal version of this:

```json
"deps": {
  "foo": "bafy...catalog-cid..."
}
```

A library bump is just a new CID. Compatibility is `migrate(oldCID,
newCID)` returning the punch list. Lockfiles already do this for bytes;
ProvekIt does it for *meaning*. The version string is at most a
human-readable nickname for a catalog CID; the binding constraint is
the CID.

### The strongest move: supply-chain auto-firewall

Today's supply-chain attack: malicious patch slips into a popular
library. The library gets a new version number. Consumers auto-update.
The malicious code runs.

Under propertyHashes:

- Library author signs N propertyHashes for version V1.
- Attacker pushes V2 with a malicious patch. The malicious patch
  silently retires or weakens propertyHashes that consumers depend on
  (or fails to mint a propertyHash that the malicious behavior would
  violate).
- Consumers' bridges reference V1 propertyHashes. When they re-resolve
  against V2's catalog, the bridges break — the propertyHashes they
  depend on aren't there, or have weakened.
- Consumer binaries refuse to claim correctness with V2.

The supply chain auto-firewalls at the semantic boundary. Dependabot
today says "the version number changed." Dependabot under proofHashes
says "the meaning changed in *these specific* ways and *here are* the
bridges that broke." The consumer doesn't trust the library author's
labeling; they trust the catalog diff.

This is also a different threat model than reproducible builds.
Reproducible builds detect "the bytes don't match what was supposed to
be built." Semantic-firewalling detects "the bytes do match, but the
*meaning* changed." The first catches build-time tampering; the second
catches author-level malice or mistake.

## Why the existing literature stops short

A reasonable reader will ask: "Aren't SLSA + sigstore + Reproducible
Builds + IMA + refinement types *together* almost the missing
envelope?"

Answer: no, because they're not designed to compose with each other.
Each assumes itself as the spine.

- SLSA + sigstore + Reproducible Builds are about **artifact identity
  and origin**. They sign provenance, not behavior.
- IMA + secure boot are about **execution integrity**. They sign which
  bytes run, not what those bytes mean.
- Refinement-typed languages are about **in-language verification**.
  They prove behavior, but inside one type system, with one solver,
  with no neutral content-addressed leaf format that other systems can
  reference.
- in-toto attestations are general but **scoped to supply chain**.
  Their schemas are pipeline-shaped, not invariant-shaped.

The space they collectively cover is "the supply chain is honest and
the artifact is what was intended." That's load-bearing and necessary.
It is not "the artifact's behavior satisfies these named properties."
That's the semantic layer, and it's the hole.

ProvekIt's contribution is the hole, plus the discipline of attaching
to all the existing layers as leaves rather than competing with them.

## The institutional inversion

The envelope is what's new; the leaves are mostly what was already
there. That makes the on-ramp basically free: `provekit init` walks
`package.json`, finds tsc / biome / vitest / eslint / their per-language
analogs, mints a Stage for each, and a real proofHash exists on day one
without writing a single explicit invariant.

Every language ecosystem already has these tools. cargo check, rustfmt,
clippy in Rust. gofmt, go vet, staticcheck in Go. mypy, ruff, pytest in
Python. They're all leaves. They've all been there for decades. They
just haven't been content-addressed and composed under one root.

The envelope is the difference. Everything underneath was already real.

## Acknowledgments

This spec exists because a session pressure-tested the original
"composable signing tree" framing and caught it overclaiming. The
original draft said "nobody has assembled signatures into a single tree
before, because nobody had the envelope." That was wrong: SLSA,
sigstore, in-toto, TUF, secure boot, IMA *do* assemble signatures into
trees. The corrected claim, after sharpening, is that none of them
operate at the **semantic** layer or compose **across domains**, and
that's the load-bearing distinction. Without that correction this spec
would be an overreach.
