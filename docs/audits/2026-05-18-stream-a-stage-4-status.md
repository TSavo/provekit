# Stream A Stage 4 status

Issue: #1147
Branch: kit/pk-1147-stream-a-stage-4

## Progress

- Added `DimensionValueMemento` carriers and op aliases to `PlatformSemanticsDeclaration`.
- Added `PlatformSemanticsDeclaration::compare_op_with`, returning exact, divergent, or uncharacterizable comparison results.
- Wired per-callsite platform semantic divergences into `ChangedCallsite.effect` using the target `DimensionValueMemento` CID as the effect name.
- Added `--focus` to migration bind arguments and scoped changed callsites to the focused callsite.
- Added `loss_dimensions` to `LossRecordMemento`, populated with `IrFormula::DivergenceBetween` for platform semantic divergences.
- Preserved the existing async propagation and legacy sqlite `last_insert_rowid` loss path.

## Passing checks

- `cargo test -p libprovekit --test core_interface platform_semantics_compare_op -- --nocapture`
- `cargo test -p provekit-cli point_query -- --nocapture`
- `cargo test -p libprovekit --test core_interface`
- `cargo test -p provekit-ir-types --test platform_semantics_mementos`
- `cargo test -p provekit-realize-rust-core --test platform_semantics`
- `cargo test -p provekit-cli --test migrate_async_rewrite_test -- --nocapture`
- `cargo test -p provekit-cli --test stage3_cross_language_test -- --nocapture`

## Stop condition

The protected burned test command failed:

```text
cargo test -p provekit-cli --test slice2_java_realize_plugin_byte_identical --test mint_kit_integration --test trinity_roundtrip_test --test verb_composition --test cli_surface
```

Failure:

```text
cli_surface::lift_zig_shows_production_composes_but_unit_tests_conflict failed
error: unable to update file from .../.zig-local-cache/.../provekit-lift-zig
to .../implementations/zig/provekit-lift-zig/zig-out/bin/provekit-lift-zig:
PermissionDenied
```

I stopped after this failure, per the burned-test rule. The failure is in the Zig install step writing `implementations/zig/provekit-lift-zig/zig-out/bin/provekit-lift-zig`, before the lift plugin can answer RPC.

## Commit status

Not committed in this pass. Earlier git metadata writes also failed in this linked worktree with:

```text
cannot lock ref 'ORIG_HEAD': Unable to create .../provekit/.git/worktrees/pk-1147-stream-a-stage-4/ORIG_HEAD.lock: Operation not permitted
```
