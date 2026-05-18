# #961 Proc Macro Invocation Status

## Result

Implemented the local code and audit changes for retiring the `procedural-macro`
loss class through first-class `concept:proc-macro-invocation` and
`concept:derive-attribute` operation applications.

The v3 audit artifact reports zero `procedural-macro` rows in
`handles-partially-with-loss-record`:

```text
procedural_macro_partial_rows=0
```

`bootstrap/audit-delta-v2-to-v3.md` records:

```text
| `procedural-macro` | 249 | 0 | -249 | #961 |
```

## Implemented

- Minted `concept:proc-macro-invocation` and `concept:derive-attribute` concept
  shapes under `menagerie/concept-shapes/`.
- Extended `provekit-walk` term emission to carry derive and attribute macro
  syntax as `concept:op-application` entries in `proc_macro_invocations`.
- Extended `walk_rpc` term mode to parse full source files so file-level
  derive and attribute macro context is preserved.
- Extended `provekit-realize-rust-core` to emit carried attribute token streams
  before realized Rust functions.
- Preserved proc macro invocation sidecars through libprovekit bind and lower
  request reconstruction.
- Removed `procedural-macro` from accepted surface-audit loss classification
  and generated v3 audit artifacts.
- Added six discrimination tests: three derive cases and three general
  attribute macro cases.

## Verification

```text
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-walk --test term_emit_proc_macro_invocation
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-walk --test term_emit_d3
cargo test --manifest-path implementations/rust/Cargo.toml -p provekit-realize-rust-core rpc_emits_proc_macro_invocations_before_realized_function
cargo test --manifest-path implementations/rust/Cargo.toml -p libprovekit --test lower_claim_bind_result
python3 -m py_compile bootstrap/scripts/libprovekit_audit_receipt.py bootstrap/scripts/libprovekit_surface_audit.py menagerie/concept-shapes/scripts/mint_proc_macro_invocation.py
python3 bootstrap/scripts/libprovekit_audit_receipt.py --skip-build --v1-csv bootstrap/libprovekit-surface-audit.v2.csv --csv bootstrap/libprovekit-surface-audit.v3.csv --gap-report bootstrap/libprovekit-gap-report.v3.md --delta bootstrap/audit-delta-v2-to-v3.md --baseline-label v2 --current-label v3 --phase-label post-961
git diff --check
```

All listed verification passed.

## Operational Notes

- No `gh` commands were run.
- No push or PR was attempted.
- Burned-five provekit-cli test files were not modified.
- No en dash or em dash characters were introduced in the diff or untracked
  files.

## Commit Blocker

Local commit could not be completed because this worktree cannot write Git
metadata:

```text
error: could not lock config file /Users/tsavo/provekit/.git/config: Operation not permitted
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-961-procmacro/index.lock': Operation not permitted
```

