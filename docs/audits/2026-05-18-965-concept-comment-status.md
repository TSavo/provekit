# Issue 965 concept comment status

## Status

Implementation is present in the worktree, but local commit is blocked by filesystem policy on the shared git metadata.

Branch:

```text
kit/pk-965-concept-comment
```

Commit blocker:

```text
git add -A
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-965-concept-comment/index.lock': Operation not permitted
```

Local git identity config is also blocked:

```text
git config user.name "T Savo"
error: could not lock config file /Users/tsavo/provekit/.git/config: Operation not permitted
```

Direct write probe confirms the restriction is on git metadata, not the worktree:

```text
touch /Users/tsavo/provekit/.git/worktrees/pk-965-concept-comment/codex-write-test
touch: /Users/tsavo/provekit/.git/worktrees/pk-965-concept-comment/codex-write-test: Operation not permitted
```

No GitHub writes were performed.

## Implemented

- Minted `concept:comment(surface)` shape spec and generated catalog entries.
- Added `mint_comment.py` and wired it into `menagerie/concept-shapes/mint.sh`.
- Added minter catalog compatibility for existing `abstraction`, `realization`, and `receipt` index entries so comment minting can run against the current concept catalog.
- Rust bind lift recognizes line comments and block comments as `concept:comment` occurrences, excludes provekit carrier comments, and avoids markers inside ordinary quoted strings.
- Python bind lift recognizes full-line Python comments as `concept:comment` occurrences and excludes provekit carrier comments.
- Python lower emits comment-only functions with `pass` when needed for valid Python syntax.
- Rust lower emits `concept:comment` as a Rust comment and preserves Rust comment surfaces that came through a Python comment hop.

Comment shape CID:

```text
blake3-512:d9c806063bb97d59ca655b6c50b6ad2ff4cbadd02d6238a51a33a63ec6626af6d92e338ca10f9598fa322cd960d007349752f477fdf3a9384491282d8d12fef2
```

## Verification

Fresh checks run after implementation:

```text
pytest implementations/python/provekit-lift-python-source/tests/test_bind_lifter.py -q
41 passed in 0.48s
```

```text
pytest implementations/python/provekit-realize-python-core/tests/test_realizer.py -q
44 passed in 8.51s
```

```text
CARGO_TARGET_DIR=/private/tmp/pk-965-target cargo test -p provekit-walk --bin provekit-walk-rpc -- --nocapture
test result: ok. 19 passed; 0 failed
```

```text
CARGO_TARGET_DIR=/private/tmp/pk-965-target cargo test -p provekit-realize-rust-core -- --nocapture
test result: ok. 16 passed; 0 failed
test result: ok. 3 passed; 0 failed
```

```text
cargo test -p provekit-mint-amp
test result: ok. 1 passed; 0 failed
test result: ok. 3 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 1 passed; 0 failed
```

```text
cargo build -p provekit-cli -p provekit-canonicalizer
Finished `dev` profile
```

The build still reports the pre-existing warning in `provekit-cli/src/cmd_bind.rs` for unused import `NamedTerm`.

```text
python3 menagerie/concept-shapes/scripts/mint_comment.py
comment_shape_cid concept:comment blake3-512:d9c806063bb97d59ca655b6c50b6ad2ff4cbadd02d6238a51a33a63ec6626af6d92e338ca10f9598fa322cd960d007349752f477fdf3a9384491282d8d12fef2
```

```text
git diff --check
exit 0
```

Unicode dash check over touched files found no en dash or em dash characters.

## Files changed

- `implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/bind_lifter.py`
- `implementations/python/provekit-lift-python-source/tests/test_bind_lifter.py`
- `implementations/python/provekit-realize-python-core/src/provekit_realize_python_core/realizer.py`
- `implementations/python/provekit-realize-python-core/tests/test_realizer.py`
- `implementations/rust/provekit-mint-amp/src/catalog.rs`
- `implementations/rust/provekit-mint-amp/src/lib.rs`
- `implementations/rust/provekit-mint-amp/tests/mint_then_read_round_trips.rs`
- `implementations/rust/provekit-realize-rust-core/src/lib.rs`
- `implementations/rust/provekit-walk/src/bin/walk_rpc.rs`
- `menagerie/concept-shapes/README.md`
- `deleted concept-shapes catalog/index.json`
- `menagerie/concept-shapes/cids.tsv`
- `menagerie/concept-shapes/mint.sh`
- `menagerie/concept-shapes/scripts/mint_comment.py`
- `menagerie/concept-shapes/specs/comment_shape.spec.json`
- `menagerie/concept-shapes/specs/op-definitions/concept:comment.op-def.ccl.json`
- `menagerie/concept-shapes/specs/op-definitions/index.cids.json`
