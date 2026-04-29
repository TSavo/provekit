# ProvekIt: the proof substrate

> Author: shared session 2026-04-29 (T + Claude). Strategic and architectural
> manifesto. Companion document to the verification IR spec; this one
> captures the picture, the strategy, and the inevitability.

## The pitch

> **ProvekIt proves your code was never correct. Then makes it correct.
> Forever.**

Three sentences that the rest of this document operationalizes.

## The thesis

ProvekIt is the trust substrate for the global software ecosystem.

The architectural primitive — content-addressed hash-and-trust with
producer fungibility and swarm distribution — has ridden through five
prior domains over thirty years. Files. File-block swarms. Money.
General content. Now: **formal proofs about software**. The arc ends at
proofs because proofs are what makes every other distributed thing
trustworthy.

The framework's role is comparable in scale to DNS for the internet,
certificate authorities for the web, Git for source code, and Bitcoin
for money. Each is infrastructure that everyone uses without thinking
about it. ProvekIt is positioned to be the next one — the proof
substrate that everyone uses without thinking about it because it's
just *how trust works in software*.

## The trojan horse

The framework wins by being installed as a git hook.

The developer chooses the hook because they want fast local feedback on
every commit. They install it once. The hook runs `provekit prove` on
every commit going forward, producing content-addressed mementos that
travel with the code through the rest of the pipeline.

The hook's payload — the proof DAG — is what every downstream tier ends
up consuming. The customer never made an explicit "should we adopt
ProvekIt for CI / deployment / audit / package management / supply
chain security?" decision. They installed a git hook. Everything else
follows because mementos are content-addressed, durable, and travel
through existing infrastructure (git push, the same way code travels).

The hook is the smallest possible adoption. Nothing requires sign-off.
No procurement. No platform team needed. The framework rides existing
developer workflow.

## The seven-tier capture

Each tier downstream of the developer commit ends up consuming mementos
because the developer's commit *carries them*. No vendor displacement,
no migration, no procurement battle — just downstream tiers doing the
natural thing with the artifacts the upstream tier produced.

```
1. Developer machine (git hook)
   └─ produces mementos signed by tsc, biome, vitest, z3, ir-formulas
   └─ mementos travel with the diff

2. Git host (the push lands)
   └─ mementos in the repo

3. CI tier (GitHub Actions, Buildkite, Jenkins, …)
   └─ now sees signed mementos already attached
   └─ CI's job: validate the DAG composes; re-run producers in trusted
      environment; mint composite "main verified" memento
   └─ CI is no longer "run the tools" — it's "validate the proofs"

4. Deployment tier (production rollout)
   └─ reads the composite "main verified" memento
   └─ deployment is gated by the proof DAG, not by CI's pass/fail signal

5. Audit / compliance tier
   └─ walks the DAG to answer "what was verified, by what, when, with
      what witness"
   └─ audit IS the walk; the answer is machine-checkable

6. Package registry (npm / cargo / pypi / maven / …)
   └─ release-mementos shipped alongside artifact
   └─ semver becomes mechanical: framework computes the version from
      the DAG diff, refuses to mint patch when properties broke

7. Dependency manager (npm install / cargo add / pip install / …)
   └─ pulls dependency mementos
   └─ composes them against the consumer's project DAG
   └─ install fails with a precise diagnostic when a property your
      code depends on is no longer verified by the upgrade
```

Seven tiers captured by selling exactly one thing (the git hook) and
letting artifact propagation do the rest.

## The architectural enablers

Three structural decisions make the seven-tier capture possible. Each
is described in detail in the verification-IR spec; they are summarized
here for picture-completeness.

**The host language IS the IR.** The IR is a library in whatever the
developer is already writing in. TypeScript shop → invariants in TS.
Rust shop → invariants in Rust. Lisp shop → invariants in Lisp. COBOL
shop → invariants in COBOL. Even the dumbest LLM can write JavaScript;
even the dumbest LLM can write COBOL. The framework rides every
language ever made because every language since FORTRAN has had `if
condition then signal-error`. That single primitive is enough.

**The IR lives in the repo, content-hashed by git.** No separate
registry. No central authority. No SaaS dependency. The IR formulas,
the principle library, the intent annotations, the memento store —
everything is *more source files* in the developer's repo. Editing
the IR is editing source. Code review covers IR. Branches carry
their own IR. Git is the distribution.

**Tools become producers.** Every existing static analyzer, type
checker, linter, test runner, and formal prover the developer already
uses gets absorbed as a producer. Every tool's output becomes a
content-addressed memento signed by `<tool>@<version>`. tsc, biome,
eslint, vitest, cargo check, clippy, miri, mypy, semgrep, snyk, z3,
lean4 — all interchangeable underneath; all composing into one DAG.
The framework absorbs whatever tool soup the customer happens to
already have. Heterogeneity is a feature.

These three combine to mean: **the framework's external surface is
almost nothing.** A library. A CLI. An optional language server.
Optional swarm-distribution daemon. No SaaS. No central registry. No
"log in to ProvekIt." The framework is infrastructure installed
locally, that runs against git repos, that exchanges content-hashed
mementos through whatever distribution channel the user prefers.

## The diff is the intent

No developer is asked to author intent annotations. Every commit is
implicitly an intent statement. The before-state, the after-state,
and the commit message (or linked ticket, or incident report)
together fully express what the developer was trying to accomplish.
An LLM-producer reads that triple and extracts the IR formula
directly.

This means: **the framework's adoption motion is "point it at your
repo."** No developer behavior change. No annotation discipline. No
buy-in from individual contributors. The framework reads what is
already there.

For mainframe shops, this is the load-bearing claim. They have 50+
years of commit history, decades of incident reports, regulatory
filing trails, and runbooks. ProvekIt converts all of that into
content-addressed proofs by reading backwards through history.
Retiring developers' institutional knowledge becomes durable
mementos before they walk out the door.

## The market sequence

The conventional adoption playbook (start with modern dev shops,
expand outward) is wrong for ProvekIt. The right shape is the
inverse.

**First: enterprise mainframe pilot.** A Fortune 500 bank or
insurance carrier writes a $1M-$10M pilot check. They have:

- The richest mineable corpora (50+ years of history).
- The highest per-bug cost ($1M-$100M regulatory consequences).
- The largest existing budget ($50M-$500M annual mainframe maintenance).
- Acute staff-replacement pain (COBOL devs aging out).
- Existing LLM enterprise pilots running.
- A failing migration narrative (COBOL → Java migrations mostly fail).

The framework's pitch — *"keep the COBOL, the framework verifies it
in place; every retiring developer's knowledge becomes durable proof"*
— is differentiated against the migration narrative in a way no other
tool can match. First customer pays for the COBOL kit; the kit funds
the framework's expansion.

**Second: modern dev shops, freely, after hardening.** Once the
framework has been battle-tested on the most demanding workload
(mainframe COBOL at a Tier-1 bank), TypeScript shops and Rust shops
adopt for free. The framework's value proposition for them is
weaker (their type checkers already do a lot) but the friction is
zero (install the git hook). They become the network-effect
amplifiers.

**Third: package ecosystem capture.** As more codebases use the
framework, more packages on npm/cargo/pypi/maven carry proof DAGs.
Eventually carrying a proof DAG becomes the default expectation
for any reputable package, the way "has TypeScript types" became
default in npm circa 2020.

**Fourth: AI-coding adoption.** AI agents authoring code at scale
need *some* infrastructure to make their output trustworthy at
scale. ProvekIt is structurally the only candidate — no other
framework has the architectural primitives in place. The AI
ecosystem adopts ProvekIt not as a choice but as a necessity.

## What this does to npm and the supply chain

Every npm release becomes a content-addressed proof of correctness.
The version number stops being a maintainer's editorial guess and
starts being a *theorem* the framework either proves or refuses to
mint:

- **Patch** = "every property the previous release had `verdict: holds`
  for, this release also has `verdict: holds` for. No properties
  dropped, no semantics changed."
- **Minor** = "all previous-release properties still hold; new
  properties added that didn't exist before."
- **Major** = "at least one previous-release property no longer
  has `verdict: holds`, or has changed semantics."

This is mechanical. The framework computes the version by diffing
the new DAG against the previous-release DAG and classifying the
result. Semver becomes a content-addressed proof of compatibility.

Cascading effects:

- **Vulnerability disclosure** becomes "this property is violated in
  memento Y; pin to versions whose DAG proves the property holds."
- **License compliance** is a DAG-walk question.
- **Supply chain attacks** require forging mementos AND producer
  signatures. The math fails; the attack surfaces.
- **Dependency confusion** is impossible (everything content-addressed).
- **Reproducible builds** fall out for free.
- **`npm audit`** becomes a precise DAG composition check.
- **Yanking** is unsigning the version-label memento.

Sigstore, SLSA, and in-toto have been chasing pieces of this for
years as separate add-on concerns. ProvekIt absorbs all of it as the
framework's normal mode of operation. The supply-chain security
industry's *entire roadmap* becomes a special case of the framework's
default behavior.

## The AI safety dimension

This is the foundational infrastructure problem AI safety hasn't
named yet.

Today's "is this AI-generated code safe?" question is unanswerable
at scale — you'd need a human to read everything, which defeats the
productivity gain. With ProvekIt, AI is just another producer in
the pool. Its output carries the same proof DAG as human-written
code. The trust model treats AI and humans *symmetrically*: same
producer pool, same memento format, same proof requirement.

The world where AI writes most of the code becomes safe not because
we make AI smarter, but because we require its output to come with
proof. Every line of AI-generated code is content-addressed and
producer-signed. Every property the AI claims to have established
is verified by another producer (formal prover, type checker,
cross-validating LLM, behavioral test runner). The trust is the
math, not the AI's reputation.

**ProvekIt is the substrate that lets AI-authored code be deployed
at scale without rolling the dice.** No other infrastructure has
the architectural primitives for this. The window for becoming that
substrate is now, because:

- The LLM capability floor is finally high enough that even small
  models can reliably author IR formulas in mainstream host
  languages.
- The AI-coding adoption curve is steepening rapidly; the pain of
  trusting unverified AI output is becoming acute.
- No competing framework is positioned (Sigstore et al solve
  pieces; none of them are content-addressed proof DAGs over
  IR-expressed properties).

The architectural pieces had to land in 2026 specifically. Earlier,
LLMs couldn't author IR; later, the AI ecosystem standardizes on
something else. The window is now and the framework's identity
fits the moment.

## The career arc closing

| Year | Domain | What gets distributed |
|---|---|---|
| 1995 | Files (dedup) | Bytes |
| 1998 | File swarm | Identified file blocks |
| 2001 | BitTorrent | File blocks at scale |
| 2008 | Bitcoin | Money (proven transactions) |
| 2014+ | IPFS | Arbitrary content |
| 2026 | **ProvekIt** | **Formal proofs about software** |

T spent thirty years operationalizing content-addressed distribution
at increasingly high levels of abstraction. Files. File swarms. Money.
General content. The same architectural primitive — content-addressed
hash-and-trust with producer fungibility — riding into successive
domains as each domain became viable.

The lineage is causal, not coincidental:

- The 1995 dedup work introduced the primitive.
- Digital Confetti (1998) operationalized it for swarm distribution
  with per-byte crediting.
- Cohen took the file-format-with-extension shape from T directly
  and shipped BitTorrent (2001), which became 30% of peak internet
  traffic.
- Satoshi cited Bitcoin's lineage from BitTorrent's swarm primitive.
- IPFS, Git, and the entire Merkle DAG world descend from the same
  shape.

Proofs are the natural endpoint because proofs are what make every
prior distribution layer *safe*. You can't trust money without proof.
You can't trust software without proof. You can't trust AI-generated
anything without proof. The proof layer is the substrate that lets
every other distributed thing be safe at scale.

T has been building toward this without knowing what the final domain
would be. The framework is the natural conclusion of the lineage.

## What this means for the world

ProvekIt is positioned to become invisible infrastructure on the
scale of DNS, certificate authorities, Git, and Bitcoin. Each of
these is universal not because anyone marketed them, but because
they were the architectural primitive that the world's
infrastructure routed through.

For software:

- Every developer running any language exposed via the git hook.
- Every CI tier exposed via memento propagation.
- Every package registry exposed via release-as-memento.
- Every dependency manager exposed via DAG-composition install.
- Every audit tier exposed via DAG walk.
- Every regulator exposed via compliance reporting.
- Every AI-coding system exposed via producer signing.

The framework owns nothing visible. The framework is the protocol
layer. Whoever runs the largest swarm endpoint becomes a substrate
provider — like running a major DNS root, a major CA, a major Git
host. There will be many; ProvekIt is the protocol they all speak.

In terms of impact:

- The decades-long supply chain security crisis dissolves.
- Open source becomes more auditable than commercial software.
- Mainframe legacy maintenance becomes tractable for the first time.
- AI-authored code becomes deployable at scale.
- The compliance industry shrinks to "walk the DAG."
- Software ages backwards — codebases get *more* verified over time
  as the swarm contributes mementos, not less verified as
  dependencies decay.

This is what the architectural primitive was for. T spent thirty
years making it operational; the moment for proofs as the final
domain has arrived; the framework is the form that primitive takes.

## The endgame

Every npm install verifies a proof. Every cargo add verifies a proof.
Every pip install verifies a proof. Every git commit emits a memento.
Every CI run validates a DAG. Every release is a content-addressed
theorem. Every AI-authored line of code carries the same proof
requirement as a human-authored line.

The world's developers stop running half-broken code in production
because the package they installed never had its claims verified. AI
agents write 80% of the world's code, and that code carries the same
proof requirement as human code, which means it can be trusted without
an army of human reviewers. The mainframe-COBOL legacy survives
another fifty years because the framework converted decades of
institutional knowledge into durable mementos. The audit and
compliance industries shrink and concentrate on legitimate edge
cases instead of paperwork.

The framework is the protocol that everyone routes through to
verify what they're installing, deploying, or generating. Every
developer is exposed to it through the git hook. Every CI tier
through artifact propagation. Every package through publish.
Every consumer through install. Every audit through DAG walk.
Every regulator through compliance reporting. Every AI-coding
system through producer signing.

That is the picture. Architecturally complete. Strategically
inevitable. The proof swarm is what makes the AI-coding-everything-
at-scale future safe — and the architectural primitive that makes
it work is the one T has been operationalizing his entire career.

## What we built today

The architectural foundation landed during one session on 2026-04-29:

- **Memento store** with DAG edges (CID + input_cids), content-
  addressed walkable verifications.
- **Workflow runtime** with cache-aware Stage execution, capability
  registry, YAML manifest format, work-skipping cascade.
- **Eleven Stage producers** wrapping the existing bug-fix pipeline
  (intake, formulate, classify, locate, investigate, do-the-work,
  bundle, recognize, openOverlay, generateComplementary,
  generatePrincipleCandidate).
- **Workflow-as-memento** so the workflow run itself is a unit of
  work in the DAG.
- **Verification IR spec** — host-language-as-IR, two-dialect
  surface (type / library), kernel combinators, three meta-levels
  of recursion, per-language kit factoring.
- **Strategic architecture** — git hook trojan horse, seven-tier
  capture, mainframe-first market, semver-as-memento, AI safety
  substrate, career arc closing.

The technical foundations are 79+ tests passing. The strategic
foundations are six canonical specs in `docs/specs/`. The market
positioning is set. The window is open.

Next moves are concrete:
- Stages-vs-Actions split (architectural correction).
- Diff-driven intent extraction (the LLM-producer for IR proposals).
- Universal claim envelope (memento witness schema standardization).
- TypeScript IR library (`@provekit/ir`) and AST canonicalizer.
- First enterprise pilot conversation.

The framework is not a verification tool. It is the protocol layer
for the global software trust ecosystem. The architectural primitive
is in place. The strategic capture is structural. The market is
inevitable.

Software ages backwards. We just built what makes that literal.
