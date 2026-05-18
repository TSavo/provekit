# 2026-05-18 #1068 Trinity exhibit status

Status: implementation complete in the working tree. Local commit creation is blocked by metadata write permission on the linked worktree Git directory.

Implemented:

- Added `implementations/rust/provekit-cli/tests/trinity_citation_comments_exhibit.rs`.
- Added the `trinity_citation_comments_exhibit` slow-test target in `implementations/rust/provekit-cli/Cargo.toml`.
- Added the dedicated `prove-trinity-citation-exhibit` slow-lane CI job in `.github/workflows/ci.yml`.
- Added the fixture README pointer in `menagerie/trinity-exhibit-fixtures/README.md`.

Verification run:

- `rustfmt --check implementations/rust/provekit-cli/tests/trinity_citation_comments_exhibit.rs`
- `cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-cli --test trinity_citation_comments_exhibit --features provekit-cli/slow-tests -- --list`
- `cargo test --release --manifest-path implementations/rust/Cargo.toml -p provekit-cli --test trinity_citation_comments_exhibit --features provekit-cli/slow-tests --no-run`
- Forbidden dash scan over touched artifacts with `rg -n '[\x{2013}\x{2014}]' ...`
- `git diff --check` for tracked changes and `git diff --no-index --check` for the new integration test file

Local full runtime was not run because this machine has no Java runtime:

```text
The operation couldn't be completed. Unable to locate a Java Runtime.
Please visit http://www.java.com for information on installing Java.
```

The new CI job provisions Java 21 and is the intended runtime lane for this test.

Commit blocker:

```text
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-1068-trinity-exhibit-impl/index.lock': Operation not permitted
```

An alternate temporary index and object directory successfully staged the change set and created a commit object, but copying the resulting objects into the repository object store also failed:

```text
cp: /Users/tsavo/provekit/.git/objects/18/2676e9d3373a2afe91bd71fc212ece1476bf53: Operation not permitted
```

This session cannot create files anywhere under `/Users/tsavo/provekit/.git`, including `objects`, `objects/info`, and the linked worktree gitdir. `HEAD` was not advanced.
