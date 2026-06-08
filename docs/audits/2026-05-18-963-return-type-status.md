# #963 return-type-user-defined retirement status

Implemented in the worktree, but local commit creation is blocked by the gitdir filesystem metadata.

## Completed

- Extended `concept:sort` to carry `generic_args`.
- Added a minted `concept:sort` catalog artifact for the new 3-argument shape:
  `blake3-512:8d9e3d54325e7a123528a38f7fc268c64a2dfe9a43fca08a234dd530015c7e53f89510093404db9ab63cdcfee59fe1de712f1b7ac6a736475ae2f090c1d2eab0`.
- Updated the Rust term lifter to emit `return_sort` as ctor JSON with generic args.
- Removed emission of `return-type-user-defined` for return types that can be represented as concept sorts.
- Kept existing partial loss classes for `Result`, `Option`, `Vec`, byte vec, and byte array while adding structured `return_sort`.
- Added discrimination coverage for bare user-defined, top-level parametric, nested parametric, Option, Vec, and qualified generic return sorts.

## Verification

- `cargo test -p provekit-walk --test term_emit_d2 -- return_type --nocapture`
- `cargo test -p provekit-walk --test term_emit_d2`
- `cargo test -p provekit-walk`
- `python3 -m py_compile menagerie/concept-shapes/scripts/mint_core_sorts.py`
- `jq empty menagerie/concept-shapes/specs/sort_shape.spec.json deleted concept-shapes catalog/index.json`
- `git diff --check`
- Touched-file scan for en dash and em dash returned no matches.

## Blocker

`git add` cannot create the worktree index lock:

```text
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-963-return-type/index.lock': Operation not permitted
```

Alternate-index staging also cannot write objects:

```text
error: unable to create temporary file: Operation not permitted
error: implementations/rust/provekit-walk/src/emit.rs: failed to insert into database
error: unable to index file 'implementations/rust/provekit-walk/src/emit.rs'
fatal: updating files failed
```

The affected gitdir and object database carry `com.apple.provenance`, and removing the attribute is also refused:

```text
xattr: [Errno 1] Operation not permitted: '/Users/tsavo/provekit/.git/worktrees/pk-963-return-type'
```

No GitHub writes were performed.
