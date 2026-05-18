# #979 name lifecycle status

Branch: `kit/pk-979-name-lifecycle`
Worktree: `/Users/tsavo/provekit-worktrees/pk-979-name-lifecycle`

## Implemented

- Rust bind lift now reads an immediately preceding `// concept: NAME` comment and emits it as `concept_annotation` for the bind entry.
- The bind result payload sanitizer consumes `concept_annotation`, `attr_pre`, and `attr_post` into the named result and strips those lift-side keys from the source-term half of `concept:bind-result`.
- Added a CLI lifecycle integration test covering lift, bind, lower, edit `// concept: ...`, relift, and bind to `concept:my-thing`.
- Documented the name lifecycle loop in `protocol/specs/2026-05-13-bind-ir-lift-result.md`.

## Verification

- `cargo test -p provekit-walk --bin provekit-walk-rpc`
- `cargo test -p provekit-cli --test verb_composition`
- `cargo test -p provekit-cli --test cmd_bind_integration`
- `cargo test -p libprovekit core::bind`
- `git diff --check`
- Diff scan found no en dash or em dash characters in the patch.

## Blocker

Local commit is blocked by sandbox permissions on the main gitdir:

```text
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-979-name-lifecycle/index.lock': Operation not permitted
```

Direct write probes also fail with `Operation not permitted` under:

- `/Users/tsavo/provekit/.git/worktrees/pk-979-name-lifecycle`
- `/Users/tsavo/provekit/.git/objects`
- `/Users/tsavo/provekit/.git/refs`

The worktree files are writable, but this session cannot write Git metadata, so the local commit could not be created here.
