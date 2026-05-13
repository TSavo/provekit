# Writing a lift adapter, step 5: publishing

The adapter compiles, fixtures are green, cross-adapter parity holds. This step is the operational layer: naming, versioning, distribution, documentation.

## Naming

The canonical naming convention is `provekit-lift-<library-name>` per host language. Examples:

- Rust: `provekit-lift-proptest`, `provekit-lift-contracts`, `provekit-lift-kani`.
- TypeScript: `@provekit/lift-zod`, `@provekit/lift-class-validator`, `@provekit/lift-fast-check`.
- Python: `provekit.lift.pydantic`, `provekit.lift.deal`.
- Java: `provekit-lift-java-bean-validation`, `provekit-lift-java-jml`, `provekit-lift-java-spring-web`.
- C#: `Provekit.Lift.DataAnnotations`, `Provekit.Lift.Linq`.
- Ruby: `provekit-lift-active_model`, `provekit-lift-dry_validation`.

The host language's package idiom takes precedence over a strict naming rule: pick what looks normal in the ecosystem.

The library name follows the canonical name for the source library, hyphenated. `bean-validation`, not `BeanValidation`. `dry-validation`, not `Dry::Validation`. `class-validator`, not `class_validator`.

## Versioning

Two version axes:

1. **Adapter version**: tracks bug fixes and coverage additions. Semver.
2. **Source library version range**: tracks which versions of the source library this adapter supports.

A typical adapter package's `Cargo.toml` (or equivalent) declares:

```toml
[package]
name = "provekit-lift-zod"
version = "0.3.1"

[dependencies]
zod = ">=3.20, <4.0"  # source library compatibility range
```

If the source library makes a breaking change, the adapter must adapt or pin to a pre-break version. Adapters should track source-library churn and update.

## Coverage manifest

Each adapter ships a `COVERAGE.md` (or includes a section in the README) listing exactly what the adapter handles:

```markdown
# Coverage: provekit-lift-zod

## Handled

- z.string() with .min(N), .max(N), .length(N), .email(), .url(), .regex(R)
- z.number() with .int(), .min(N), .max(N), .positive(), .negative()
- z.boolean()
- z.object({ ... }) with nested schemas
- z.array(...) with .min(N), .max(N)
- z.optional(), z.nullable()
- z.union([ ... ]), z.intersection([ ... ])
- z.literal(V), z.enum([ ... ])

## Skipped (warns)

- z.string().datetime(): requires temporal sort, not yet in IR primitives
- z.preprocess(...): runtime transforms, not statically liftable

## Unrecognized (silently ignored)

- Custom validators (z.custom(fn)): by definition not statically liftable
- z.lazy(...): recursive schemas, planned for v0.4
```

This manifest is the contract: users know exactly what the adapter sees. The `docs/reference/per-adapter-coverage.md` aggregator pulls from each adapter's manifest.

## Distribution

Per host language idiom:

- Rust: `crates.io` via `cargo publish`. Yanking is rare; prefer a new version.
- TypeScript: `npm` via `npm publish` (or `pnpm publish`). Use the `@provekit/` org scope.
- Python: PyPI via `python -m build && twine upload`.
- Java: Maven Central via the standard publish flow.
- C#: NuGet via `dotnet pack && dotnet nuget push`.
- Ruby: RubyGems via `gem build && gem push`.
- Zig: Zig package manager (still evolving; pin in `build.zig.zon`).

For all distributions: include a clear README, a CHANGELOG, an MIT or Apache-2.0 license file, and a link back to the ProvekIt repository.

## Inclusion in `make conformance`

After the package is published (or even before, locally), the adapter is added to the kit's `make conformance` target. The adapter's fixtures run as part of the kit's CI. The adapter's coverage manifest is summarized in `docs/reference/per-adapter-coverage.md`.

This means every adapter's bytes are watched. A drift in the adapter's canonical IR output will fail CI even if the adapter's package version hasn't changed. This protects users from silent canonicalization changes.

## Documentation

A shipping adapter has at minimum:

1. **README** at the package root: install, usage example, link to the ProvekIt monorepo.
2. **COVERAGE** manifest as above.
3. **An entry in [`docs/reference/per-adapter-coverage.md`](../../reference/per-adapter-coverage.md)** in the monorepo: one paragraph summary, link to the adapter's package, coverage tier (A/B/C), known gaps.
4. **A tutorial mention** in the relevant per-language tutorial (`docs/tutorials/<language>.md`): "if your codebase uses [library], try [adapter]".

The tutorial mention is the user-facing surface; the COVERAGE manifest is the contract; the per-adapter-coverage entry is the index.

## Maintenance

Adapters age. Source libraries make breaking changes. Canonical predicates evolve when new ones are added to the IR vocabulary. Adapters need to track all of this.

Set up CI for the adapter that:

1. Re-runs conformance on every push.
2. Re-runs against the latest version of the source library on a weekly schedule (catches source-library breaks early).
3. Re-runs against the protocol catalog at `make conformance` (catches protocol bumps).

When all three are green, the adapter is healthy.

## Versioning across protocol bumps

When the protocol catalog version bumps (v1.1.0 → v1.2.0), every adapter potentially needs to update. New canonical predicates may exist; deprecated ones may need migration; the IR grammar may have new shapes.

The kit maintainers coordinate the bump. Adapter authors:

1. Update the adapter to use the new IR primitives where appropriate.
2. Re-run conformance fixtures; update pinned canonical bytes.
3. Bump the adapter version (typically a minor or patch bump, since breaking adapter changes are rare and would be a major).
4. Republish.

The bump cadence is slow (months between protocol versions). Adapter maintenance load is low.

## When this step is done

The adapter is published, its coverage is documented, its conformance fixtures run in CI, and the per-language tutorial mentions it. Users who land on the tutorial can install and use it immediately.

The adapter is now part of the substrate. Other adapters in other languages can target the same canonical predicates, achieving cross-domain equivalence for free.

## Read next

- [docs/contributing/release-process.md](../release-process.md) (when written): protocol bump coordination.
- [docs/reference/per-adapter-coverage.md](../../reference/per-adapter-coverage.md): the aggregator your adapter joins.
