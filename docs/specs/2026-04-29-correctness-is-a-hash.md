# Correctness is just a hash

> Author: shared session 2026-04-29 (T + Claude). The architectural
> punchline. Crystallization document.

## The thesis

> **Correctness is just a hash.**
>
> **A binary is a signed DAG of correctness.**

Two sentences. The rest of this document operationalizes them.

## The proof IS the DAG

A mathematical proof is a DAG of inferences from axioms. ProvekIt's
memento DAG is the mechanical realization of that. Each `verdict: holds`
memento is a proof step. The DAG composes them. The root is the
composite claim.

Mathematicians have called this structure "a proof" for 2500 years.
ProvekIt does not invent a new abstraction. ProvekIt makes the existing
one content-addressed and swarm-distributed.

**Implication:** a "DAG of invariants" is just **the proof**. There is no
separate term to coin. We have inherited the term from formal
mathematics; we apply it without modification.

## The proof hash

The proof DAG canonicalizes to a single 32-byte root hash. That hash
identifies the entire DAG mechanically. Two artifacts with the same
proof hash claim the same set of properties. Verifiable by hash
comparison; no source-tree inspection needed.

```
proofHash := root CID of the proof DAG
```

The proof hash is the canonical correctness identifier of any artifact.

## A binary is a signed DAG of correctness

The phase change in what software IS.

**Before:** a binary is a byte stream that we hope works. Identity is
the content hash of the bytes. Provenance is editorial (changelogs,
release notes, maintainer attestation by reputation).

**After:** a binary IS a signed DAG of correctness. The bytes are
LEAVES of the DAG, alongside the formal claims about what those bytes
do. Identity is the proof hash; the bytes are derived. Provenance is
the DAG walk.

When you say `lodash@4.17.21`, you mean a specific signed proof DAG
whose leaves include a specific byte sequence. The bytes are part of
the DAG. The DAG includes the bytes. They are not separable in the
trust system.

**Practical consequences:**

- "Run lodash" means "execute the bytes whose proof DAG hashes to H,
  validating the binary-proof-binding memento before launch."
- "Trust lodash" means "walk the DAG from H down to its axioms; each
  edge cryptographically attested."
- "Upgrade lodash" means "swap the signed DAG of correctness whose
  hash is H₁ for the one whose hash is H₂, with the diff between them
  surfaced as a punch list of property changes."

The binary is no longer "code that might be correct." The binary is
**a proof, executable**. Curry-Howard, made operational, made
mechanical, made distributed.

## Trustless: your proofkit, not theirs

**This is the keystone.** Everything else in this document is mechanical
plumbing; this section is the architectural commitment that makes the
plumbing structurally sound.

**The library publishes the DAG. You re-prove it. With your proofkit.
Not theirs.**

The maintainer's signature on a published proof is metadata, not trust.
The cryptographic attestation a consumer relies on is not the
maintainer's signature; it's their OWN local re-verification.

When `pnpm install lodash` finishes, your proofkit immediately:

1. Reads the published proof DAG (the structural claims, the
   inputCids, the propertyHashes)
2. Walks the DAG with YOUR producer pool (your z3, your tsc, your LLM
   verifiers, your kit catalogs)
3. Re-mints every memento under your own producer signatures
4. Compares your verdicts to the maintainer's claimed verdicts
5. Accepts only the mementos where YOUR proofkit confirms the claim

The maintainer's signature tells you WHO claimed; your proofkit tells
you WHETHER IT'S TRUE.

### This collapses the maintainer-trust assumption

Today's supply chain attestation tools (Sigstore, SLSA, in-toto) all
ultimately require that you trust SOMEBODY. The maintainer's signing
key. The build environment's attestation. The reproducible-build
auditor. Every link in the chain has a human or organization at its
root that you have to believe.

ProvekIt's adversarial re-verification removes that root entirely. You
trust nothing about the source. You trust nothing about the maintainer.
You trust nothing about the build process. **You trust the math your
own machine performs.**

If lodash's maintainer is compromised and ships a malicious release,
the malicious release contains:
- Bytes (a different content hash)
- A claimed proof DAG (potentially malicious)
- A maintainer signature on the claim

Your proofkit re-runs the proof. Either:
- (a) The malicious bytes don't satisfy the claimed proofs → your
      re-verification fails → install rejected
- (b) The proofs themselves are also malicious → but they have to be
      mechanically valid FOL claims that hold under YOUR producers
      → if any of your producers disagree, install rejected

The attacker would need to produce code that genuinely satisfies a
genuinely-checkable proof of doing what the original code did. At that
point, the malicious code IS the original code's behavior, mechanically
verified. The attack collapses into "the code does what it's supposed
to do" — which isn't an attack.

### Your proofkit is your trust anchor

Your local ProvekIt installation is configured with:

- **Producer pool.** Which SMT solvers, type checkers, LLMs, formal
  provers participate in your verification. A bank's proofkit might
  configure 5 redundant solvers; a hobbyist's might configure 1 LLM.
- **Kit catalogs.** Which language kits and their built-in axioms you
  trust as foundational. The TS-kit's parseInt contract is foundational
  for TS code; the COBOL-kit's contracts are foundational for COBOL.
- **Verification budget.** How thorough you want to be. Cheap (sample
  the DAG, accept on partial verification) vs expensive (re-verify
  every memento with every producer).
- **Acceptance criteria.** Which verdict combinations you accept. A
  paranoid posture demands 3-of-5 producer agreement; a permissive
  posture accepts any single `verdict: holds`.

Different consumers have different proofkit configurations. The same
published DAG produces different verdicts in different consumers'
proofkits. **The framework doesn't impose a uniform trust posture; it
provides the substrate over which each consumer expresses theirs.**

### Producer diversity IS the trust mechanism

Single-producer verification is single-trust. Multi-producer
verification is adversarial re-verification at scale.

If z3 says `verdict: holds` and Soufflé says `verdict: holds` and tsc
says `verdict: holds` and three independent LLMs all say `verdict:
holds` for the same memento, the verdict is robust against ANY
single-producer compromise. Diversity dilutes risk multiplicatively.

The producer pool's diversity IS the trust mechanism. There is no
single oracle. There is a chorus of independent producers, all
adversarially re-verifying the same claims. Compromise becomes
intractable: an attacker would need to compromise enough producers,
across enough vendors, to flip a 3-of-5 vote — for every consumer who
re-verifies.

### Anti-fragility: the framework strengthens with use

Every install is a re-verification. If 1M users install lodash, lodash's
proof DAG is re-verified 1M times by 1M independent producer
configurations. Any disagreement between any two configurations is a
signal — either lodash made a wrong claim, or one producer is broken,
or there's an edge case in producer semantics worth surfacing.

The MORE the framework is used, the MORE proofs are re-verified, the
STRONGER the trust in each proof becomes. **Network effects on
trustability, not just adoption.** This is the structural difference
between identity-based attestation (which doesn't get stronger with
use) and adversarial re-verification (which does).

The framework is anti-fragile in the technical sense: stress (more
adversarial re-verifications) makes it stronger.

### Bad claims are self-limiting

If a kit author writes a wrong invariant ("parseInt always returns a
non-zero number"), every proofkit that re-verifies catches the
wrongness on first use. A counterexample exists; one of the producers
finds it; verification fails; the bad claim doesn't propagate beyond
initial publication.

Catalog mistakes are SELF-LIMITING. Bad invariants are caught on first
use, not on hundredth use after the bad code has spread. The framework
doesn't depend on the kit author's competence; it depends on the
mechanical falsifiability of false claims.

### What this means for the trust hierarchy

There is no trust hierarchy.

There is no "trusted source of invariants." No "trusted package
maintainer." No "trusted framework vendor."

There is only:
- **Claims published** (DAGs)
- **Claims independently verified** (your proofkit's output)

Signatures are useful for IDENTITY, ATTRIBUTION, and PROVENANCE — they
tell you WHO claimed something, which is useful for reputation systems
and audit trails. But the VALIDITY of any claim is independent of the
signer's identity. It is mechanical.

**Trust no one, verify everyone.**

This is the deepest content-addressing primitive applied to
correctness. Bitcoin removed the trusted bank. IPFS removed the trusted
host. ProvekIt removes the trusted maintainer.

## Three-coordinate artifact identity

Today's package identity:

```
name@version + contentHash
```

Tomorrow's package identity:

```
name@version + contentHash + proofHash
```

Three coordinates. ALL THREE must match for an install to be valid.

| Coordinate | What it pins | What changing it means |
|---|---|---|
| `name@version` | The artifact's editorial label | Maintainer marks a release |
| `contentHash` | The exact bytes shipped | Build output bit-identical to claim |
| `proofHash` | The contracts the bytes satisfy | The DAG of properties claimed |

A tampered build has a content-hash mismatch. A package that ships
weaker contracts has a proof-hash mismatch. Both are mechanically
detectable. Both refuse to install when pinned.

**Lockfiles record all three.** `package-lock.json` and `pnpm-lock.yaml`
extend their schema; the three-coordinate pin becomes the standard.

## Library upgrades are proof-hash diffs

```
$ provekit upgrade lodash
Proposing: lodash 4.17.21 → 4.18.0
  contentHash: a1b2... → c3d4...
  proofHash:   H₁     → H₂

Properties added (5):
  + groupByTotalCount
  + groupByPreservesOrder
  + ...

Properties removed (2):
  - shuffleDeterministicWithSeed (callsites in your code: 3)
  - chainPureSemantics            (callsites: 0)

Verification will fail at:
  src/billing/invoice.ts:47    (depends on shuffleDeterministicWithSeed)
  src/audit/random.ts:12       (depends on shuffleDeterministicWithSeed)
  src/test/golden.ts:88        (depends on shuffleDeterministicWithSeed)

Continue? [y/N]
```

Migration is a punch list of named, located violations. Not an
exploration. Not "let's see what tests pass." A precise mechanical diff
between two contract surfaces, surfaced before the upgrade lands.

This collapses an entire category of operational pain. Today: library
upgrades require human exploration to discover what broke. Tomorrow:
library upgrades produce mechanical diff lists, generated before any
code is changed, listing every callsite that's about to fail and why.

## Library discoverability becomes hash-driven

Today: search npm for "shuffle." Get 50 results. Read READMEs. Pick
one. Hope.

Tomorrow: search the proof-hash registry by property:

```
$ provekit search 'forAll<T[]>(arr => shuffled(arr).length === arr.length)'

Found 47 packages whose proof DAGs contain a memento with this propertyHash:
  - lodash@4.17.21       (kit-trusted, 1.8M weekly downloads)
  - underscore@1.13.2    (kit-trusted, 8M weekly downloads)
  - shuffle-array@1.0.1  (community, 50K weekly downloads)
  - ...
```

You discover libraries by **what they prove**, not by what they're
named. The propertyHash is the unit of search. Two libraries that prove
the same property are interchangeable at that property's level —
mechanically, by hash. The market self-sorts on coverage and
trust-rooting, not on README quality.

Library quality becomes mechanically measurable. ProofHashCount per
public function is a number. The market sorts.

## The supply chain becomes a chain of proof hashes

Today's supply chain attestation tools (Sigstore, SLSA, in-toto) tie
binaries to maintainer identities. They prove "this binary came from
this build process." They do NOT prove the binary CORRECTLY does what
its surface claims.

Tomorrow: the supply chain is a chain of proof hashes. Each link
attests:
1. **Source proof hash** — what the source claims
2. **Binary-proof-binding** — what bytes correspond to the source proof
3. **Hardware attestation** — what hardware the binary loaded into

Walking the chain validates every link. Tampering at any layer breaks a
hash. **The supply chain becomes the proof DAG.** Supply-chain security
is a special case of proof-DAG composition; no separate tooling needed.

## The runtime is the proof's execution

A running binary is the proof DAG's executable form. A monitor producer
observes its behavior in production. Each function call that satisfies
an invariant produces a memento — small, cheap, signed. Over a binary's
lifetime, billions of these accumulate.

Runtime mementos extend the DAG. The static proof said "for all x in
domain D, P(x) holds." The runtime evidence says "for these specific
calls in production, P held." The DAG grows monotonically with
operational evidence.

This is what "software ages backwards" means at the operational level:
**the proof DAG accumulates evidence indefinitely**. A 5-year-old
codebase's proof DAG includes 5 years of runtime confirmations. That's
weight you cannot fake.

When a runtime memento fires `verdict: violated`, it points DIRECTLY at
the source memento it contradicts. Debugging is graph navigation. The
counterexample is the input. The fix is "satisfy the invariant or
revise it." Mechanical.

## What this absorbs

The unification consequence: most software-trust concerns become
aspects of one substrate.

| Today's concern | In the unified model |
|---|---|
| Type checking | Producer minting "host-checker passes" mementos |
| Linting | Producer minting "lint passes" mementos |
| Test coverage | DAG walk: which mementos exist for which line |
| Code review | Producer minting "reviewer signed off" mementos |
| CI green light | Composite "all required mementos hold" memento |
| Supply chain attestation | Chain of proof hashes |
| SBOM | DAG walk listing dependencies' proof hashes |
| Reproducible builds | binary-proof-binding mementos require deterministic builds |
| Provenance tracking | DAG walk over content hashes |
| Compliance audit | DAG walk; the audit IS the walk |
| Migration safety | Proof-hash diff between current and target |
| Library discovery | Property-hash search |
| Library upgrade | Proof-hash diff with named violations |
| Versioning | Proof hash IS the version (semver becomes mechanical) |
| Bug fix | New memento referencing prior `verdict: violated` via inputCids |

All hash-equivalent. All walkable. All signed. All in one substrate.

## Software ages backwards

Each commit may add zero new invariants. But each commit's code
modifications get checked against the entire existing invariant DAG —
including every transitive dependency's DAG — automatically. New code
paths inherit coverage from every pre-existing invariant via shadow AST
walking.

Each library release may add zero new properties. But each release's
proof DAG composes with every dependent's proof DAG. The ecosystem's
verified surface grows with every release.

Each runtime second may produce no novel state. But every observed
function call adds a leaf to the DAG. The proof DAG grows with every
running process in the world.

The DAG **monotonically accumulates evidence**. The codebase becomes
more provably correct over time, not less. The normal direction (rot,
drift, forgotten constraints) is inverted because the DAG holds the
constraints, not the actors.

## The career-arc closing, restated

Each layer captured a domain by reducing it to a single hash:

| Year | Domain | Reduction |
|---|---|---|
| 1995 | Files | content hash |
| 1998 | File swarms | block hash + dedup hash |
| 2001 | Distributed file delivery | infohash |
| 2008 | Money | transaction hash |
| 2014+ | General content | CID |
| ~2009+ | Source code | commit hash |
| 2026 | **Correctness** | **proof hash** |

Each prior layer enabled the next. Files made dedup possible. Dedup
made swarming possible. Swarming made trustless distribution possible.
Distribution made money-as-hash possible. Money made smart contracts
possible. Smart contracts made... and now: **proof-as-hash makes
correctness-as-a-public-good possible.**

The arc closes at proofs because proofs are what makes everything
else trustworthy at scale.

## Naming

The artifact: **the proof DAG** (existing term, retained).

Its identifier: **the proof hash** (32-byte CID of the canonicalized
DAG root).

A binary's full identity: **(name, contentHash, proofHash)** — the
three-coordinate pin.

Search query language: **property-hash search** — find libraries by
what they prove.

Upgrade workflow: **proof-hash diff** — what changed in the contract
surface.

Audit workflow: **DAG walk** — the audit IS the walk.

These are the five operational terms. Document them once; use them
consistently.

## What this changes about the catalog work

The TS-kit's `parseInt.invariant.ts` files are not documentation. They
are the seed mementos of the global proof DAG. Their hashes will appear
in the inputCids of millions of downstream mementos forever.

Build the catalog FIRST. The lifter is plumbing; the catalog is the
genesis block.

The catalog's proof hash becomes the kit's primary deliverable. When
the TS-kit is published, its identity is `provekit-ts@1.0+contentHash+proofHash`.
Every TS project transitively depends on that proof hash. Every project's
audit walks through it.

Get the catalog right and it's a public good forever. Get it wrong and
the entire ecosystem inherits the bug. **The catalog files commit
TODAY, before the lifter ships, because they are the durable artifact
the framework's value flows through.**

## Two states

The complete trust posture, stated mechanically:

> **The code either does EXACTLY what it says — hashes all the way down
> to hardware — or the signature doesn't match.**

That's the entire architecture in one sentence. There is no third
state. No "probably correct." No "trusted maintainer." No "passing CI"
as a trust signal. No "looks fine to me" as a code-review verdict.

The hash chain composes — and it goes all the way down to physics:

```
project invariants (what this code claims about itself)
  ↓ composes via inputCids
inherited library proofs (every transitive dependency's DAG)
  ↓ composes via cross-equivalence mementos
kit catalog mementos (host-language built-ins, signed by kit authors)
  ↓ composes via specification references
spec leaves (ECMA-262, ISO C, POSIX, IEEE 754 — content-hashed)
  ↓ composes via standards-body attestation
language-runtime proof DAG (V8 has formal verification of hot paths;
                            JVM has its own; Wasm runtimes have theirs)
  ↓ composes via hardware-instruction-set semantics
CPU instruction-set verification (Intel, AMD, Apple, ARM publish formal
                                  models of their ISAs — content-hashed)
  ↓ composes via microarchitectural verification
silicon circuit-level proofs (formal verification of arithmetic units,
                              cache coherence, memory ordering — Intel
                              learned this lesson from Pentium FDIV in
                              1994; today every major vendor publishes
                              verification artifacts)
  ↓ composes via gate-level synthesis
transistor behavior models (verified empirically + via TCAD simulation;
                            the physical layer)
  ↓ composes via solid-state physics
semiconductor physics (content-hashed peer-reviewed papers, foundry
                       process attestations, charge-carrier behavior
                       at sub-nanometer scales)
  ↓ composes via fundamental physics
quantum mechanics (the standard model's mathematical formulation;
                   the substrate of transistor behavior)
```

**Every codebase's proof DAG eventually grounds out at physics.**

The chain is universal. From your shitty TypeScript invariant about
billing math, walking down the inputCids: through React, through V8,
through ECMA-262 + IEEE 754, through Intel's FPU verification, through
silicon-level circuit proofs, through transistor models, through
semiconductor physics, through quantum mechanics. Every codebase. Every
language. Every host. Same chain. Different leaves, same depth, same
grounding.

**The Pentium FDIV story (1994) is the historical proof-of-concept.**
Intel's chip computed division wrong. Every program that called FDIV
inherited the error. The fix was at the silicon. Intel learned: formally
verify the FPU. Every major chip vendor today publishes verification
artifacts for their arithmetic units. Those artifacts ARE leaves of
every codebase's proof DAG that does arithmetic. Every TS project
calling `5.5 + 3.2` transitively depends on Intel's (or AMD's, or
Apple's) FPU proof.

Either every link in this chain composes correctly under your
proofkit's adversarial re-verification, or the signature doesn't match.

**There is no middle ground.**

**Hardware vendors become producers in the framework.** The chip ships
with a proof DAG. The CPU's identity isn't just "Intel Xeon X5680" — it's
`Intel Xeon X5680 + sha256(silicon) + proofHash(verified ops)`. Three
coordinates, same as software. The hardware market reorganizes around
proof depth. Vendors compete on coverage of their ISA's verified
operations. "How much of your chip is formally verified?" becomes a
spec-sheet line item.

**Software's correctness is grounded in physics, mechanically.** Not as
philosophy. As hash-chain composition. When a regulator asks "is this
banking system correct?" — the answer is "walk the DAG from the running
CPUs all the way down to physical reality. Every link is content-
addressed and signed. The audit IS the walk." Compliance becomes a
mechanical proof traversal.

**This is the trust substrate of computation itself.** Not a software-
only system. ProvekIt is the content-addressing infrastructure that
lets EVERY VERIFIABLE CLAIM about ANYTHING compose into one DAG,
eventually grounded in physical observation.

The system is mechanically binary at every layer. Composing software
becomes composing hash chains. Trusting software becomes verifying hash
chains. Tampering anywhere — hardware, OS, binary, source, proof, kit,
spec — breaks a hash. The chain refuses to compose. The artifact is
rejected.

**Attack surface collapse:**

The only remaining attack vectors are:
- Compromise the hardware attestation root (Intel/AMD/Apple/Nitro keys)
- Compromise the spec leaves (rewrite ECMA-262)
- Compromise the canonical FOL definition itself
- Find a math error in the trusted kernel combinators

Each is an attack on the SUBSTRATE, not on individual artifacts. Each
requires breaking the math, the silicon, or the standards body — not
the maintainer, not the build system, not the package registry. The
attack economics shift from "compromise any single party" to
"compromise the substrate of computation itself."

This is not a hardening of the existing trust model. This is a
DIFFERENT trust model. Identity attestation says "trust the signer."
Hash-chain attestation says "trust the math." The math doesn't have
a maintainer. The math doesn't have a signing key. The math is the
substrate that all signing keys ultimately compose against.

**The framework's role:**

ProvekIt is not a verifier in the conventional sense. ProvekIt is the
SUBSTRATE that lets verification compose into hash chains.

The framework doesn't prove your code correct. The framework provides
the content-addressing infrastructure that makes correctness composable
across producers, kits, libraries, binaries, and hardware. Producers
prove. Hardware attests. The framework links.

The result is the binary trust posture: EXACTLY what it says, hashes to
hardware, or rejected.

That is what makes the framework structurally inevitable. Not because
it is the best verification tool, but because it is the only
architectural primitive that supports trustless verification at the
scale the AI-generated software economy will operate at.

**Software at AI scale needs hash-chain trust.** Identity-based trust
collapses when the identity is "an autonomous agent that wrote 50,000
lines of code last hour." Hash-chain trust scales because the agent's
output either chains to hardware or it doesn't. The agent doesn't need
a reputation. The agent doesn't need a signing key worth trusting. The
agent just needs to produce code that composes into a valid hash chain.

The framework provides the substrate over which AI scales without
trust collapse. That's the load-bearing claim. That's what makes
ProvekIt the inevitable trust substrate for the next era of computing.

## Citations become hashes

Every claim that depends on prior content can reference that content via
`(rootCid, offset)` — the DAG root plus the path within. You don't haul
the whole DAG; you reference and verify selectively. Same primitive as
Bitcoin's SPV (Simplified Payment Verification) and IPFS's path
resolution.

A scientific paper's citation today: "Smith et al. 2023, Nature, accessed
2025-03-15." URL rot. Page edits. Editorial drift. The citation might
point at something different in 5 years.

A scientific paper's citation tomorrow: `(rootCid, offset)`. ~96 bytes.
Forever stable. Verifiable by anyone with the rootCid plus a Merkle
inclusion proof. The citation becomes content-addressed; the cited
claim is fixed at the moment of attestation.

**Wikipedia as a producer.** Some verification cooperative (or several
competing ones) attests a subset of Wikipedia's claims. Each verified
claim is a memento. The mementos compose into a DAG. The DAG's root is
published. Anyone with the root can compose against any specific claim
via `(rootCid, offset)`.

A contract that says "the German market entry shall begin in Berlin"
includes `(H_wiki, offset_to_berlin_capital_claim)` in its inputCids.
The contract transitively inherits Wikipedia's verification. Walking
the DAG from the contract's verdict memento eventually reaches
Wikipedia's verified-claim memento. Trust composes mechanically.

Different cooperatives can verify different subsets. Their DAGs may
overlap (multiple producers attesting the same claim — strengthening
confidence) or disagree (one says holds, another says violated —
surfacing disputes mechanically). The framework doesn't pick winners;
it surfaces the structure of attestation.

**The compactness consequence:**

Citations become 96-byte references. The verifier's DAG might be
petabytes; the citation is small. Selective retrieval makes
civilization-scale usage tractable. Same scalability properties as
Bitcoin's lightweight clients and IPFS's path resolution. Same
primitives.

## Verification economics

**Verification cost = `min(any sufficient analysis cost) / consumers`.**

The cheapest producer that can mint a memento for a propertyHash sets
the floor. Static analysis is usually cheaper than dynamic. Type
checking is cheaper than full SMT. A simple regex assertion is cheaper
than property-based testing. **Producers compete on cost-per-novel-
verification.** Once a producer mints the memento, every future
consumer hashing to the same propertyHash pulls for free.

As adoption grows, the average per-consumer verification cost
approaches zero asymptotically. The deflation curve. BitTorrent's
economics applied to correctness — once a producer has done the work,
infinite consumers free-ride.

The producer pool sorts naturally:
- Cheap, high-volume producers (regex, type-check, simple LLM eval)
  handle 99% of routine verification
- Expensive, low-volume producers (frontier LLMs, formal SMT, formal
  proof) handle the genuinely novel 1%

The framework's economic logic flips today's AI compute curve.
Today: more queries = more revenue per query. Tomorrow: more queries
= less revenue per query (most hit cache). The bottleneck shifts from
compute to NOVELTY. The infrastructure that captures value is the one
that COORDINATES the swarm, not the one that serves the bytes.

## Commercial truth claims

Once the substrate is operational for software, it absorbs commercial
truth claims by the same primitive. **"Pepsi is better than Coke"
stops being marketing and becomes a propertyHash.**

```
P_pepsiPreferenceOverCoke =
  hash("in blind double-blind taste tests at sample size ≥ 10000,
        with ISO-accredited methodology,
        statistical significance p < 0.01,
        more than half of testers prefer Pepsi over Coke")
```

The propertyHash is precise, content-addressed, public. The producer
(Pepsi, or an independent firm Pepsi commissions) runs the test. Each
test is a memento. The aggregate memento says "verdict: holds for
P_pepsiPreferenceOverCoke." The DAG composes to ISO accreditation
mementos, calibrated instrument mementos, statistical methodology
mementos.

**Coke can mint counter-mementos.** Their own labs, their own studies,
attesting different verdicts (or attesting different propertyHashes
that define "preference" under different conditions). The DAG composes
both. Consumers see the disagreement. Different proofkits weight the
mementos differently — by funding source, by methodology rigor, by
accreditation level.

**The framework doesn't pick a winner.** It surfaces the STRUCTURE of
the attestation. Walk the DAG; weight the producers; see the funding
source; see the methodology; see the rigor. Marketing becomes
archaeology of evidence.

**The implications:**

- Vague marketing ("America's favorite!") gets crowded out by precise
  marketing ("verdict: holds for propertyHash H, witnessed by N
  independent labs, signed by W").
- False advertising becomes mechanically falsifiable. The FTC
  investigation is a DAG walk.
- Comparative advertising becomes precise. "Better" is bound to a
  specific propertyHash. The bindingHash is the comparison; the
  propertyHash is the metric.
- Brand wars become proof wars. Companies compete on the depth and
  rigor of their proof DAGs.

The cola wars don't end because someone wins. They end because
unsubstantiated claims stop being viable. **Companies compete on
proof depth, not on rhetoric.**

## Scalability

**An exabyte-scale DAG is manageable at every level.** The architectural
property that makes this work: a 32-byte CID encodes a reference to any
node in the DAG, regardless of the DAG's total size. A complete claim
envelope reference — `(rootCid, propertyHash, bindingHash)` — fits in
64 bytes. Plus signature, verdict, producer identity, the whole envelope
is well under 256 bytes.

256 bytes encodes the entire chain.

The DAG itself might be petabytes or exabytes globally — billions of
claims composed across decades of verification work. But:

- **No single node holds the whole DAG.** Each consumer holds only the
  roots they care about plus the sub-trees they've pulled on demand.
- **Selective retrieval scales sub-linearly.** A specific claim's
  verification requires walking O(log n) hashes via Merkle inclusion
  proofs, not the full graph.
- **The chain is built incrementally.** Each claim's verification work
  happens ONCE, by some producer at some moment. After that, the claim
  is pure reference — `(rootCid, offset)`, 96 bytes, eternally valid.
- **The DAG grows monotonically by accretion.** New claims add new
  leaves; old claims stay valid forever. The graph never re-computes;
  it only extends.

This is the same scalability property as Bitcoin (a lightweight client
holds only block headers and selective transaction proofs, not the full
blockchain) and IPFS (a node holds only the CIDs it pins or has
recently fetched, not the entire content network).

The substrate becomes manageable at civilization scale because:

- The CHAIN OF VERIFICATION is built once per claim. Done forever.
- The REFERENCE is small (32 bytes per CID).
- The RETRIEVAL is selective (Merkle paths, not full traversal).
- The DAG GROWS as things become verifiable — not all at once, but
  incrementally, organically, as producers attest claims they can
  verify.

A scientific paper from 2050 references claims attested in 2026. The
2026 mementos are still valid; their verification work was done once.
The 2050 paper composes (rootCid, offset) references; 64-96 bytes per
citation. The verification chain from the 2050 paper to its 2026
ancestors walks a few thousand hashes, not the full DAG.

**The exabyte DAG fits on every laptop because the laptop only needs
the roots it cares about plus the paths it walks.**

## The substrate's reach

The architectural primitive — content-addressed hash-and-trust with
adversarial re-verification — applies wherever:

- The artifact's behavior can be measured and content-addressed
- Producers can attest claims about it
- ZK proofs can attest classification without revealing private content
- Hardware can attest to physical observations

That set includes essentially everything in the digital age, modulo
what can be measured. The substrate is the trust layer for:

- **Software correctness** (v1 — what we're building)
- **Library trust and supply chain** (immediate downstream)
- **Hardware attestation** (chain extension to physics)
- **Knowledge claims** (Wikipedia subset, scientific papers)
- **Industrial certification** (pharma, materials, batteries — see
  `2026-04-29-zk-verification-economy.md`)
- **Commercial truth** (marketing claims, comparative advertising)
- **Identity, governance, supply chain, AI provenance** — all
  downstream consequences of the v1 substrate

These are not parallel concerns. They are the SAME concern at
different scales. The framework's value cashes out in software
first; the broader implications follow because the architectural
primitive is universal. Discipline matters: lead with software;
let downstream consequences accumulate as adoption locks in.

Bitcoin's discipline — "electronic cash," not "blockchain for
everything" — is what made the core thesis durable. ProvekIt's
discipline is the same: software correctness, not "trust substrate
for everything verifiable." The latter follows from the former.

## What's durable

**The codebase is not durable. The spec is.**

Every line of TypeScript currently in `src/` will be rewritten — once,
twice, ten times — over the next decade. New implementations will
emerge in Go (ProvegIt), Rust (ProverIt), Mojo, Zig, languages that
don't exist yet. Optimization passes will replace whole modules.
Architectural improvements will refactor everything. Future maintainers
will read today's code as historical artifact.

What survives every rewrite:

- **The wrapper schema** (universal claim envelope) — the memento shape
- **The canonical FOL form** (AST canonicalizer spec) — the byte-identical hash construction
- **The CID construction** (deterministic, host-language-agnostic)
- **The producer interface** (Stage / Action contracts)
- **The kit standard** (per-language responsibilities)
- **The catalog format** (`.invariant.<lang>` files as universal contracts)
- **The trust posture** (adversarial re-verification; chain to physics)
- **The architectural primitive** (content-addressed hash-and-trust)

These live in `docs/specs/`. They are the framework. The code in `src/`
is the FIRST IMPLEMENTATION OF the framework — load-bearing for
operational existence today, replaceable in ten years, irrelevant in
fifty.

**The mementos minted under the spec are durable across every
implementation rewrite.** A memento minted in 2026 by today's
TypeScript code, with content hash H, will validate against a 2046
Mojo rewrite's verifier — because both implementations consume the
same canonical FOL form, produce the same wrapper schema, build the
same CID. Hash-equivalence across implementations IS the durability
property.

**This mirrors every prior layer of the career arc:**

- The Bitcoin SPEC is durable; Bitcoin Core's C++ is not. Knots, btcd,
  bcoin, brd — all valid implementations of the protocol. Each
  rewriteable. The protocol survives.
- The HTTP SPEC is durable; nginx/Apache/Caddy are not. Three decades
  of implementations; the spec persists.
- The TCP SPEC is durable; every OS's network stack is not. Implementations
  rewritten dozens of times; TCP unchanged.
- The Git SPEC is durable; libgit2/jgit/gix/dulwich are not. Multiple
  implementations across languages; the data model persists.
- The IPFS SPEC is durable; go-ipfs/Helia/iroh are not. Implementations
  reimagined repeatedly; CIDs compose forever.

ProvekIt's spec inherits this property by construction. Anyone can
reimplement the framework. The reimplementation produces identical
mementos because the canonical form is specified. The mementos compose
across every implementation that has ever existed or will ever exist.

**"Correctness is a signed DAG" is the durable architectural primitive.**

Not the lifter we'll write tomorrow. Not the prover we'll wire up next
week. Not the CLI we'll ship next month. Those are decorations on the
durable thing. The durable thing is the spec — the architectural
identity, the trust posture, the canonical hash construction, the
universal claim envelope.

**Implications for how this session's work is valued:**

- The 1311-line `2026-04-29-ts-ir-language.md` spec is durable. The
  lifter that implements it, when written, is decorative.
- The 700+-line `2026-04-29-correctness-is-a-hash.md` spec is durable.
  The CLI that implements `provekit prove` is decorative.
- The catalog seeds (`parseInt.invariant.ts`, `Math.invariant.ts`) are
  durable. They are the genesis-block mementos of the global proof DAG.
- The TypeScript implementations in `src/canonicalizer/`, `src/ir/`,
  `src/claimEnvelope/`, `src/producerKeys/` are decorative. They prove
  the spec is operational. Future implementations replace them.

**The spec is the framework. The code is the framework's current
shadow.**

Build the spec rigorously. Ship the code as the operational existence
proof. Let the spec outlive every implementation, because that is what
the architecture's durability claim CASHES OUT TO. Implementations are
forks of the spec; mementos compose across forks; the spec is the
constitutional document; the codebase is current administration.

This is what makes ProvekIt structurally inevitable. Not because we
build a great codebase. Because the architectural primitive — content-
addressed hash-and-trust applied to correctness — is what every future
verification system necessarily converges to. We're writing the spec
that those systems will inherit. The spec is the durable contribution.

## What this is for

A reader who understands this document understands that ProvekIt is:

- A trust substrate where correctness is content-addressable
- A market where the proof hash is the unit of trade
- A supply chain where the proof DAG is the chain
- A runtime where the binary is the proof
- A protocol where verification is hash composition

Each piece independently evolvable. Every layer's protocol outlasts
every layer's current implementation.

**The customer-facing thesis is the two sentences at the top:**

> **Correctness is just a hash.**
>
> **A binary is a signed DAG of correctness.**

Everything in this document is the architecture that makes those two
sentences operationally true. Vibe coding becomes safe by default
because verification is hash-pinned. Constraint coding becomes the
default mode because the gate is mechanical. Programming becomes
specification because the IR is the source. The proof tree is the
codebase's durable identity, surviving every prover that ever
contributes a leaf.

That is what makes software age backwards.
