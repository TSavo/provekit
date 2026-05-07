# Content-Addressed CI Protocol (CICP)

**Status:** v0.1.0 draft extension protocol
**Date:** 2026-05-07
**Layer:** extension protocol over TDP, PEP, protocol catalogs, proof-file bundles, and the ProvekIt memento substrate
**Related:**
- `2026-05-07-protocol-evolution-protocol.md` - protocol changes as witnessed catalog transitions
- `2026-05-06-truth-discharge-protocol.md` - unit truth over signed body-claims
- `2026-05-06-grammar-conformance-protocol.md` - grammar and invariant conformance for extension bodies
- `2026-04-30-protocol-versioning.md` - catalog CID as protocol version
- `2026-04-30-proof-file-format.md` - `.proof` bundle format
- `.github/workflows/ci.yml` - current repository CI shape

## Section 0. Purpose

CICP defines CI as a content-addressed proof graph.

Traditional CI treats every run as an event:

```text
checkout commit
install tools
run commands
look at pass/fail
throw most of the result away
```

CICP treats every CI result as a witnessable implication:

```text
CIJobInputClosureCid -> CIJobResultWitnessCid
```

The result is reusable when the input closure is byte-identical or when
every changed input is bridged by an accepted evolution witness under
policy.

The protocol does not say "skip tests because paths look unrelated." It
says:

```text
this job's declared blast radius has the same CID,
and the prior result witness is still accepted under policy,
therefore this job result may be reused.
```

This turns CI from a matrix of repeated rituals into a DAG scheduler
over signed, content-addressed facts.

## Section 1. Central Lemma

**Lemma: Content-addressed CI reuse.** A CI result is reusable if and
only if:

1. the prior result witness is accepted under the current CI policy;
2. the current job input closure CID equals the witness's input closure
   CID, or every difference is covered by an accepted bridge/evolution
   witness;
3. the job definition, runner identity, toolchain identity, protocol
   catalog CID, and declared nondeterminism policy are included in the
   input closure;
4. no required dependency is unavailable, unsigned, tainted, or outside
   policy.

Stated as an implication:

```text
accepted(previousJobResultWitness)
and equivalent_or_bridged(previousInputClosureCid, currentInputClosureCid, policyCid)
  -> reusable(previousJobResultWitness, currentJob)
```

The hard part is not hashing output. The hard part is defining the input
closure narrowly enough to be useful and completely enough to be honest.

## Section 2. Vocabulary

**CI input closure.** The content-addressed manifest of everything that
can affect a CI job: source subsets, generated sources, lockfiles,
toolchains, runner image, command, environment policy, protocol catalog
CID, conformance fixtures, secrets policy, network policy, and relevant
prior witnesses.

**Blast radius.** A kit- or job-specific input closure. The blast radius
answers: "What bytes could possibly affect this job?"

**Blast radius CID.** The CID of the canonical blast-radius manifest.
Changing a relevant input changes this CID.

**CI job result witness.** A TDP-compatible witness that a job ran, or
that a prior accepted result was reused, for a specific input closure.

**Impact witness.** A signed claim that compares two repository or
protocol states and names which blast radii changed, which prior
witnesses remain reusable, and which jobs must run.

**Output CID.** The CID of the job's declared output artifact, such as a
test report, conformance report, `.proof` bundle, binary, log bundle, or
coverage artifact.

**Reuse witness.** A witness that a prior CI result is still accepted for
the current state because its input closure is unchanged or bridged.

**Bridge/evolution witness.** A witness that lets policy carry prior
facts across a change. PEP witnesses protocol catalog evolution; FRP
witnesses implementation repair; other extension protocols may witness
toolchain, runner, or fixture migration.

## Section 3. Non-goals

CICP does not:

- replace GitHub Actions, Buildkite, local `make`, or any concrete CI
  scheduler;
- make CI results trustworthy without runner identity and policy;
- claim that logs alone are meaningful correctness artifacts;
- ignore nondeterminism, time, network, or secret access;
- make core verification execute tests, compilers, or CI jobs;
- let an unchanged source path override a changed protocol catalog CID.

The protocol defines the evidence shape. Schedulers may implement any
execution strategy that respects the evidence.

## Section 4. Blast Radius Manifests

A blast radius manifest is the canonical input to a job.

Draft shape:

```json
{
  "kind": "CIBlastRadius",
  "schemaVersion": "1",
  "jobKey": "provekit/conformance/rust",
  "subjectKind": "kit|workflow|protocol|proof-bundle|artifact",
  "subject": "rust",
  "protocolCatalogCid": "blake3-512:...",
  "jobDefinitionCid": "blake3-512:...",
  "commandCid": "blake3-512:...",
  "runnerIdentityCid": "blake3-512:...",
  "toolchainCids": ["blake3-512:..."],
  "sourceClosureCid": "blake3-512:...",
  "lockfileCids": ["blake3-512:..."],
  "generatedInputCids": ["blake3-512:..."],
  "fixtureCids": ["blake3-512:..."],
  "relevantSpecCids": ["blake3-512:..."],
  "policyCid": "blake3-512:...",
  "nondeterminism": {
    "network": "forbidden|declared|unrestricted",
    "clock": "forbidden|declared|unrestricted",
    "secrets": "forbidden|declared|unrestricted",
    "randomness": "forbidden|declared|unrestricted"
  },
  "inputCids": ["blake3-512:..."]
}
```

Normative fields:

| Field | Meaning |
|---|---|
| `kind` | MUST be `"CIBlastRadius"`. |
| `schemaVersion` | MUST be `"1"` for this draft. |
| `jobKey` | Stable identifier for the CI job. |
| `subjectKind` | Job subject family. |
| `subject` | Kit, workflow, protocol, proof bundle, or artifact being checked. |
| `protocolCatalogCid` | Catalog CID under which the job is interpreted. |
| `jobDefinitionCid` | CID of the workflow step/job definition. |
| `commandCid` | CID of the normalized command and arguments. |
| `runnerIdentityCid` | CID of the runner image or runner identity claim. |
| `toolchainCids` | CIDs for compilers, interpreters, package managers, solvers, and language runtimes. |
| `sourceClosureCid` | CID of the source subset that can affect this job. |
| `lockfileCids` | Dependency lockfiles or dependency graph roots. |
| `generatedInputCids` | Generated sources or generated config inputs. |
| `fixtureCids` | Conformance fixtures, golden files, corpora, or baseline artifacts. |
| `relevantSpecCids` | Protocol specs that affect the job. |
| `policyCid` | CI admission/reuse policy. |
| `nondeterminism` | Declared treatment of network, clock, secrets, and randomness. |
| `inputCids` | Closure of CIDs the manifest depends on. MUST include every non-null CID above. |

The blast radius CID is the CID of the canonical manifest bytes. It is
not a list of paths. Paths are merely one way to construct source
closure CIDs.

## Section 5. Protocol Catalog as Root Input

Every blast radius MUST include `protocolCatalogCid`.

This is the rule that makes protocol evolution automatically invalidate
downstream CI caches. If the protocol catalog changes:

```text
oldProtocolCatalogCid != newProtocolCatalogCid
```

then every blast radius manifest containing that catalog changes unless
policy accepts a bridge/evolution witness.

PEP supplies the bridge for protocol changes:

```text
oldProtocolCatalogCid
  -> newProtocolCatalogCid
  -> ProtocolEvolutionWitness
```

CICP does not hard-code which protocol changes require reruns. Policy
decides using the PEP change class and evidence:

| PEP change | Default CICP impact |
|---|---|
| `extension-only`, no cross-kit semantic obligation | Prior kit witnesses MAY be reused if their blast radii are otherwise unchanged. |
| New required fixture, lifter obligation, ProofIR invariant, or checker rule | Affected kit blast radii change; affected jobs MUST rerun or be bridged. |
| `migration-required` | Prior witnesses MUST name migration or adoption witnesses before reuse. |
| `core-candidate` or `breaking` | Prior witnesses SHOULD be refused unless policy explicitly accepts a migration path. |

The consequence:

```text
protocol evolution is cache invalidation by CID.
```

No scheduler-specific path rule is needed to notice that the protocol
changed.

## Section 6. Job Result Witness

A successful job emits a result witness.

Draft positive witness body:

```json
{
  "kind": "CIJobResultBodyClaim",
  "schemaVersion": "1",
  "jobKey": "provekit/conformance/rust",
  "blastRadiusCid": "blake3-512:...",
  "result": "pass",
  "outputCid": "blake3-512:...",
  "logCid": "blake3-512:...",
  "startedAt": "2026-05-07T00:00:00Z",
  "finishedAt": "2026-05-07T00:00:00Z",
  "runnerIdentityCid": "blake3-512:...",
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."],
  "producer": {
    "kind": "ci-runner",
    "name": "github-actions",
    "version": "..."
  }
}
```

The TDP witness over this body says only:

```text
true(this job result was produced or accepted for this blast radius under policy)
```

It does not say the code is correct in all possible environments. It
says the named CI obligation was discharged for the named input closure.

## Section 7. Reuse Witness

Skipping a job is itself a claim.

Draft reuse body:

```json
{
  "kind": "CIReuseBodyClaim",
  "schemaVersion": "1",
  "jobKey": "provekit/conformance/rust",
  "currentBlastRadiusCid": "blake3-512:...",
  "previousBlastRadiusCid": "blake3-512:...",
  "previousResultWitnessCid": "blake3-512:...",
  "reuseReason": "identical-input-closure|bridged-by-evolution",
  "bridgeWitnessCids": ["blake3-512:..."],
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

If `currentBlastRadiusCid == previousBlastRadiusCid`, reuse is a lookup.
If they differ, reuse MUST be justified by bridge witnesses accepted by
policy.

The scheduler may report:

```text
skipped: rust conformance
reason: prior witness accepted for identical blast radius
```

but the load-bearing artifact is the signed reuse witness, not the word
"skipped."

## Section 8. Impact Witness

An impact witness summarizes a change.

Draft body:

```json
{
  "kind": "CIImpactBodyClaim",
  "schemaVersion": "1",
  "baseStateCid": "blake3-512:...",
  "candidateStateCid": "blake3-512:...",
  "protocolEvolutionWitnessCids": ["blake3-512:..."],
  "changedBlastRadiusCids": ["blake3-512:..."],
  "unchangedBlastRadiusCids": ["blake3-512:..."],
  "requiredJobKeys": ["provekit/conformance/rust"],
  "reusableWitnessCids": ["blake3-512:..."],
  "refusalCids": [],
  "policyCid": "blake3-512:...",
  "inputCids": ["blake3-512:..."]
}
```

The impact witness answers:

```text
which jobs must run,
which prior witnesses may be reused,
and why.
```

The default ProvekIt CI strategy becomes:

```text
compute current blast radii
look up accepted prior result witnesses
accept reuse witnesses for unchanged or bridged radii
run only missing obligations
emit fresh result witnesses
```

## Section 9. Kit Conformance Blast Radii

Each language kit SHOULD publish a kit blast radius.

For a kit:

```text
kit source closure
kit lifter/dropper/realizer sources
language lockfiles
toolchain identity
shared conformance fixtures
relevant protocol specs
protocol catalog CID
CI policy
  -> kitBlastRadiusCid
```

The kit conformance witness then binds:

```text
kitBlastRadiusCid -> KitConformanceWitness(result = true)
```

If a PR changes only TypeScript source and the Java kit blast radius CID
is unchanged, CI can reuse the Java witness. If the protocol catalog CID
changes, the Java blast radius changes automatically unless a PEP
witness and CI policy admit reuse.

This is the operational answer to "CI takes forever": most jobs become
hashing and witness lookup, and only changed or unbridged blast radii
execute expensive work.

## Section 10. Output CIDs

Outputs SHOULD be content-addressed.

Examples:

- test report CID;
- conformance report CID;
- `.proof` bundle CID;
- compiled binary CID;
- solver transcript CID;
- log bundle CID;
- coverage artifact CID;
- generated source CID.

However, output CIDs are downstream of the result. They do not justify
reuse by themselves. The reusable object is:

```text
input closure CID + job definition + runner/toolchain/policy -> result witness
```

An output CID without the input closure is just a blob.

## Section 11. Nondeterminism and Taint

CICP-aware tooling MUST fail closed when nondeterminism is undeclared.

The following inputs are taint sources unless explicitly modeled:

- wall-clock time;
- network access;
- secrets;
- random seeds;
- external package registries;
- mutable toolchain channels;
- unpinned operating system images;
- hardware or accelerator-specific behavior;
- flaky tests.

Policy MAY admit nondeterminism if the job records enough evidence to
make the result meaningful. For example, a job may use network access
only through a pinned package index snapshot CID, or it may declare a
random seed as part of the input closure.

If a job is flaky, CICP should not pretend otherwise. It may emit:

```text
CIJobResultBodyClaim(result = "flaky")
```

or a refusal explaining that no stable witness was produced.

## Section 12. Runner Identity

Runner identity is part of the input closure.

At minimum, a runner identity claim SHOULD include:

- operating system image or container image digest;
- CPU architecture;
- installed toolchain roots;
- scheduler identity;
- isolation policy;
- secret and network policy;
- signer identity.

Untrusted pull requests MAY emit candidate result witnesses, but policy
SHOULD NOT treat them as accepted witnesses unless the runner and signer
are accepted.

## Section 13. Garbage Collection

CICP makes CI artifacts cacheable, but not immortal.

Policy MAY garbage-collect:

- output blobs no longer reachable from accepted witnesses;
- logs after retaining their CIDs and summaries;
- candidate witnesses from untrusted runners;
- superseded impact witnesses;
- artifacts whose toolchain or runner policy has expired.

Garbage collection MUST NOT rewrite witness history. It removes local
availability of blobs; it does not change the CIDs or the signed facts
already emitted.

## Section 14. Relationship to PEP

PEP witnesses protocol catalog evolution. CICP consumes that witness
when deciding whether downstream CI facts remain reusable.

The useful composition:

```text
ProtocolEvolutionWitness(oldCatalog, newCatalog)
  -> CIImpactBodyClaim
  -> changed / reusable blast radii
  -> required job result witnesses
```

If PEP says a catalog transition is extension-only and CI policy agrees
that no kit semantics changed, previous kit conformance witnesses may be
carried forward. If PEP says a new required conformance fixture was
added, the affected blast radii change and the affected jobs rerun.

CICP therefore makes protocol-aware cache invalidation mechanical.

## Section 15. Fail-closed Behavior

CICP-aware tooling MUST fail closed when:

1. a blast radius manifest is malformed;
2. any required CID is missing, unavailable, or malformed;
3. a protocol catalog CID changed without an accepted PEP witness or
   explicit policy decision;
4. a job definition, runner, toolchain, fixture, spec, or source closure
   CID differs without an accepted bridge;
5. output bytes do not hash to their declared output CID;
6. logs or result artifacts are required by policy but unavailable;
7. nondeterminism is observed but not declared;
8. the prior result witness is unsigned, invalid, expired, refused, or
   produced by an unaccepted runner;
9. a reuse witness points to a result witness whose input closure does
   not match or bridge to the current closure.

Failing closed means no reuse result is available. It does not imply the
code is wrong. It means the job must run or a stronger witness must be
provided.

## Section 16. Non-executing Core

Core ProvekIt verification MUST NOT execute CI jobs, compilers, tests,
package managers, containers, or schedulers.

CICP is an extension protocol. Core verification validates signed bytes,
CIDs, and references. CICP-aware tooling interprets CI body claims under
policy and emits result, reuse, impact, or refusal witnesses.

## Section 17. Examples

### Section 17.1 Unchanged kit

```text
previousRustBlastRadiusCid == currentRustBlastRadiusCid
previousRustConformanceWitness accepted
  -> CIReuseWitness(result = true)
```

The Rust job is skipped with evidence.

### Section 17.2 Changed TypeScript kit

```text
typescript source closure changed
typescriptBlastRadiusCid changed
no bridge witness exists
  -> run TypeScript conformance
  -> emit new CIJobResultWitness
```

Other kit jobs may reuse prior witnesses if their blast radii are
unchanged.

### Section 17.3 Protocol extension-only patch

```text
v1.6.1 catalog -> v1.6.2 catalog
PEP witness: extension-only
CICP policy: no kit semantic obligation changed
  -> prior kit witnesses reusable through bridge
```

The protocol changed, so the blast radius CIDs changed. Reuse is not
automatic. It is admitted through the PEP bridge and CI policy.

### Section 17.4 Protocol conformance fixture added

```text
new catalog adds required fixture CID
PEP witness: compatible or migration-required
CICP policy: affected kits must rerun
  -> impacted blast radii rerun
```

The cache invalidates because the fixture CID is part of each affected
kit blast radius.

## Section 18. Catalog Property Key

If cataloged, this extension protocol SHOULD use:

```text
content-addressed-ci-protocol
```

Cataloging CICP pins the spec bytes and gives CI tooling a stable key
for declaring support. It does not make CI execution part of core
verification.

## Section 19. Open Questions

1. Should `CIJobResultBodyClaim` and `CIReuseBodyClaim` remain in one
   protocol or split once the reuse shape stabilizes?
2. What is the minimal portable runner identity that GitHub Actions,
   local `make`, and future hosted runners can all emit?
3. Should CICP define a canonical source-closure manifest format, or
   should source closure be delegated to language/toolchain-specific
   extension protocols?
4. How should policies score flaky jobs: refusal, degraded witness, or
   repeated-run quorum?
5. Should CI impact witnesses be required for protocol PRs after PEP,
   or remain an optimization over PEP adoption?

## Section 20. Citation

Cite as:

> ProvekIt Protocol Working Notes (2026). *Content-Addressed CI Protocol
> (CICP)*. Draft extension protocol v0.1.0.
