# Golden: self-check on provekit-shim-rust-std (no oracle)

## What this captures

The deterministic scoreboard from `provekit self-check --target examples/provekit-shim-rust-std --json`
run WITHOUT `--oracle`. `oracle.requested=false` makes the output fully deterministic: no
rust-analyzer daemon, no wall-clock, no absolute paths.

## Honest numbers (2026-05-31)

- `silentlyDropped: 0` -- hard invariant, must stay zero
- `falsePass: 0` -- hard invariant, must stay zero
- `panicSafe: 0` -- honest: the shim has 4 syntactic panic sites, none guarded, none dischargeable without the oracle
- `panicCensus`: 4 sites, all `src/lib.rs`, all `unproven`; the reason line is the verifier's refuse-floor message
- `catalogCid` is a content hash of the lifted contracts; it is path-independent (verified 2026-05-31 across two checkout paths)

## Normalization applied

None. The output is byte-stable and path-independent without normalization:
- `file` fields are repo-relative within the target crate (`src/lib.rs`), not absolute paths
- No timestamps in the scoreboard struct
- All arrays (`panicCensus`, `droppedSites`) are produced via `BTreeMap::into_values()` or `sort_by(site_cmp)`, both deterministic
- `catalogCid` is a BLAKE3 hash over content, not over path

## Regenerating

When a change legitimately moves a number (e.g., a new discharge tier closes some panicCensus entries):

```
UPDATE_GOLDEN=1 cargo test -p provekit-cli --test self_check_golden
```

The test rewrites this `.json` file with the new output. Update this `.md` with a one-line why:
> e.g. "panicSafe 0 -> 3: Tier D-lib closes serde_json::Value sites (#NNNN)"
