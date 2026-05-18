# Rust Boundary Widening Status

Date: 2026-05-18
Branch: `kit/pk-1188-rust-boundary-widening`

## Completed

- Added Rust boundary realization records for:
  - `concept:dynamic-dispatch` as `rust:dyn-trait-object`
  - `concept:closure` as `rust:closure-expression`
  - `concept:iterator` as `rust:iterator-trait`
  - `concept:generic-instantiation` as `rust:monomorphization`
  - `concept:reference` as `rust:shared-reference`
- Updated the concept-shape catalog index for the five new realization records.
- Added Rust canonical body templates for all five concepts with `realization_kind = boundary-realization`, target library tags, and loss-record contributions.
- Updated `provekit-realize-rust-core` so boundary body templates emit concrete Rust bodies and carry observed loss evidence before the sugar-carrier fallback path.
- Added three tests per concept emission: positive rendering, structural loss evidence, and discrimination against missing concepts.
- Removed an unused CLI re-export so `cargo build --workspace` finishes without warnings.

## Verification

- `python3 tools/classify-realization-tags.py`
  - Rust row is now `| rust | 37 | 0 | 12 | 14 | 0 | 63 |`.
- `cargo test -p provekit-realize-rust-core`
  - Passed.
- `cargo test -p libprovekit`
  - Passed.
- `cargo build --workspace`
  - Passed without warnings after the CLI unused import cleanup.
- `git diff --check`
  - Passed.

## Blocker

The code is implemented and verified, but local Git metadata in this worktree is not writable from the current sandbox, so I could not rebase or create the requested local commit.

Observed failures:

```text
git stash push -u -m pk-1188-rust-boundary-widening-wip
error: could not write index
```

```text
git add ...
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-1188-rust-boundary-widening/index.lock': Operation not permitted
```

Direct write probe also failed:

```text
touch /Users/tsavo/provekit/.git/worktrees/pk-1188-rust-boundary-widening/index.lock
touch: /Users/tsavo/provekit/.git/worktrees/pk-1188-rust-boundary-widening/index.lock: Operation not permitted
```

The current branch is also behind local `origin/main` by 9 commits. The intended next step is to rebase the finished changes onto `origin/main`, rerun the same checks, and create the local commit referencing #1170, #1171, #1172, #1173, and #1174 once `.git` writes are available.
