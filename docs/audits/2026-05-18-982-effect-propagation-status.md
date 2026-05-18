# #982 Effect Propagation Status

Status: implementation complete, local commit blocked by git metadata permissions.

Branch: `kit/pk-982-effect-propagation`

Implementation:

- `implementations/rust/libprovekit/src/core/bind.rs`
  - Normal bind now parses an optional top-level `effectPropagation` or `effect_propagation` graph.
  - Normal bind calls `effect_propagation::propagate_effects`.
  - Widen decisions are emitted as `promotionDecisionMementos`.
  - Halt decisions are emitted as `effectHaltMementos`.
  - Refuse decisions are emitted as `effectRefusalMementos`.
  - The bind-result op-tree metadata round trip preserves the new report fields.

- `implementations/rust/libprovekit/src/effect_propagation.rs`
  - Propagation graph structs now deserialize from snake_case and selected camelCase fields.

Tests:

- Red check confirmed before implementation:
  - `cargo test -p libprovekit bind_effect_propagation`
  - Result before implementation: 9 failed, covering 3 Widen tests, 3 Halt tests, and 3 Refuse tests.

- Verification after implementation:
  - `cargo test -p libprovekit`
  - Result: passed.
  - `cargo test -p provekit-cli --test cmd_bind_integration`
  - Result: passed.
  - `cargo test -p provekit-cli --test migrate_async_rewrite_test`
  - Result: passed.
  - `rustfmt --check implementations/rust/libprovekit/src/core/bind.rs implementations/rust/libprovekit/src/effect_propagation.rs`
  - Result: passed.
  - `git diff --check`
  - Result: passed.
  - Diff dash check for em dash and en dash
  - Result: no matches.

Commit blocker:

```text
fatal: Unable to create '/Users/tsavo/provekit/.git/worktrees/pk-982-effect-propagation/index.lock': Operation not permitted
```

Reproduced by:

```text
git add implementations/rust/libprovekit/src/core/bind.rs implementations/rust/libprovekit/src/effect_propagation.rs
```

Direct writes to `/Users/tsavo/provekit/.git/worktrees/pk-982-effect-propagation` also fail with `Operation not permitted`, so local commit cannot be completed from this sandbox.
