# Release process

This describes how a ProvekIt protocol release is cut: catalog version bump, kit re-mints, conformance harness updates, multi-package distribution.

This document is for protocol maintainers. End users should never need it; adapter authors usually don't either.

## What gets released

A ProvekIt release is many things at once:

1. **A new protocol catalog** with a new CID.
2. **Updated specs** (IR grammar, proof file format, handshake algorithm, lattice tractability theorem, signatures, kit standard), only those that changed.
3. **Updated kits**, one per host language, all re-minting their self-contracts to match the new catalog.
4. **Updated conformance fixtures** if the canonical input/output bytes changed.
5. **Updated tooling**: `provekit` CLI, lift adapters that depend on new IR primitives.
6. **Distributed packages** to crates.io, npm, PyPI, Maven Central, NuGet, RubyGems, etc.

Release cadence is slow. Major releases (v1.0 → v2.0) coincide with breaking protocol changes. Minor releases (v1.1 → v1.2) coincide with new IR primitives, new spec docs, or new conformance fixtures. Patch releases (v1.1.0 → v1.1.1) fix bugs without changing the catalog CID.

## Pre-release checklist

Before tagging:

- [ ] All shipping implementations pass `make ci` against the new catalog.
- [ ] All shipping lift adapters pass their conformance fixtures.
- [ ] All cross-kit parity fixtures pass.
- [ ] `tools/recompute-spec-cids/` reports no drift.
- [ ] `make conformance` is green on `ubuntu-latest` and on the maintainer's macOS box.
- [ ] CHANGELOG entries written for every spec change.
- [ ] Migration notes written if any spec change affects existing kits.
- [ ] [`docs/reference/cids.md`](../reference/cids.md) updated to the new CIDs.
- [ ] [`docs/reference/per-language-status.md`](../reference/per-language-status.md) updated.

## The bump itself

Bumping the catalog CID is a coordinated activity:

1. **Freeze**: announce the bump on the project repository. Adapter authors should hold non-essential PRs.
2. **Update specs** in `protocol/specs/`. Add new spec files; update existing ones if needed. Each spec's bytes determine its CID; recompute via `tools/recompute-spec-cids/`.
3. **Update CDDL grammar** if new IR primitives are added.
4. **Regenerate codegen artifacts** in every kit (`provekit-ir-codegen` and equivalents).
5. **Re-mint self-contracts** in every kit. The new pinned CIDs replace the old ones in `make conformance`.
6. **Update `provekit verify-protocol`** in the Rust CLI to expect the new catalog CID.
7. **Update conformance fixtures** if the canonical input/output bytes changed.
8. **Run `make ci`** to verify the whole stack.
9. **Tag the release** in git.
10. **Publish packages** in dependency order: protocol catalog first, then kits, then lift adapters, then tooling.
11. **Update tutorials and how-to docs** to reference the new catalog CID where it appears.
12. **Announce** on the project repository.

## Migration notes

When a spec change affects existing kits, write migration notes. Example:

> ### Migration from v1.1.0 to v1.2.0
>
> **New IR primitive**: `temporal_atomic` for time-based predicates. Adapters can now lift `@FutureOrPresent` (Bean Validation) to canonical IR.
>
> **Action required by kit authors**: regenerate codegen, re-mint self-contracts. The new pinned CIDs are in [`docs/reference/cids.md`](../reference/cids.md).
>
> **Action required by lift adapter authors**: optionally support `temporal_atomic` if your source library has temporal annotations. No-op for adapters that don't.
>
> **Action required by users**: re-run `provekit verify-protocol` after upgrading to the new CLI. Existing `.proof` bundles remain valid (provability is monotonic). New `.proof` bundles minted under v1.2.0 may use the new primitive.

The note explicitly addresses each role's required action and clarifies what stays the same.

## Backwards compatibility

The protocol's monotonic-provability property gives strong backwards compatibility:

- **Old `.proof` bundles** remain valid. Their CIDs are correct against their bytes; their bytes haven't changed. A new verifier reads them; they discharge call sites just as before.
- **Old kits** can verify old `.proof` bundles. They don't need to upgrade to read content minted under earlier protocol versions.
- **New kits** can read old `.proof` bundles. The CID structure is stable across versions; new kits know about more primitives but the old primitives still parse.

Forwards compatibility is partial:

- **Old kits cannot read new `.proof` bundles** that use new primitives. They either skip the new mementos (degrading their handshake to "more Tier 3 work") or they error.
- **Old verifiers cannot follow new bridges** to contracts that use new IR primitives. Same degradation.

The default kit behavior on encountering an unknown primitive is to log a warning and skip the memento. This makes upgrades soft: old kits don't crash on new content, they just discharge less of it.

## Distribution dependency ordering

Publish in dependency order to avoid "I just published an adapter that depends on a kit version that isn't on npm yet" deadlocks:

1. Protocol catalog CIDs (just file changes; no package).
2. Rust workspace (canonical CLI). `cargo publish provekit-ir-types`, then `provekit-ir-symbolic`, then `provekit-claim-envelope`, then `provekit-proof-envelope`, then `provekit-self-contracts`, then `provekit-verifier`, then `provekit-cli`.
3. Per-language kits in arbitrary order. Each kit's packages are published per its ecosystem's convention.
4. Lift adapters per kit, after the kit is published.
5. Tooling (LSP plugins, build-script integrations).

For repository tags, a single tag per release covers the whole monorepo: `v1.2.0`. CI builds and tests every implementation under that tag.

## Who can cut a release

Today, release authority lives with the maintainers of `tools/recompute-spec-cids/` and `make conformance`. In practice this is a small group.

The release process is intentionally rigid because the catalog CID is the protocol's identity. A botched release that publishes an inconsistent set of kits (some pinning v1.1.0 self-contracts, some pinning v1.2.0) breaks cross-kit conformance for users until corrected.

When the project grows, the release process should be automated: `make release` could orchestrate the bump, conformance check, package publish, and announcement. Today it's a coordinated manual procedure.

## Post-release

After the release is announced:

- Update [`docs/reference/cids.md`](../reference/cids.md) on `main` if not already.
- Update tutorials that reference the catalog CID.
- Watch for adapter authors reporting upgrade issues; respond in issues / PRs.
- Wait roughly two weeks before considering the release "stable." Early-adopter feedback often reveals migration friction.

## When this is done

Every shipping implementation is on the new catalog CID. `provekit verify-protocol` reports the new CID. Tutorials and reference docs reference the new CID. Adapter authors have migrated.

The next release cycle begins with adapter requests, spec proposals, and bug reports against the new version.

## Read next

- [proposing-a-spec-change.md](proposing-a-spec-change.md) (when written): how a spec change gets proposed and accepted.
- [docs/governance/protocol-versions.md](../governance/protocol-versions.md) (when written): version policy.
- [docs/reference/cids.md](../reference/cids.md): current catalog CID.
