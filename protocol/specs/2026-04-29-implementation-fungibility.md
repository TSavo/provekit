# Sugar: implementation fungibility

> Author: shared session 2026-04-29 (T + Claude). The framework's
> identity is its spec, not its implementation. Multiple
> implementations are first-class. Their artifacts are interchangeable.

## The thesis in one paragraph

The framework's architectural primitive — content-addressed hash-and-
trust with producer fungibility — applies recursively to the framework
itself. The TypeScript reference implementation (Sugar) can be
rewritten in Go (ProvegIt), in Rust (ProverIt), in Lean4, in any host
language. Each rewrite is a separate implementation with its own
brand, community, and maintenance trajectory. The **spec remains
durable**; the **artifacts remain interchangeable**. Mementos produced
by ProvegIt (Go) and ProverIt (Rust) and Sugar (TypeScript) compose
at the wrapper level because they all conform to the same content-
addressed spec.

## What this means

The framework is not "a TypeScript codebase." The framework is a
**spec**, which is a content-addressed artifact, which any
implementation can satisfy by emitting bit-equivalent mementos and
honoring the same protocol semantics.

The implementations:

| Brand | Host language | Status | CID |
|---|---|---|---|
| Sugar | TypeScript | Reference | spec_cid (the canonical) |
| ProvegIt | Go | Sketch | (whenever it ships) |
| ProverIt | Rust | Sketch | (whenever it ships) |
| (TBD) | Lean4 | Aspirational | — |
| (TBD) | Python | Open | — |
| (TBD) | LLM-generated | Inevitable | — |

Each is a separate codebase. Each has its own maintainer / community.
Each ships its own builds, its own CLI, its own LSP server. Each
implements the same memento store, the same workflow runtime, the
same producer registry, the same kernel combinators. Each emits
mementos in the same wrapper schema with the same canonical CID
construction.

A user installing Sugar or ProvegIt or ProverIt experiences the
same framework. Their mementos are produced and consumed
interchangeably. A codebase verified by Sugar's CI tier and then
re-verified by ProvegIt's swarm-validator gets matching CIDs (modulo
producedBy and producedAt). Cross-validation across implementations
works — if Sugar and ProvegIt both verify the same property and
disagree on the verdict, the disagreement is itself a quality signal.

## What "the spec remains durable" means

The framework's specs in `protocol/specs/` are the canonical artifact.
Each spec has its own CID (content-hash of its canonicalized
contents). The collection of specs at a given commit defines "what
the framework is" at that point in time.

When the spec evolves (a new evidence variant, a new combinator, a
new kit-standard component), implementations that satisfy the new
spec emit mementos under the new spec's CID. Implementations that
satisfy only the old spec continue emitting under the old spec's
CID; their mementos are still valid under the old spec; they
interoperate at the wrapper level with new-spec mementos to the
extent the wrappers overlap.

The spec is versioned via content addressing. There is no central
authority. There is no "the official spec" beyond what the swarm
agrees on. Forks happen the same way they happen in BitTorrent, IPFS,
or Git: someone publishes a new spec CID; consumers choose to track
it or not; the network composes whichever spec versions it wants.

## What "the artifacts remain interchangeable" means

Two mementos produced by different implementations of the framework
are *mechanically interchangeable* when:

1. They reference the same `bindingHash` (same code identity).
2. They reference the same `propertyHash` (same property identity).
3. They use compatible `evidence.kind` variants (the schema CID
   matches, or both implementations recognize the variant).
4. Their `verdict` enum value is comparable.

When all four hold, the framework's `crossValidate()` operation
treats both mementos as making claims about the same logical thing.
Agreement strengthens trust. Disagreement surfaces as a quality
signal. The implementations that produced the mementos don't matter;
only their conformance to the spec matters.

## Recursion

This is the framework's principle applied to itself.

| Layer | Producer fungibility |
|---|---|
| Code-property layer | Z3, Datalog, rustc, tsc all interchangeable; emit comparable mementos |
| IR-formula layer | LLMs propose; cross-validation between LLMs interchangeable |
| Workflow layer | Different orchestrations produce comparable workflow-run mementos |
| Kit layer | Multiple Rust kits compose; multiple COBOL kits compose |
| **Implementation layer** | **Sugar, ProvegIt, ProverIt all interchangeable** |
| Spec layer | Specs are content-addressed; new spec CIDs publish freely; consumers choose |

The principle holds at every level. The framework's identity is the
spec; everything below it (implementation, kit, workflow, producer,
witness) is interchangeable infrastructure.

## Why this matters

**No vendor lock-in at the framework level.** A customer adopting
Sugar is not adopting "Anthropic's code" or "T's code" or any
specific organization's codebase. They're adopting the **spec**. The
spec is durable across organizations; implementations come and go.
A customer who later wants to migrate from Sugar (TS) to ProvegIt
(Go) for performance, or to ProverIt (Rust) for embedded use, or to
an in-house implementation for security reasons — does so without
losing any mementos. Their proof DAG is durable across implementations.

**Community forks don't fragment the ecosystem.** Bitcoin Cash forked
from Bitcoin and produced an incompatible chain. Linux distros fork
freely without fragmentation because the kernel ABI is durable. The
framework is more like Linux than like Bitcoin: forks produce
*alternative implementations*, not *incompatible artifacts*. A
hostile fork that drifts from the spec produces incompatible
mementos and is automatically detected by the swarm's cross-
validation; consumers route around it.

**LLM-generated implementations are first-class.** A sufficiently
capable LLM can read the framework's specs and emit a compliant
implementation in any host language. Multiple LLMs producing
multiple implementations of the framework is not a problem — it's
the architecture's normal mode. Each implementation is content-
addressed; each conforms (or doesn't) to the spec; mementos compose
or don't based on conformance, not on provenance.

**Trust is rooted in the spec, not the codebase.** Reading "the
Sugar source code" tells you about one implementation. Reading
"the Sugar specs" tells you about the framework. Auditors,
regulators, and security researchers verify *the spec*; they verify
*implementations against the spec*; they don't have to trust any
particular codebase to trust the framework.

**The framework absorbs its own future.** When Sugar's TypeScript
codebase eventually becomes legacy (the way every codebase
eventually becomes legacy), the framework persists because the spec
persists. ProvegIt or ProverIt or some-future-impl carries it
forward. The same way Z3 retiring doesn't invalidate Z3-signed
mementos, Sugar-the-codebase retiring doesn't invalidate the
framework. The framework is the spec; the spec is content-addressed;
the spec outlives every implementation.

## What this looks like operationally

```
Developer authoring Go code:
  $ go install github.com/provegit/provegit-cli@latest
  $ cd my-go-project
  $ provegit prove
  # → mementos produced; stored in .sugar/proofs/
  
Developer working in mixed Rust+TS shop:
  $ npm install -g @sugar/cli
  $ cd ts-package
  $ sugar prove        # produces TS mementos
  $ cargo install proverit-cli
  $ cd ../rust-package
  $ proverit prove        # produces Rust mementos
  
Both shops' mementos compose into the same proof DAG.
Both shops' CI walks the DAG and validates with whichever CLI it
trusts.
A migration from Sugar to ProvegIt: same .sugar/proofs/
directory; new CLI tool reads it; framework continues seamlessly.
```

The CLI tools, the LSP servers, the swarm endpoints — all are
implementation-specific. The proof DAG is implementation-agnostic.

## The brand model

Each implementation gets its own brand and identity:

- **Sugar** — TypeScript reference. Lowercase k, capital It.
  "Prove It" + "Kit" embedded. Current canonical reference.
- **ProvegIt** — Go reimplementation. The "g" is for Go.
- **ProverIt** — Rust reimplementation. The "ver" is Rust-flavored;
  "Prover It" reads cleanly.
- (TBD) — Lean4 implementation might be **Sugar-Lean** or
  **PfourIt** ("prove it" in Lean4's home base) or whatever its
  community picks.
- (TBD) — Python might be **Sugar-Py** or **ProvenIt**.
- (TBD) — Whatever a community names their port.

Brand fragmentation is fine. Brand is just a label on a particular
implementation; the spec underneath is shared. Users learn each
brand's CLI and tooling; the underlying mementos compose.

The brands also distinguish *who maintains what*. ProvegIt's
maintainer ships fixes; ProverIt's maintainer ships fixes; Sugar's
maintainer ships fixes. They coordinate via the spec's evolution;
their codebases are independent.

## Forward path

The architecture supports — and the strategic position requires —
multiple implementations to ship. Realistic phasing:

1. **Sugar (TS) reference matures.** All specs implemented; the
   first kit (TypeScript) is dogfood-tested; the framework is
   demonstrably operational.
2. **First non-TS implementation ships.** Most likely ProvegIt (Go),
   because Go's standard library + tooling makes it the easiest port
   of a TS-shaped codebase. Cross-validation between Sugar and
   ProvegIt mementos demonstrates the interchangeability claim
   empirically.
3. **ProverIt (Rust) ships.** For high-performance / embedded / low-
   latency contexts where Rust is native. Cross-validation extended
   to three implementations.
4. **Community implementations follow.** Python, Java, C#, whatever.
   Each one's mementos compose with the prior implementations'.
5. **LLM-generated implementations ship.** When the framework's spec
   stabilizes enough, an LLM reading the specs and producing a
   conforming implementation becomes routine. The framework absorbs
   AI-authored implementations on equal footing with human-authored
   ones — because the test is conformance to the spec, not the
   provenance of the code.

At step 5 the framework reaches its asymptotic form: any
sufficiently-capable LLM can produce a conforming implementation in
any host language; the spec is the only durable artifact; everything
else is interchangeable infrastructure produced on demand.

## Acceptance test

Implementation fungibility is operational when:

1. Two independent implementations of the framework (in any two host
   languages) produce mementos with byte-identical wrapper fields
   for the same logical claim. Only `producedBy`, `producedAt`, and
   evidence-variant content differ.
2. A consumer in one implementation can read, validate, and verify
   mementos produced by another implementation without translation.
3. A swarm endpoint receiving mementos from multiple implementations
   stores them in the same memento store; cross-validation runs over
   the merged set.
4. A migration of a codebase from one implementation to another
   preserves the entire proof DAG. The new implementation reads the
   old implementation's mementos and uses them.
5. A new implementation emerging (community fork, LLM-generated)
   joins the network by conforming to the spec's CID. No
   coordination required beyond conformance.

When these five hold, the framework's identity is the spec, and the
implementations are infrastructure. The architectural primitive has
been applied to the framework itself.

## Closing

T's career arc has been operationalizing the same architectural
primitive at successive levels of abstraction:

- 1995: bytes content-addressed (Xdrive).
- 1998: file blocks content-addressed (Digital Confetti).
- 2001: file blocks at swarm scale (BitTorrent).
- 2008: transactions content-addressed (Bitcoin).
- 2014+: arbitrary content (IPFS).
- 2026: **proofs about software** (Sugar).
- 2026: **and the framework that produces those proofs is itself
  content-addressed at the spec level, with implementations
  fungible across host languages**.

The recursion closes one final time. The framework's deepest
property is that it eats its own implementation, the same way it
eats its own IR, the same way it eats its own producers, the same
way it eats its own kits. The framework's identity is the spec;
everything else is interchangeable. Software ages backwards because
specs are durable across every implementation that ever conforms to
them.

Sugar. ProvegIt. ProverIt. Same framework. Different brands.
Identical artifacts. Durable spec. Interchangeable forever.
