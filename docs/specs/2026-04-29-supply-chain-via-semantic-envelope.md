# ProvekIt: Solving Supply-Chain Attacks via the Semantic Envelope

> Author: shared session 2026-04-29 (T + Claude). Companion to
> `2026-04-29-the-semantic-envelope.md`. Given that the semantic
> envelope exists, here is the specific class of attacks it forecloses
> — and where existing defenses stop short.

## The defense lattice

Supply-chain attacks share one shape: an adversary modifies code in a
dependency that you trust, in order to compromise your system without
modifying your code. The defenses against this fall into five layers:

| Layer            | Question answered                                     | Existing systems                          |
|------------------|-------------------------------------------------------|-------------------------------------------|
| 1. Identity      | Who is authorized to push?                            | npm 2FA, sigstore, GitHub OIDC            |
| 2. Provenance    | Where did this artifact come from?                    | SLSA, in-toto                             |
| 3. Integrity     | Did the bytes change in transit / at rest?            | Reproducible builds, IMA, package signing |
| 4. Distribution  | Did the right version arrive at the consumer?         | TUF, signed manifests                     |
| 5. **Behavior**  | **Does the code do what it claims to do?**            | **— gap —**                               |

Existing systems cover layers 1–4 well. They make it harder to slip a
compromised package into the supply chain. They **do not detect a
compromised package that successfully gets in.**

Layer 5 is where ProvekIt operates. The semantic envelope assumes the
supply chain *will* be compromised at some point and asks: can you
detect a compromised package even if it has all the right signatures,
comes from the right author, and rebuilds reproducibly?

The answer is yes, because the catalog diff at the propertyHash level
is mechanical and adversary-resistant.

## How attacks survive the existing four layers

Each existing layer provides real value but is bypassable:

- **Maintainer takeover.** Layer 1 falls when an attacker gains the
  legitimate maintainer's credentials. Real-world cases: ua-parser-js
  (2021), event-stream (2018), node-ipc (2022). All four upstream
  layers are intact; the attacker IS the trusted identity.
- **Long-term insider.** A maintainer who is legitimate for years and
  then turns malicious. Identity, provenance, integrity, distribution
  all check. The trust model assumed they'd stay benign.
- **Stealth bug-shaped sabotage.** A "fix" that introduces an
  exploitable behavior change while leaving tests green. Reproducible
  builds confirm the bytes are the bytes the maintainer intended; they
  cannot tell whether *those* bytes are correct.
- **Compromised CI / build pipeline.** Layer 3 checks reproducibility,
  but if the toolchain itself is compromised (Reflections on Trusting
  Trust), reproducible builds reproduce the same compromise.
- **Typosquatting / dependency confusion.** Layer 1 protects identity
  but not name. The user believes they pulled `lodash`; they pulled
  `lod4sh`. Both are signed by their respective maintainers.

In every case, layers 1–4 each report "no anomaly." Yet the consumer
ends up running compromised code. The gap is layer 5: nothing checks
whether the *behavior* of the code matches what the consumer is
relying on.

## What semantic-level leaves change

The semantic envelope makes layer 5 mechanical. The argument is the
same one made in `the-semantic-envelope.md`, applied operationally:

### A library bump is detectable at the propertyHash level

When a library author publishes V2 of a package:

1. V2's catalog memento has propertyHashes for each invariant the
   library now claims.
2. V1's catalog memento (already minted) has the propertyHashes V1
   claimed.
3. The migrate workflow computes the diff: which propertyHashes were
   preserved, strengthened, weakened, retired, or newly added.
4. Consumers' bridge mementos reference specific propertyHashes by CID.
   The set of bridges that resolve against V2's catalog vs V1's is
   computable in O(consumer-bridges).

A malicious V2 that violates a property the consumer depends on either
**(a) retires the propertyHash** the consumer's bridge references, or
**(b) keeps the propertyHash but the verdict no longer holds**. Both
cases are detectable.

Case (a) is the easy one: the bridge fails to resolve. The consumer's
proofHash refuses to compose. Their binary cannot claim correctness
with V2.

Case (b) is the load-bearing one: the attacker **lies in the catalog**,
keeping the same propertyHash but leaving in code that violates it.
This is detected by the verdict layer — the SMT/Z3 check or the test-
based check whose CID is the leaf body. If the verdict is freshly
recomputed against the new code and fails, the catalog memento for V2
is internally inconsistent (its claimed propertyHashes don't match
what its own check says holds). That's a content-addressing failure,
not a trust judgment.

### The attacker cannot lie consistently

Where existing trust layers can be socially engineered, the
propertyHash layer cannot, because propertyHashes are mechanically
derived from canonical IR + verdict bytes. Concretely:

- The attacker cannot publish a catalog claiming `(propertyHash-X,
  verdict: holds)` if the code does not, in fact, satisfy
  propertyHash-X under the chosen check. The catalog memento's CID is
  a function of its bytes; if its bytes are inconsistent with the
  verdict, the consumer's verifier rejects it.
- The attacker cannot publish two catalogs with the same propertyHash
  but different verdicts and have both validate, because the verdict
  is content-addressed.
- The attacker cannot retire a propertyHash and pretend they didn't,
  because the consumer is comparing V1's catalog CID to V2's catalog
  CID; the retirement appears in the diff.

The attacker's only remaining moves are:

1. Compromise the *check* itself (the SMT solver, the test harness,
   the lifter). Defended by the same envelope: the check's CID is a
   leaf in the proofHash. Consumers can pin specific check CIDs and
   reject leaves produced by a different check version.
2. Compromise the *consumer's view* of which catalog CID to fetch.
   Defended by layers 1–4 (TUF, sigstore, npm provenance, etc.).
   ProvekIt depends on those layers to know which catalog CID is
   authoritative for V2.

This is the architectural payoff of the envelope being neutral and
composable: ProvekIt doesn't try to defend layers 1–4. It depends on
them as leaves. Every existing supply-chain defense becomes a leaf in
the proofHash, contributing its own institutional weight, while
ProvekIt adds the missing layer 5 leaf.

## Worked threat model: each attack class

### 1. Maintainer takeover

**Attack.** Attacker phishes the maintainer's npm credentials. Pushes
V2 with a backdoor.

**Existing layers.** Pass. Identity, provenance, signature all check —
the attacker IS the maintainer for purposes of those layers.

**ProvekIt.** V2's catalog diff is computed. Either:
- The backdoor's behavior violates a propertyHash the maintainer
  retired in the same release, in which case the migrate workflow
  flags the retirement and consumers see the punch list.
- The backdoor preserves the catalog (identical propertyHashes, same
  verdicts), which means the backdoor must operate within the
  invariants the library has *publicly committed to*. That is a much
  smaller attack surface — the attacker can only express their attack
  in behaviors that are not constrained by any propertyHash. Which is
  the same as saying: the more invariants a library publishes, the
  smaller the maintainer-takeover blast radius.

### 2. Long-term insider turn

**Attack.** A legitimate maintainer of three years quietly inserts a
backdoor in V47.

**Existing layers.** All pass. Years of clean history, valid signatures,
audited builds.

**ProvekIt.** Same as case 1, but with a stronger property: the
maintainer's *previous* releases established a track record of
propertyHash growth (each minor adds invariants, each major retires
some). A V47 that suddenly retires multiple invariants without a
documented rationale is statistically anomalous in the catalog history
and surfaceable by tooling. The defense isn't perfect (a careful insider
adds the backdoor in space not constrained by any property), but the
constraint surface is the public set of propertyHashes, which is
auditable history.

### 3. Stealth bug-shaped sabotage

**Attack.** A "fix" that introduces a subtle integer overflow / off-
by-one / unchecked path. Tests pass.

**Existing layers.** Pass. Reproducible builds confirm bytes; tests
green.

**ProvekIt.** This is the canonical case. If the consumer's bridge
references a propertyHash that the saboteur's change violates (e.g.
"forall x, parseInt(x) >= 0 implies sqrt(x) is real"), the verdict
recomputed against V47 returns "fails." V47's catalog cannot
honestly claim the propertyHash holds. Either:
- The catalog claims it anyway (lying) — content-addressing fails,
  as above.
- The catalog retires it — the migrate workflow flags the retirement.
- The catalog weakens it — the migrate workflow flags the weakening
  and shows which consumer bridges depended on the strong form.

### 4. Compromised CI / toolchain

**Attack.** The build infrastructure is compromised. Tools (tsc, cargo,
go) are themselves modified to inject behavior.

**Existing layers.** Reproducible builds reproduce the compromise.
SLSA L3 depends on the builder being trusted. If the builder is
malicious, layer 3 is bypassed.

**ProvekIt.** The check tools — tsc, biome, vitest, the lifter, the
Z3 solver — are themselves leaves in the proofHash. Each tool's
binary is content-addressed; the consumer's proofHash pins specific
tool CIDs. If the toolchain CID changes, the proofHash diverges,
even if the source code didn't.

This doesn't *prevent* a compromised toolchain from producing
compromised verdicts. It means consumers can pin "I trust this exact
tsc version, signed by this team," and any change to that pinning is a
detectable proofHash divergence. Combined with reproducible builds of
the toolchain itself, the surface for trusting-trust-style attacks
shrinks to "did the toolchain author go rogue or have their key
stolen?", which is layer 1 and out of ProvekIt's scope but visible as a
proofHash diff.

### 5. Typosquatting / dependency confusion

**Attack.** Consumer pulls `lod4sh` thinking it's `lodash`.

**Existing layers.** Both packages are legitimately signed by their
respective publishers. Layer 1 doesn't help.

**ProvekIt.** The consumer's `package.json` carries catalog memento
CIDs, not names. `"lodash": "bafy<canonical-lodash-catalog-cid>"`
binds to a *specific* catalog. There is no name to confuse. The
attacker can publish `lod4sh` with a similar name; they cannot publish
it with the same catalog CID, because catalog CIDs are
content-addressed.

The strongest form of this defense is to remove names from the
dependency mechanism entirely; names become human-readable nicknames
for catalog CIDs, like Git tags vs commit hashes.

### 6. Sub-dependency / transitive attacks

**Attack.** Attacker compromises a dependency of a dependency.
Direct deps look fine; the malicious code is three levels deep.

**Existing layers.** Hard to detect; SBOM tooling can list the
transitive set but not check it semantically.

**ProvekIt.** PropertyHash composition is recursive. Your library A
depends on B which depends on C. A's catalog memento references B's
catalog CID; B's catalog memento references C's catalog CID. When C
is compromised, C's catalog CID changes. B's resolution of "I depend
on C-V1" no longer matches; B's catalog must update. A's resolution
of "I depend on B-V?" surfaces the change. The transitive change
propagates up the proofHash tree, mechanically, without anyone having
to look at code.

### 7. Malicious update via CDN / registry serving wrong bytes

**Attack.** npm or a CDN serves modified bytes for a package the
consumer thought they were pulling.

**Existing layers.** Layer 4 (TUF, signed manifests, package
signatures) helps. Reproducible builds + signature verification
catches this.

**ProvekIt.** Catalog CIDs are self-verifying. If the registry serves
bytes that don't hash to the claimed CID, the consumer rejects them
before any compilation happens. The defense overlaps with existing
package signing but is stronger: the catalog CID is *the* identity,
not an attached signature.

### 8. Sybil attack on review / governance

**Attack.** Attacker creates fake maintainer accounts, gets their
malicious PR merged into a library.

**Existing layers.** Social — depends on the project's review process.

**ProvekIt.** Doesn't help with the merge itself, but the resulting
release's catalog diff is mechanical. If the merged change weakens or
retires propertyHashes consumers depend on, the migrate workflow
flags it regardless of who reviewed. The defense degrades the
attacker's payoff: even if they get malicious code merged, downstream
consumers detect the semantic divergence on the next bump.

## What this does not solve

Honesty matters here. The semantic envelope is not a complete supply-
chain solution. It does not prevent:

- **Properties not stated.** If a library doesn't publish a property,
  the consumer can't depend on it via bridge. Behavior outside the
  property set is unconstrained. The defense is proportional to how
  thoroughly the library author publishes invariants — which is the
  positive incentive to *over-publish* propertyHashes (every published
  property is one more attacker-side constraint).
- **Bugs in the check.** If the SMT solver or the lifter is buggy in a
  way that lets a violation pass, the leaf is wrong. Defended by
  pinning specific check CIDs and by check-vs-check cross-validation
  (multiple solvers verifying the same propertyHash should agree).
- **Side-channel and runtime attacks.** A library with correct
  propertyHashes can still leak data through timing, cache patterns,
  or speculative execution. These are below the semantic layer
  ProvekIt operates at.
- **Compromise of the consumer's local tools.** If the consumer's
  `provekit verify` itself is compromised, it can lie about catalog
  diffs. ProvekIt's own binary is, recursively, a candidate for
  proofHashing — but the bootstrap problem is real and worth flagging.

The claim is not "supply-chain attacks are impossible under ProvekIt."
The claim is **"the missing layer-5 defense becomes mechanical, and the
attacker's surface shrinks to behaviors that no published property
constrains."** That's a load-bearing reduction even if it isn't a
complete solution.

## Composition with existing layers

Critically, ProvekIt does not replace any of layers 1–4. It composes
*above* them by attaching to their signatures as leaves:

- npm provenance signature CID → leaf in your proofHash
- SLSA attestation CID → leaf
- sigstore Rekor entry CID → leaf
- Reproducible build verification CID → leaf
- TUF root signature CID → leaf
- **Plus** the propertyHash CIDs (the new contribution)

A consumer who today relies on (npm provenance + SLSA + reproducible
builds) and adds ProvekIt gets:

1. Everything those layers already provide.
2. Cross-domain composition: each defense's verdict is a leaf, all
   under one proofHash root.
3. The new layer-5 defense: semantic divergence detection at library
   bumps.

The consumer's binary's proofHash now carries every existing trust
signal *plus* a content-addressed semantic verdict. The attacker has
to defeat all five layers to land a compromise unnoticed, where today
they only have to defeat 1–4.

## The institutional shape of the defense

The deepest property of this defense is that it doesn't require
universal adoption to provide value. A single library that publishes
propertyHashes lets every one of its consumers detect compromises in
that library's future bumps. There's no network effect required; each
library author choosing to publish invariants raises the floor for
their downstream tree.

Compare to existing layer-1–4 defenses: SLSA L3 requires a trusted
builder; sigstore requires the Rekor log; TUF requires the project to
adopt the framework. Each demands ecosystem-wide buy-in to be
load-bearing.

Layer-5 defense scales differently: it activates per-library, immediately,
the moment any one library decides to publish propertyHashes. Consumers
who adopt ProvekIt can compose against any library that does, without
waiting for universal adoption.

This is the same property that made content-addressed dedup work in
1995 and made BitTorrent work in 2001: *anyone who publishes a hash
provides value to anyone who can verify it*, with no central
coordination. The semantic envelope is the same primitive applied to
behavior instead of bytes.
