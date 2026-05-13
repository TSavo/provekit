# Version Chains: Pinning, Walking, and Package Management

**Status:** v1.2.0 normative addendum to the substrate-layers spec, the contract-cid spec, and the contract-set-extension spec.
**Date:** 2026-05-03

## §0. Why this spec exists

Package managers (npm, PyPI, Maven, Cargo, RubyGems) play three roles today: they pick what a version label means, they distribute the binary, and they implicitly underwrite the trust relationship between maintainer and consumer. All three are honor-system. The maintainer types `18.2.1`, the registry serves it, the consumer fetches it, and nobody verifies that the resulting binary preserves the contracts the consumer had pinned to.

The substrate already has all the machinery to replace each role mechanically:

- **Versioning** is a derived view: walk `previousContractSetCid` references in attestation metadata.
- **Distribution** is a content-addressed fetch: any CID-resolver can serve any bundle.
- **Trust** is a witness chain: consumers pick which signers they trust, walk the witness DAG, accept or reject.

This spec names the pattern. It does not add new substrate primitives. It defines the metadata conventions a maintainer publishes and the DAG walks a consumer performs to replicate the package-manager role without a central registry.

## §1. The maintainer's publication chain

A library is a sequence of releases. Each release is a signed bundle attestation per the substrate-layers spec, with body metadata per the contract-set-extension spec.

**At release time, the maintainer:**

1. Authors the new contract set (existing contracts + newly added contracts) and emits the bundle.
2. Computes `contractSetCid` for the new bundle (per the contract-set-extension spec §1).
3. Mints an attestation with body fields naming the prior release's `contractSetCid` and any other relevant metadata:

```json
{
    "envelope": { "signer": "...", "declaredAt": "...", "signature": "..." },
    "header":   { "schemaVersion": "1", "kind": "self-contracts-attestation", "lang": "rust", "cid": "blake3-512:<bundle CID>" },
    "metadata": {
        "contractSetCid":         "blake3-512:<this set>",
        "previousContractSetCid": "blake3-512:<prior set>",
        "versionTag":             "18.3.0",
        "channel":                "stable",
        "binaryCid":              "blake3-512:<binary>",
        "buildSourceCommit":      "git:<commit hash>"
    }
}
```

4. Publishes the attestation. Any CID-resolver can serve it: a maintainer's own server, an IPFS node, a personal git repo, a peer-to-peer swarm. The publication step is signing and posting; the substrate does not care where the bytes live as long as their CID is what was signed.

The chain emerges from the `previousContractSetCid` references. Walking the chain backward IS the version history.

## §2. Consumer pinning

A consumer references libraries through a pin file (analogous to `package-lock.json`, `Cargo.lock`, `Gemfile.lock`). On the substrate, the pin is a CID:

```toml
[dependencies.react]
attestationCid = "blake3-512:<the maintainer's attestation>"
trustPolicy    = "policy-react-conservative"
```

`attestationCid` pins exactly this attestation. To resolve it, the consumer fetches the attestation, validates its envelope signature, walks its `previousContractSetCid` chain back to whatever depth their policy requires, and confirms each link per the contract-set-extension spec §3.

The pin is signer-independent because `attestationCid` references the maintainer's signed memento by content. The maintainer cannot rewrite history without producing a different `attestationCid`. Multiple maintainers can publish their own attestations naming the same release; the consumer picks which signers they trust by referencing the corresponding `attestationCid`.

A consumer may also pin a `contractSetCid` instead of a specific attestation:

```toml
[dependencies.react]
contractSetCid = "blake3-512:<the desired contract set>"
trustPolicy    = "policy-react-conservative"
```

This is "any release whose contract set is this exact one". Equivalent to a maximally strict version pin. The consumer accepts any attestation by any trusted signer claiming this `contractSetCid`. Witness counts on the contractSetCid become the trust signal, not the attestation's identity.

## §3. Range matching as DAG queries

`"^18.2.0"` (semver-compatible-with-18.2.0) becomes a typed query over the substrate:

```
resolve(library, base_contractSetCid, policy):
    candidates = walk_forward(base_contractSetCid)
        .filter(attestation.signer in policy.trustedSigners)
        .filter(attestation.metadata.channel == policy.channel)
        .filter(not attestation.metadata.yanked_by_anyone_in_policy.trustedSigners)
        .filter(every link in chain back to base_contractSetCid validates per §3 of contract-set-extension)
    return latest(candidates) by metadata.declaredAt
```

The maintainer's `^` semantics — "compatible with 18.2.0, take the latest non-breaking" — becomes "any release whose chain reaches 18.2.0's `contractSetCid` via valid extensions, ranked by declaration time." The version-string mapping (`^18.2.0` accepts `18.2.x`, `18.x.y`, but rejects `19.0.0`) is exactly the contract-set-extension chain check, expressed in CIDs rather than version strings.

`~18.2.0` (allow patch but not minor) becomes:

```
walk_forward(base)
    .filter(attestation.metadata.contractSetCid == base.contractSetCid)
```

Same set, accept any signer's witness within policy. Patch is a degenerate extension where `addedContractCids` is empty.

`>= 18.2.0, < 19.0.0` is the explicit form of `^`, expressible as the same DAG walk with a stop condition at any attestation whose chain breaks (no `previousContractSetCid` or fails validation).

## §4. Yank handling

A "yank" today is the registry retroactively withdrawing a published version. On the substrate, the maintainer publishes a NEW attestation that yanks an earlier one:

```json
{
    "metadata": {
        "yanksContractSetCid":  "blake3-512:<the withdrawn set's CID>",
        "yankReason":           "security:CVE-2026-XXXX",
        "yankSeverity":         "critical"
    }
}
```

The yank is itself a signed memento. Consumers walk the DAG and apply yanks per their policy:

- **Strict policy**: any yank by the maintainer of the original release excludes the yanked set from resolution.
- **Audit policy**: only yanks signed by specific trusted security signers are honored.
- **Permissive policy**: yanks are informational; the consumer continues to install the yanked version with a warning.

Different consumers honor yanks differently. The substrate doesn't enforce a single yank semantics; it carries the signed yank claim, and tooling decides what to do.

## §5. Channels and pre-release lanes

A maintainer publishes parallel attestation chains for different channels:

```
stable:  v18.0.0 → v18.1.0 → v18.2.0 → v18.3.0 → ...
canary:  v18.0.0 → v18.0.0-canary.1 → v18.0.0-canary.2 → ...
lts:     v17.0.0 → v17.0.1 → v17.0.2 (no minor bumps allowed)
```

Each chain is a sequence of attestations linked by `previousContractSetCid`. The `channel` metadata field labels which chain a given attestation belongs to. A consumer pinning to the `lts` channel walks only the LTS chain and never picks up minor bumps.

Branching: a maintainer may fork a chain by minting two attestations whose `previousContractSetCid` is the same prior release. Each fork is its own chain going forward. Consumers choose which fork to follow based on the metadata (`channel`, `versionTag`, `releaseManager`, etc.).

Merging: two chains converge when a maintainer mints an attestation whose new contract set is the union of two prior chains' contract sets. The chain DAG forms a lattice; consumers pick paths through it.

## §6. Trust delegation

The consumer's policy decides who they trust. The substrate provides the verification infrastructure but does not pick signers.

A consumer's trust policy is itself a memento (a body field convention):

```json
{
    "trustedSigners": [
        "ed25519:<react-team>",
        "ed25519:<react-team-security-auditor>",
        "ed25519:<consumer-internal-signing>"
    ],
    "minWitnessCount":     2,
    "requireWitnessFrom":  ["ed25519:<security-auditor>"],
    "channel":             "stable",
    "yankPolicy":          "strict"
}
```

Resolving a dependency walks the DAG and checks each candidate against the policy. The policy itself can be content-addressed and shared: an enterprise publishes their policy memento, and all teams inside the org pin to that policy's CID. Updating the policy is a new memento; teams that pin to the old policy stay on the old policy until they update their pin.

Trust is local. Two consumers with different policies see different valid-resolution sets for the same dependency. Both are correct; the substrate doesn't pick.

## §7. Distribution

Distribution decouples from the registry. A bundle's `cid` is the content-address; any CID-resolver can serve it. Possibilities:

- The maintainer's own HTTPS server, mirrored via the bundle's CID.
- IPFS / Helia / Iroh / any content-addressed network.
- A peer-to-peer swarm where consumers seed bundles they've fetched.
- A traditional package registry that adopts the substrate (registry-as-cache).

The CID is the only authoritative identity. A bundle fetched from Maven Central, a personal Git repo, and an IPFS node all hash to the same CID if their bytes match. The substrate doesn't care which host served it; the consumer recomputes the CID and matches against their pin.

This collapses the registry's distribution role into a generic content-fetch mechanism. Existing registries can serve as one fetch path among many; new fetch paths emerge naturally without protocol changes.

## §8. The package manager, redrawn

Today's package manager is monolithic: it holds the registry, picks versions, fetches binaries, manages trust implicitly. On the substrate, the same surface decomposes into independent tools:

- **Pin file**: declarative spec of what the consumer wants. Contains CIDs and policy references.
- **Resolver**: walks the substrate DAG, applies policy, picks attestations satisfying the pin.
- **Fetcher**: takes a CID, returns bytes. Pluggable backend (HTTPS, IPFS, local cache).
- **Verifier**: re-checks invariants (envelope signature, content CIDs, header validity, body interpretations).
- **Audit publisher**: optional component for organizations that publish their own attestations of dependencies (license checks, security scans, internal blessings).
- **Yank applier**: implements the consumer's yank policy.

Each component is independently replaceable. An ecosystem can build their preferred shape (cargo-style, npm-style, custom enterprise registry, fully decentralized peer-to-peer) on the same DAG. Different consumers can use different package managers against the same maintainer attestation chains.

## §9. Migration from existing ecosystems

A library currently published on npm can publish a parallel attestation chain on the substrate without leaving npm. The maintainer:

1. For each existing release, computes `contractSetCid` from the IR contracts derivable by ProvekIt's lift adapters.
2. Backfills attestation chain via `previousContractSetCid` references reflecting the existing version history.
3. Publishes the attestation chain (anywhere CIDs are resolvable).

Consumers may continue using npm normally, OR pin to the substrate attestations for releases that have them. Hybrid pins are valid: some dependencies pinned via npm version strings, others pinned via attestation CIDs. The transition is gradual; the substrate doesn't require a flag day.

When enough consumers prefer attestation pins, registries start serving attestations alongside binaries. When most consumers do, the registry's role shrinks to "convenient cache" and the substrate is the source of truth.

## §10. What this spec does not add

This spec adds zero substrate primitives. It defines metadata conventions and tooling patterns. The maintainer writes specific body fields; the consumer interprets them. The substrate sees signed mementos with content-addressed references and arbitrary metadata, exactly as before.

The package-manager replacement is a composition above the three primitives (sign, hash, reference) plus the layered shape and the metadata conventions specced this session. It is the empirical demonstration that the substrate's design is sufficient: the same minimal core handles individual contract claims, witness chains, version histories, trust policies, distribution, and registry roles, with no growth at the protocol level.

The substrate stays small. The composition layer carries the world.
