# #964 FFI call effect-occurrence status

Status: implementation complete, verification passed, local commit blocked by sandbox permission on shared Git worktree metadata.

Worktree: `/Users/tsavo/sugar-worktrees/pk-964-ffi-call`
Branch: `kit/pk-964-ffi-call`

## Implemented

- Rust walk file-aware term emission now carries collected `extern` function declarations into lowering.
- Matched FFI calls emit structured `EffectOccurrence` records with `occurrence_kind: "UnresolvedCall"`, `role: "body"`, Rust `UnresolvedCall` signature CID, ABI, binding, file locator, and discharge key.
- `#[link_name = "..."]` is honored for the occurrence payload and discharge key while preserving the Rust binding in the locator.
- Plain Rust function calls and method calls no longer emit the retired `ffi-call-unresolved-effect` loss.
- Unsafe blocks pass through term lowering so FFI calls inside `unsafe { ... }` still emit effect occurrences.
- Added 12 discrimination tests covering tail, let-RHS, statement, link-name, local-call negative, and unsafe variants.

## Verification

- `cargo test -p sugar-walk --test ffi_effect_occurrence`
- `cargo test -p sugar-walk`
- `rg -n "ffi-call-unresolved-effect" implementations/rust/sugar-walk/src implementations/rust/sugar-walk/tests` found no active source/test references.
- Forbidden dash scan over the changed Rust files found no em dash or en dash characters.
- `git diff --check`

## Commit blocker

`git add` and commit could not run because Git cannot create the worktree index lock:

```text
fatal: Unable to create '/Users/tsavo/sugar/.git/worktrees/pk-964-ffi-call/index.lock': Operation not permitted
```

A direct write test in `/Users/tsavo/sugar/.git/worktrees/pk-964-ffi-call` also fails with `Operation not permitted`, so this runner cannot update the Git index or refs for the linked worktree.
