# Cold-start: the bootstrap problem, addressed honestly

Sugar's headline metric, the hash-discharge fraction at Tier 1, depends on the lattice of cached implications being well-populated. A populated lattice means the typical `(post, pre)` pair the verifier sees has already been discharged by some earlier verifier; the current run discharges it for free.

An empty lattice means every pair falls through to Tier 3 (Z3 invocation), which is slow. Early adopters land here. This document is honest about the cold-start problem and the path through it.

## What "cold start" means concretely

Imagine three milestones in a project's adoption of Sugar:

### Day 1: empty lattice

The project just adopted Sugar. No mementos exist for any of its dependencies. Every contract is novel. Every `(post, pre)` pair the handshake tries triggers Tier 3.

Discharge breakdown looks like:

```
total call sites:        47
discharged by hash:       0    (0%)
discharged by cache:      0    (0%)
discharged by solver:    47    (100%)
```

The verifier runs Z3 47 times. Each invocation takes seconds. The first build takes minutes. Users see "is this thing actually fast?"

This is the worst case. Every adopter starts here.

### Day 30: warming lattice

After the first month, the project's verifier has minted dozens of implication mementos for the `(post, pre)` pairs it sees regularly. Subsequent builds discharge most of those at Tier 2 (signature check, sub-millisecond):

```
total call sites:        47
discharged by hash:       0    (0%)
discharged by cache:     35    (74%)
discharged by solver:    12    (26%)
```

Tier 3 is now the residue, not the average case. Builds are faster but Z3 still runs for the genuinely-novel pairs.

### Day 365: well-warmed lattice

A year in, the lattice has been seeded by hundreds of users of common dependencies (assuming the implication-server pattern is in wide use). Even genuinely-novel pairs in this project tend to have already been discharged by someone else's verifier. The current run pulls them from the implication server and discharges at Tier 2.

Hash-discharge fraction approaches the theoretical asymptote (typically 80-95% for healthy projects):

```
total call sites:        47
discharged by hash:      40    (85%)
discharged by cache:      6    (13%)
discharged by solver:     1    (2%)
```

The protocol's amortization goal manifests at this stage. Tier 1 CID equality
is the hot path; Tier 3 semantic proving is rare.

## The bootstrap challenge

The above trajectory only happens if **adoption is spread across enough independent users** that the implication server (or shared lattice) accumulates results from many projects. A single isolated user never reaches Day 365's discharge fraction; they reach Day 30 and stay there.

This is the bootstrap problem. Sugar's value compounds with adoption. Adoption requires value. Closed loops of "you should adopt because everyone else has" do not generate first adopters.

## What works (and what doesn't)

### Works: reference contracts curated centrally

The reference contracts library ([`reference-contracts/`](../reference-contracts/) when written) is the bootstrap accelerant. A curated set of canonical bridge anchors (`ref-parseInt-v1`, `ref-email-format-v1`, `ref-uint32-arithmetic-v1`, etc.) gives every adapter a target.

When the JavaScript `parseInt` adapter and the Rust `parse` adapter both bridge to `ref-parseInt-v1`, every codebase that uses either function and depends on the other gets cross-language Tier-1 discharge from day one, because the lattice was pre-populated by the curated reference contracts.

This is why the polyglot demo is the load-bearing piece (see [`docs/tutorials/polyglot-stack.md`](../tutorials/polyglot-stack.md)). It's not just a tutorial. It's an existence proof that the cold-start can be broken with curated bridge anchors.

### Works: kit-self-contracts as initial seed

Every kit ships with a self-contracts package. The self-contracts include a small canonical set of contracts about basic IR primitives (eq, lt, gt, atomic predicates over Int and String). These are seeded into every adopter's lattice on day one.

This handles the most common call sites (comparisons, basic numeric checks, string predicates) at Tier 1 from the very first build. Discharge fraction at Day 1 is not actually zero; it's typically 10-15% because basic primitives are pre-seeded.

### Works: published `.proof` files in package registries

When `lodash` (or any popular dependency) ships a `.proof` alongside its npm package, every consumer who upgrades inherits all of `lodash`'s contracts. The lattice grows transitively as upstream projects adopt.

This is the path of least resistance: a few tens of widely-depended-upon packages adopting Sugar seed the lattice for thousands of downstream consumers.

### Works partially: implication servers

A passive indexer that aggregates implication mementos from many projects. The first project in the world to discharge `(post-X → pre-Y)` mints the implication memento and pushes it to the server. The second project that hits the same pair pulls the memento from the server and discharges at Tier 2.

This works when there's an active server with broad participation. It does not work when projects are siloed.

### Doesn't work: pure adapter coverage

It would be tempting to think "if every annotation library has a lift adapter, the cold-start is solved." It isn't. Lift adapters produce *contracts*; they don't produce *implications*. The lattice's discharge work is on implications, not contracts.

A lattice with 100,000 contracts and 0 implications has the same discharge fraction as a lattice with 100 contracts and 0 implications: zero at Tier 1, full Tier 3 fallback.

The bootstrap is an *implication* problem, not a *contract* problem. Solving the cold-start means seeding implications, not contracts.

### Doesn't work: trying to discharge everything ahead of time

Naively, "let's pre-compute every implication that could ever be needed" sounds appealing. It is computationally infeasible. The space of `(post, pre)` pairs is enormous; Z3-discharging every plausible pair would consume more compute than every cryptocurrency mining operation combined and still wouldn't cover the actual pairs users hit.

The protocol's design implicitly accepts this: lazy minting, bounded scope, just-in-time discharge. The cold-start is a feature, not a bug; eager discharge wouldn't terminate.

## What the protocol's design buys

Despite the cold-start, the protocol's design is correctly shaped for amortization:

- **Each (post, pre) pair is discharged at most once globally.** Across all users, all projects, all languages.
- **Discharge results are content-addressed and signed.** They are portable.
- **The lattice grows monotonically.** Once a pair is discharged, it stays discharged forever (assuming the original signer's key remains trusted).

So the asymptote (Tier 1 fraction approaches 95%) is reachable. The question is just how many adopters and how much elapsed time it takes to fill the lattice.

## Honest expectations for early adopters

If you adopt Sugar in 2026 (today), here's what's realistic:

- **Day 1**: 10-15% Tier 1 (kit self-contracts seed). Most call sites Tier 3. Builds are slow.
- **Day 30**: 50-70% Tier 1+2 within your project. Builds are moderate.
- **Day 90**: 70-85% within your project, *if* you publish to and pull from a shared implication server.
- **Year 1**: 85-95% if the ecosystem has grown to a few dozen projects sharing implications. Otherwise, plateau at Day 90.

The ceiling is not the protocol's; it's the network effect.

## Suggested adoption pattern for early adopters

1. **Adopt internally first.** Use Sugar within a single team / project / company. Build up a private implication server. Internal cold-start is faster than ecosystem cold-start.
2. **Publish to a public implication server when ready.** Once your team has working `.proof` flows, contribute mementos to a shared server. This helps every other adopter and accelerates ecosystem cold-start.
3. **Bridge to reference contracts.** Wherever possible, bind your implementations to canonical bridge anchors. Cross-language transfer happens for free once bridges exist.
4. **Be patient.** The protocol's value proposition is asymptotic. Early adopters bear bootstrap cost; they receive the long-term reward of having shaped the substrate.

## When the cold-start is broken

The cold-start is "broken" (i.e., new adopters reach high Tier 1 fractions on day one) when:

- Most popular packages in major language ecosystems ship `.proof` files.
- A public implication server is widely used and contains millions of cached pairs.
- A curated reference-contracts library covers the common cross-language bridge points.

This is the network-effect milestone. Years out, plausibly. Sugar's protocol design is correctly shaped to support this scenario; the actual achievement of it is a social and adoption question, not a technical one.

## What this section concedes

Sugar does not solve the cold-start problem alone. It provides a substrate that, given adoption, accelerates over time. Early adopters pay the bootstrap cost and receive the long-term reward of having shaped the substrate. Late adopters get the asymptote for free.

This is honest. The thesis is monotonic provability and amortizing solver cost across the dependency graph; the thesis is achieved at the asymptote. Early adopters do not see the asymptote.

The decision to adopt early is a bet on the protocol reaching the asymptote; a bet that adoption will compound. Adopt knowing this.

## Read next

- [thesis.md](thesis.md): the central claim.
- [boundaries.md](boundaries.md): what Sugar is not.
- [../tutorials/polyglot-stack.md](../tutorials/polyglot-stack.md): the cross-domain bootstrap accelerant.
- [../reference-contracts/README.md](../reference-contracts/README.md) (when written): curated bridge anchors.
- [../security/threat-model.md](../security/threat-model.md) (when written): what trust looks like in a partially-populated lattice.
