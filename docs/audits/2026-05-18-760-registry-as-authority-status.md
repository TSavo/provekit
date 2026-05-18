# Issue 760 Registry As Dispatch Authority Status

Branch: `kit/pk-760-registry-as-dispatch-authority`
Worktree: `/Users/tsavo/provekit-worktrees/pk-760-registry-as-dispatch-authority`

## Completed

- Added content-addressed `PluginRegistryMemento` persistence under `.provekit/runs/<registry-cid>/plugin-registry-memento.json`.
- Added `.provekit/runs/` to `.gitignore` because run registries are generated run artifacts.
- Added process-local `kit_dispatch` registry sealing and caching keyed by workspace root.
- Made `dispatch_bind_lift`, `dispatch_realize`, and `dispatch_exam_manifest` try the sealed registry first.
- Made registry dispatch authorize plugin records against `PluginRegistryMemento.header.load_order`.
- Preserved the A2 filesystem fallback for unregistered kits and added the deprecation diagnostic.
- Activated CID-first federation: byte-equal registry CIDs federate immediately, different registry CIDs fall through to exam manifest compatibility checks.
- Added registry authority integration coverage for registered lift and realize dispatch, fallback diagnostics, structural CID determinism, and federation refusal.
- Updated the stale Python canonical body-template declared CID and matching self-consistency pin so `provekit-plugin-loader` passes.

## Verification

- `cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-cli --test registry_authority_dispatch_test`
- `cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-plugin-loader`
- `cargo build --manifest-path implementations/rust/Cargo.toml --workspace`
- `cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-cli`
- `git diff --check`
- Added-line scan found no U+2013 or U+2014 characters.

Known warning:

- `provekit-cli/src/cmd_bind.rs` still has the pre-existing unused `NamedTerm` import warning during CLI build/test.

## Commit Blocker

Local commit could not be created from this sandbox. Git can read the worktree state, but cannot create the linked worktree index lock:

```text
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-760-registry-as-dispatch-authority/index.lock': Operation not permitted
```

The changes are ready to commit outside this sandbox with:

```text
feat(kit_dispatch): consult sealed PluginRegistryMemento as authority (closes #760)
```

Requested identity:

```text
T Savo <evilgenius@nefariousplan.com>
```
