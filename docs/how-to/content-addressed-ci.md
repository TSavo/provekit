# Content-Addressed CI

CICP is a supply-chain integrity protocol for CI results.

A normal CI status says "this job passed." A CICP result says "this exact job, over this exact source/toolchain/protocol/config/input closure, produced this exact result witness." If any load-bearing input changes, the blast-radius CID changes and the old result no longer applies.

Reuse is the weak form of the story. It is useful, but only after the stronger admission claim is true: the current closure is byte-identical to a previously accepted result witness.

## What CICP Names

A `CIBlastRadius` body names the CI proposition. In the current implementation that includes:

- the kit/job key;
- the protocol catalog CID;
- relevant source paths and input CIDs;
- kit/toolchain and runner identity;
- configuration and policy inputs;
- additional proof or witness roots that must be part of the closure.

A `CIJobResultBodyClaim` records the result for a blast-radius CID. A `CIReuseBodyClaim` is admissible only when a current blast radius matches a previously accepted result exactly. A `CIImpactBodyClaim` records protocol-aware impact, for example when a PEP transition says a protocol extension is non-semantic for a given job policy.

## Supply-Chain Attacks This Helps Catch

**Protocol drift.** The protocol catalog CID is an input. If the verifier, catalog, or accepted protocol surface changes, the blast radius changes.

**Toolchain or kit drift.** A result minted by one lifter, canonicalizer, realizer, compiler, or runner identity does not silently stand in for another.

**Dependency and source substitution.** Source closure and input CIDs are part of the claim. A replaced file, regenerated fixture, changed dependency, or modified accepted witness invalidates the old result.

**Compromised cache admission.** A skip is not accepted because a CI cache key happens to match. It is accepted only when `provekit ci reuse` validates the current blast-radius body against a checked-in accepted `CIJobResultBodyClaim`.

**Protocol evolution confusion.** PEP and CICP compose. A protocol evolution changes the catalog CID, so CICP invalidates downstream witnesses unless policy explicitly admits a bridge.

## Accepted Witness Store

Checked-in accepted witnesses live under:

```text
.provekit/ci/accepted/<kit>/<blast-radius-cid>.job-result.json
```

The store is intentionally reviewable. A new candidate result witness should be inspected like any other supply-chain artifact before it becomes an accepted root.

## Commands

Validate any CICP body and print its canonical CID:

```sh
provekit ci check --body protocol/conformance/cicp/job-result-pass.json
```

Compute a shadow blast radius without skipping work:

```sh
provekit ci shadow \
  --repo . \
  --kit rust \
  --out-dir .provekit/ci-shadow/rust
```

Try to admit reuse against the checked-in accepted store:

```sh
provekit ci reuse \
  --current-blast-radius .provekit/ci-shadow/rust/blast-radius.json \
  --accepted-dir .provekit/ci/accepted \
  --reuse-out .provekit/ci-shadow/rust/reuse.json
```

Emit a candidate job-result witness after the job runs:

```sh
provekit ci result \
  --blast-radius .provekit/ci-shadow/rust/blast-radius.json \
  --out .provekit/ci-shadow/rust/job-result.json \
  --result pass
```

## CI Workflow Shape

The GitHub workflow does three things around each prove job:

1. computes a CICP shadow blast radius;
2. attempts reuse admission against `.provekit/ci/accepted`;
3. runs the job and uploads a candidate result witness when reuse is refused.

That makes the skip decision auditable. A supply-chain reviewer can inspect why a job was accepted, which closure it covered, and which committed witness carried the previous result.

## Golden Vectors

Language libraries validate their CICP body builders/checkers against [../../protocol/conformance/cicp/](../../protocol/conformance/cicp/). Passing vectors must derive the pinned CIDs. Refusal vectors must fail closed.
