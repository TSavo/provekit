# #975 Cluster Cardinality Status

Status: implementation complete, local commit blocked by sandbox gitdir writes.

## Change

- Added `candidateClusterManifest` to the bind named-term document.
- The manifest has `kind = "candidate-cluster-manifest"`, `schemaVersion = "1"`, `totalCandidates`, and per-cluster rows with `conceptCluster`, `candidateCount`, and `candidateCids`.
- Bind computes the view from emitted named terms, grouping by `conceptName` and counting candidate terms per concept cluster.
- Bind-result op-tree metadata carries the same manifest so `named_term_document_from_bind_payload` recovers it for CLI and path consumers.
- Older bind-result payloads without the field derive the manifest during recovery.

## Verification

- `cargo test --manifest-path implementations/rust/Cargo.toml -p libsugar --test bind_kit -- --nocapture`
- `cargo test --manifest-path implementations/rust/Cargo.toml -p sugar-cli --test cmd_bind_integration -- --nocapture`
- `cargo test --manifest-path implementations/rust/Cargo.toml -p libsugar --test lower_claim_bind_result -- --nocapture`
- `cargo test --manifest-path implementations/rust/Cargo.toml -p sugar-cli --test bind_kit_path_integration -- --nocapture`
- `git diff --check`
- added-line unicode dash check

## Blocker

Local commit is blocked because git cannot write the linked worktree index lock:

```text
fatal: Unable to create '/Users/tsavo/sugar/.git/worktrees/pk-975-cluster-cardinality/index.lock': Operation not permitted
```

Direct write probe to the same gitdir also fails:

```text
touch: /Users/tsavo/sugar/.git/worktrees/pk-975-cluster-cardinality/codex-write-test: Operation not permitted
```
