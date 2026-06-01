# Golden: self-check on provekit-shim-rust-std (no oracle)

## What this captures

The deterministic scoreboard from `provekit self-check --target examples/provekit-shim-rust-std --json`
run WITHOUT `--oracle`. `oracle.requested=false` makes the output fully deterministic: no
rust-analyzer daemon, no wall-clock, no absolute paths.

## Honest numbers (2026-05-31)

- `silentlyDropped: 0` -- hard invariant, must stay zero
- `falsePass: 0` -- hard invariant, must stay zero
- `panicSafe: 0` -- honest: the shim has 5 syntactic panic sites, none guarded, none dischargeable without the oracle
- `panicCensus`: 5 sites, all `src/lib.rs`, all `unproven`; the reason line is the verifier's refuse-floor message
- `catalogCid` is a content hash of the lifted contracts; it is path-independent (verified 2026-05-31 across two checkout paths)

## Regenerated 2026-06-01 (panic-locus branch)

> `reflexive 629 -> 656` / `undecidable 1303 -> 1276`: the locus-branch lift
> disambiguation (type-driven `serde_json::to_string::<Value>` -> distinct ctor,
> plus `panic_loci` provenance threading) supplies body-derived contracts for 27
> more sites, clustered in the CID/serialization functions (`term_cid`, `cid`,
> `canonical_bytes`, ...). `wp` reduces each via the real body (ground truth, not
> the post-as-axiom), so undecidable -> reflexive is a sound CLOSING, not a
> masking. `catalogCid` changed for the same reason (lifted-contract content
> changed). `+4 unsupported-macro-callsite` liftGaps are previously untracked
> gaps now surfaced honestly (the no-silent-failure system), NOT drops:
> `silentlyDropped` stays 0. Hard invariants unchanged: `falsePass 0`,
> `panicSafe 0`, `silentlyDropped 0`; `panicCensus` still the same 4 unproven
> sites. (No oracle here, so the serde panic-safe discharge does not appear; it
> is exercised by the warm-oracle e2e on stage3-serde-totality-fixture.)

Refreshed 2026-06-01: `catalogCid` `0f9278...` -> `5943fb5b...`
after pre-existing main drift; baseline `7daf1918a` (before this PR's
lift-direct changes) produces `5943fb5b...`, with member set and decoded
member JSON unchanged from this branch. Hard invariants unchanged.

Regenerated 2026-06-01: `catalogCid` `5943fb5b...` -> `b17a0f02...`.
`Result::expect` was added as a distinct rust-std partial, mirroring
`Result::unwrap` with precondition `result.is_ok()`. This intentionally adds one
function contract (`27 -> 28`), one unproven panic site in the no-oracle shim
self-check (`panicCensus 4 -> 5`, new `expect` at `src/lib.rs:190`), and one
surfaced `assert!` macro gap (`unsupported-macro-callsite 4 -> 5`). Hard
invariants unchanged: `falsePass 0`, `panicSafe 0`, `silentlyDropped 0`,
`droppedSites []`.

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
