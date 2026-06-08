# Recursive Composition Status

Issue: #1023
Branch: kit/pk-1023-recursive-composition

## Implemented

- Added recursive realization in `implementations/rust/libsugar/src/core/lower_plugin.rs`.
- The lower adapter now descends explicit `namedTermTree` composition nodes when they declare `compositionPoint` or `composition_point`.
- Child concepts are realized before parent concepts through the same `RealizeTransport`.
- Child realization records are carried through existing `operand_bindings` as `kind = "recursive-child-realization"`.
- Supported composition points are `before`, `after-return`, `after-throw`, and `around`.
- Parent realization claims cite child claim CIDs as premises.
- Child observed loss records are merged into the final parent `observed_loss_record`.
- Structural errors and child transport refusals fail before parent dispatch.

## Tests

Added `implementations/rust/libsugar/tests/recursive_composition_lower.rs` covering:

- two-node after-return wrapper preserving the child result
- child loss aggregation into the parent realized output
- missing child realization refusal without parent dispatch
- all four declared composition points
- unknown composition point structural refusal
- malformed child tree structural refusal

Commands run:

```text
cargo test -p libsugar --test recursive_composition_lower
cargo test -p libsugar --test lower_claim_bind_result
cargo test -p sugar-cli --test lower_kit_path_integration
rustfmt --check implementations/rust/libsugar/src/core/lower_plugin.rs implementations/rust/libsugar/tests/recursive_composition_lower.rs
git diff --check
```

All listed commands passed. `sugar-cli --test lower_kit_path_integration` emitted the pre-existing `NamedTerm` unused import warning from `sugar-cli/src/cmd_bind.rs`.

## Commit Blocker

The requested local commit could not be created because this sandbox cannot write Git metadata for the linked worktree:

```text
fatal: Unable to create '/Users/tsavo/sugar/.git/worktrees/pk-1023-recursive-composition/index.lock': Operation not permitted
```

Direct probe writes under `/Users/tsavo/sugar/.git/worktrees/pk-1023-recursive-composition` and `/Users/tsavo/sugar/.git/objects` also fail with `Operation not permitted`. The working tree changes are present, but staging and committing are blocked by Git metadata write restrictions.
