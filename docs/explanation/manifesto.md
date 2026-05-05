# ProvekIt: Correctness is just a hash

> A manifesto for the trust substrate of computation.

## The thesis

> **Correctness is just a hash.**
>
> **A binary is a signed DAG of correctness.**

Software's correctness, the property that code does what it claims to do,
becomes a content-addressed artifact. A 32-byte hash. Composable. Durable.
Verifiable mechanically by anyone who runs the math.

Whoever publishes a binary publishes its proof DAG alongside it. Whoever
consumes the binary re-verifies the DAG with their own proofkit, under their
own producers, against their own kit catalogs. The signature attests to
identity; the consumer's re-verification attests to validity.

There is no third state. The hash chain composes: bytes, source, proofs,
spec leaves, hardware attestation, all the way down to the silicon. Or it
doesn't. Either the code does exactly what it says, or the signature doesn't
match.

## What we replace

We replace nothing. We absorb.

Type checkers, linters, test runners, formal provers, code reviewers, CI
systems, deployment gates, audit traversals, package registries, dependency
managers, supply-chain attestation tools, runtime monitors, hardware
attestation chains; these existing trust artifacts do not get displaced.
They become producers in a unified substrate. Each emits content-addressed
mementos signed by its own identity. The substrate composes them into one
DAG. The DAG IS the codebase's correctness. The DAG IS the audit trail.
The DAG IS the supply chain.

Trust today is a patchwork: trust the maintainer, trust the build pipeline,
trust the type checker, trust the auditor, trust the package signer. Each
trusted authority is a separate root. Each is breakable independently.
Compromise any one and the chain collapses.

The substrate replaces patchwork trust with hash composition. The "authority"
of any signer is metadata. The validity of any claim is mechanical. You
verify the hash chain or you don't. If you don't, the artifact is rejected.

## The architectural primitive

The same architectural primitive (content-addressed hash-and-trust with
producer fungibility) has ridden through five prior domains:

| Year | Domain | Reduction |
|---|---|---|
| 1995 | Files | content hash |
| 2001 | Distributed file delivery | infohash |
| 2008 | Money | transaction hash |
| 2014+ | General content | CID |
| ~2009+ | Source code | commit hash |
| 2026 | **Correctness** | **proof hash** |

Each layer captured a domain by reducing it to a single hash. Bitcoin's
contribution was money-as-hash. IPFS was content-as-hash. Git was
source-as-hash. **ProvekIt is correctness-as-hash.**

The proof hash is the canonical correctness identifier of any artifact.
Two artifacts with the same proof hash claim the same set of properties.
Verifiable by hash comparison; no source inspection needed.

## How it works

**The host language is the IR.** You don't author invariants in some custom
syntax. The IR is a typed subset of whatever language you already write in.
TypeScript shop → invariants in TypeScript. Rust shop → invariants in Rust.
COBOL shop → invariants in COBOL. The TypeScript compiler is the proof
checker; the Rust borrow checker is the proof checker; each host language's
checker IS the gate that decides which expressions are well-formed
propositions about the code.

**Constraint by design, not contract by design.** Production code stays
bit-identical to whatever it was before the framework arrived. Constraints
attach EXTERNALLY in dedicated `.invariant.<lang>` files that reference
the existing code by name. Removing every invariant file leaves the project
exactly as it was. A 40-year-old COBOL banking system can be made provably
correct without a single line of COBOL changing.

**Adversarial re-verification.** When you install a library, your proofkit
re-runs the library's published proof DAG under your own producers. The
maintainer's signature tells you WHO claimed; your proofkit tells you
WHETHER IT'S TRUE. Trust no one. Verify everyone.

**Producer diversity is the trust mechanism.** Five 7B models agreeing is
more trustworthy than one frontier model alone. Z3 plus Soufflé plus tsc
plus three independent LLMs all agreeing is more trustworthy than any
one of them alone. The framework's trust scales with producer diversity,
not producer power.

**The DAG accumulates evidence indefinitely.** Each commit, each release,
each running second of every deployed binary adds leaves to the proof
DAG. The codebase becomes more provably correct over time, not less.
The normal direction of software (rot, drift, forgotten constraints) is
inverted because the DAG holds the constraints, not the actors.

**Software ages backwards.**

## What this enables

**Vibe coding becomes safe by default.** The dumbest LLM can write
TypeScript. The same LLM can write invariants for the same TypeScript.
Even shitty invariants raise the correctness floor. Once the invariants
exist, they constrain every future code path through shadow AST walking,
including paths the original LLM never imagined. The framework doesn't
make LLMs smarter. It makes the GATE mechanical.

**Library upgrades become proof-hash diffs.** Upgrading lodash 1.x to 2.x
is no longer "run your tests and hope." It's a precise mechanical diff
between two contract surfaces, surfaced before the upgrade lands, listing
every callsite that's about to fail and exactly why.

**Library discoverability becomes property-hash search.** "Show me every
package whose proof DAG contains a memento with this propertyHash."
Discover libraries by what they prove, not by what they're named.

**Supply chain attacks structurally collapse.** Tampering at any layer
(hardware, OS, binary, source, proof, kit, spec) breaks a hash. The chain
refuses to compose. The artifact is rejected. The remaining attack surface
is the substrate of computation itself: silicon, spec leaves, the math.

**Compliance becomes mechanical.** A regulator asks "is this banking
system correct?" The answer is "walk the DAG from the running CPUs to
the spec leaves. Every link is content-addressed and signed. The audit
IS the walk."

**AI-generated code becomes adoptable at scale.** An autonomous agent
writing 50,000 lines of code per hour cannot have a reputation worth
trusting. It doesn't need one. The agent's output either composes into
a valid hash chain or it doesn't. Identity-based trust collapses at AI
scale; hash-chain trust scales because the math doesn't have a maintainer.

## What's durable

The codebase is not durable. The spec is.

Every line of TypeScript currently in `src/` will be rewritten (once,
twice, ten times) over the next decade. New implementations will emerge
in Go, Rust, Mojo, languages that don't exist yet. The current code is
the FIRST implementation, the operational existence proof. It is
replaceable.

What survives every rewrite: the wrapper schema, the canonical FOL form,
the CID construction, the producer interface, the kit standard, the
catalog format, the trust posture, the architectural primitive. These
live in `docs/specs/` and ARE the framework.

Mementos minted under the spec compose across every implementation that
has ever existed or will ever exist. Hash-equivalence across
implementations is the durability property.

The Bitcoin SPEC is durable; Bitcoin Core's C++ is not. The HTTP SPEC is
durable; nginx is not. The TCP SPEC is durable; OS network stacks are
not. ProvekIt inherits this property by construction. The spec is the
framework. The code is the framework's current shadow.

## What this generalizes to

The architectural primitive is bound to HASHABILITY, not to software.

Anything you can SHA-256 hash becomes a value proposition that can be
verified mechanically once, rolled up into a set of larger propositions,
and assumed forever via a pointer: this DAG, this address.

Software is the FIRST DOMAIN where the framework's value cashes out.
It is not the only domain. The same primitive applies to scientific
papers (peer review as memento), legal contracts (counsel attestation
as memento), financial audits (auditor signature as memento), media
authenticity (camera attestation as memento), AI provenance (training
data attestation as memento), supply chain (manufacturing attestation
as memento), and beyond.

Each domain's existing trust authorities (peer reviewers, auditors,
courts, regulators) become producers. The substrate gives them a
content-addressed memento format to sign. The DAG composes their
attestations across domains. Cross-domain claims become DAG walks.

This generalization is downstream of the core thesis. The framework's
load-bearing claim is correctness-as-hash for software. The
generalization to other domains is what the architectural primitive
makes possible, not what we're building first.

## What this is for

ProvekIt is the trust substrate for software in the AI era. The
substrate that makes:

- AI agents trustable at scale (because their output composes into hash
  chains, not into reputations)
- Open-source dependencies safe at scale (because the proof DAG
  composes through the entire dependency tree)
- Regulated industries adoptable for vibe-coded software (because the
  audit IS the DAG walk, mechanical and complete)
- Legacy code preservation tractable (because constraint-by-design lets
  COBOL stay COBOL while accumulating verification)
- Software correctness a public good (because hashes compose globally
  and same-hash means same-verdict everywhere)

The framework's external surface is almost nothing. A library. A CLI.
An optional language server. Optional swarm distribution. No SaaS. No
central registry. No "log in to ProvekIt." The framework is
infrastructure installed locally that runs against existing developer
workflows and exchanges content-hashed mementos through whatever
distribution channels users already have.

You install it as a git hook. The git hook produces mementos. The
mementos travel with commits. CI consumes them. Deploy gates on them.
Audit walks them. Package registries ship them. Dependency managers
compose them. Seven tiers of capture by selling exactly one thing
(the hook) and letting artifact propagation do the rest.

## Closing

The architectural primitive (content-addressed hash-and-trust with
producer fungibility and adversarial re-verification) has ridden
through five prior domains and arrived at the one that verifies all
others.

Files made dedup possible. Dedup made swarming possible. Swarming made
trustless distribution possible. Distribution made money-as-hash
possible. Money made smart contracts possible.

**Correctness-as-hash makes the AI-era software economy possible.**

Two sentences. Everything in this manifesto, every spec under
`docs/specs/`, every line of code in `src/`, is the architecture that
makes those two sentences operationally true.

> **Correctness is just a hash.**
>
> **A binary is a signed DAG of correctness.**
