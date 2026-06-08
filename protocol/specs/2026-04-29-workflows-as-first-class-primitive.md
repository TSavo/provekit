# Sugar: workflows as first-class primitive

## Thesis

Sugar has TWO primitives that today are confused as one:

1. **The Certificate Authority (CA).** Memento store + producer registry
   + capability dispatch + swarm gateway. The mechanism that ISSUES
   certificates of correctness. Generic. Doesn't know what's being
   certified. Doesn't know what workflow led to a certificate request.

2. **A workflow.** A specific composition of certificate productions
   in a specific order with specific dependencies, producing a specific
   KIND of certificate as terminal output.

Today's `runFixLoop` is BOTH. It's a workflow (the bug-fix sequence:
intake → investigate → locate → classify → formulate → patch → test →
bundle) AND it's the CA machinery for that workflow's stages, welded
together. The bug-fix sequence is *perfect* for bug-fix — it was hand-
tuned for that one use case. But the workflow code and the CA primitive
are intertwined; you can't run a different workflow without rewriting
the orchestrator.

The architecturally honest split makes the CA primitive workflow-
agnostic. Workflows ride on top.

## The architectural cut

```
Layer 1 — Certificate Authority (protocol, generic)
  ┌─────────────────────────────────────────────────┐
  │ memento store                                   │
  │ producer registry                               │
  │ capability dispatch                             │
  │ swarm gateway (CID export/import)               │
  │                                                 │
  │ Knows nothing about workflows. Issues, stores,  │
  │ looks up, swarms certificates by content hash.  │
  └─────────────────────────────────────────────────┘

Layer 2 — Workflows (specific, composable, plural)
  ┌──────────────────┐ ┌──────────────────┐ ┌──────────────────┐
  │ bug-fix workflow │ │ compliance-audit │ │ property-assert  │
  │ intake →         │ │ parse-policy →   │ │ parse-prop →     │
  │ investigate →    │ │ enumerate-       │ │ bind →           │
  │ locate →         │ │ controls →       │ │ verify →         │
  │ formulate →      │ │ check-each →     │ │ certificate      │
  │ patch →          │ │ report           │ └──────────────────┘
  │ test →           │ └──────────────────┘
  │ bundle           │
  └──────────────────┘
        │ each is a recipe + small orchestration function
        │ all ride the same Layer 1 primitive
        │ none of them know about the others
```

Each workflow is a function that orchestrates certificate requests
against the CA primitive. The CA doesn't change as new workflows are
added; workflows don't change as new producers are added.

## Workflow shape

A workflow is data plus a tiny orchestration function:

```typescript
interface Workflow<TInput, TOutput> {
  /** Workflow identity (for telemetry, audit, swarm distribution). */
  name: string;
  /** Semantic version + content hash; workflows are themselves swarm artifacts. */
  cid: string;
  /** Description of what kind of certificate this workflow terminally produces. */
  produces: CertificateKind;
  /** The orchestration. Composes certificate requests against the CA. */
  run(ca: CertificateAuthority, input: TInput): Promise<TOutput>;
}

const bugFixWorkflow: Workflow<UserText, FixBundle> = {
  name: "bug-fix",
  cid: "bafy...",
  produces: "fix-bundle",
  async run(ca, userText) {
    const intent = await ca.request({ kind: "intake", input: userText });
    const investigate = await ca.request({ kind: "investigate", input: intent });
    const locus = await ca.request({ kind: "locate", input: { intent, investigate } });
    const cls = await ca.request({ kind: "classify", input: { intent, locus } });
    const inv = await ca.request({ kind: "formulate", input: { intent, locus, cls } });
    const verdict = await ca.request({ kind: "verify", input: { invariant: inv } });
    if (verdict.status === "violated") {
      const patch = await ca.request({ kind: "patch", input: { intent, inv, verdict } });
      const test = await ca.request({ kind: "test", input: { patch, inv } });
      return await ca.request({ kind: "bundle", input: { patch, test, inv } });
    }
    return await ca.request({ kind: "bundle", input: { inv, holds: true } });
  },
};
```

A different workflow uses the same CA primitive for an entirely
different purpose:

```typescript
const complianceAuditWorkflow: Workflow<{policy: Policy, code: Code}, Report> = {
  name: "compliance-audit",
  cid: "bafy...",
  produces: "compliance-report",
  async run(ca, { policy, code }) {
    const parsed = await ca.request({ kind: "parse-policy", input: policy });
    const results = await Promise.all(
      parsed.controls.map(c =>
        ca.request({ kind: "control-check", input: { control: c, code } })
      )
    );
    return await ca.request({ kind: "compliance-report", input: { results } });
  },
};
```

The CA primitive doesn't change. Producers register against
capabilities (`intake`, `parse-policy`, `control-check`, etc) — the
registry resolves which producer handles each capability. Workflows
compose. Use cases compose. The framework absorbs new workflows
without modification.

## What the codebase looks like under this split

- **runFixLoop becomes the bug-fix workflow.** Same code, same
  sequence, same correctness — just relocated under
  `src/workflows/bug-fix.ts` instead of `src/fix/orchestrator.ts`.
- **The orchestrator becomes a workflow runner.** A small function:
  given a workflow CID and an input, look up the workflow, dispatch
  to it, return its output. Maybe ~30 lines.
- **New workflows ship as new files** under `src/workflows/`, OR
  pulled from the swarm by CID. They register against the workflow
  runner the same way producers register against the producer registry.
- **The CLI gets verb-shaped over workflows.**
  `sugar prove --workflow bug-fix` (default), or
  `sugar prove --workflow compliance-audit --policy ./gdpr.yaml`,
  or `sugar prove --workflow <swarm-CID> [args]`.
- **Workflows are themselves swarm artifacts.** Someone publishes "FDA
  medical-software-validation workflow"; teams pull it by CID and run
  it on their code, get FDA-compliance certificates as output.

## Why this is the next cut

The architectural pattern T has been pointing at all session is the
same principle stated cleaner each time:

| Layer | What's behind the protocol | What's in front |
|---|---|---|
| Memento store | Producers (engines, LLMs) | Verdicts (mementos) |
| Producer registry | Concrete impls | Capabilities |
| Swarm | Local files | Network artifacts |
| **Workflow runner** | **Hardcoded orchestration** | **Composable recipes** |

Same architectural cut at every layer: separate the protocol from
the consumer. Memento store separated from producer impl. Producer
registry separated from stage code. Swarm separated from local cache.
**Workflow separated from CA.** This is the next layer.

Without this split, the framework has the *primitives* of a
certificate authority but is operationally a single-workflow tool.
With it, the framework becomes a CA-as-platform that any workflow
can ride. The bug-fix workflow stays — it's perfect — but it stops
being the thing the framework IS, and becomes one workflow among
many.

## Use cases this unlocks

Workflows that fit the same CA primitive without changing it:

- **bug-fix** (today, but as one workflow not the framework)
- **change-implementation** ("make this X do Y" — the prospective shape
  whose pipeline differs from bug-fix at the LOCATE step)
- **property-assertion** ("verify this property holds" — MCP `/prove`
  without needing a fix)
- **compliance-audit** (load a policy, check each control, emit a
  certificate of compliance)
- **principle-derivation** (mine a corpus, surface candidate
  principles, run adversarial validation)
- **mine-history** (replay git log, mint observations from past commits)
- **codebase-attestation** (cryptographically attest the current state
  of a codebase against its standing invariant set)
- **producer-cross-validation** (run two producers on the same input,
  surface disagreements as a quality signal)
- **swarm-import** (pull a principle library by CID, verify hashes,
  splice into local corpus)

Each is a recipe of certificate requests against the CA. None of them
require modifying the CA primitive. Most of them are 20-100 lines of
orchestration code.

## Implementation phasing

This is a refactor, not a fresh build:

1. **Define `Workflow` interface and `WorkflowRunner`.** The
   minimum primitive: load a workflow by name, dispatch to its
   `run()` with a `CertificateAuthority` handle.
2. **Extract the existing fix-loop sequence into
   `src/workflows/bug-fix.ts`.** Same code, same behavior. The
   orchestrator becomes thin.
3. **Wire `sugar prove` to dispatch via the workflow runner.**
   `--workflow <name>` defaults to `bug-fix` for backward compat.
4. **Add a second workflow as proof-of-concept.** Probably
   `property-assertion` (the simplest non-bug-fix shape).
5. **Allow workflows to be loaded from the swarm by CID.** Once the
   swarm-distribution layer (step 5 of the relational memento store
   spec) lands, workflows ride the same swarm.

Steps 1-3 are the load-bearing split. Step 4 demonstrates the
extensibility. Step 5 closes the loop.

## What this is for

This document captures the architectural cut T arrived at after
pushing me through several layers of "you're still not seeing it."
The certificate-authority framing was the right shape for a
SINGLE-engineer no-sharing world; the workflows-as-first-class-
primitive cut is what makes that CA primitive serve more than one
use case.

A reader who understands this document understands that Sugar is:
- **A certificate authority** for software correctness
- **+ a workflow runtime** that composes certificate requests
- **+ a swarm** that distributes both certificates and workflows

Three independently-evolvable layers. None depends on the others'
internals. New use cases compose at the workflow layer; new producers
compose at the CA layer; new audiences compose at the swarm layer.

That's the full architectural shape. Any earlier framing — "Sugar
is a verification tool," "Sugar is a fix-loop," "Sugar is AI-
assisted code review" — is at most one workflow on top of the
underlying CA primitive, mistaken for the framework itself.
