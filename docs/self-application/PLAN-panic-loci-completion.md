# Panic-locus completion plan (#1748 + #1749 + production K)

Concrete next-step plan after #1747 landed. Pick this up cold: it states the live
state, the recommended order with reasoning, the files to inspect, the tests to
write first, the minimal implementation shape that preserves the four
invariants, the verification commands, the tripwires, and the advisor
checkpoints. Coordinator (T) is driving; advisor is read-only and consulted at
the marked checkpoints.

## Current state (verified 2026-06-01)

- `main` at `dff308cc5` after #1756 merged.
- **#1747 merged.** Panic-locus preservation + guard-branch routing landed.
  `serde_json::to_string(v: &Value).unwrap()` discharges **panic-safe** on the
  warm-oracle `stage3-serde-totality-fixture` e2e; `to_string(s: &MyStruct)`
  stays honestly undecidable. Hard invariants held throughout:
  `silentlyDropped=0`, `falsePass=0`.
- **#1750 merged.** cmd_mint now fails closed on malformed `panicLoci` instead
  of silently defaulting dropped provenance to empty.
- **#1752 merged and closed #1748.** walk_rpc emits the producer receiver start
  line for split/spanning receiver `.unwrap()` sites.
- **#1753 merged.** self-check oracle convergence is explicit and logged.
- **#1755 merged.** self-check guards mid-run `.provekit/imports` mutation.
- **#1756 merged.** provekit-walk single-contract envelope now threads
  `panic_loci` into contract headers and keys `EnvelopeCache` by
  `contract_cid` plus panic-loci fingerprint.
- **Open follow-up:**
  - **#1749** - remaining `provekit-lift` direct mint path panic_loci
    threading. cmd_mint and provekit-walk envelope are done.
- **Production K measurement exists.** Per GOAL, provekit-cli currently reports
  `silentlyDropped=0`, `falsePass=0`, `panicSafe=0`, `panicCensus=32`, with
  every unproven site named by category. Next K movement comes from the Phase-2
  D-lib per-type slice, not more panic-locus plumbing.

## Recommended order

**(a) #1749 fail-closed slice first** - done by #1750.
**(b) Production K measurement on provekit-cli** - done and recorded in GOAL.
**(c) #1748 or the #1749 heavy-lift surfaces next** - #1748 is done by #1752;
provekit-walk envelope is done by #1756; provekit-lift direct remains.

### Why this order

- The fail-closed slice is the only thread that touches the
  `silentlyDropped=0` invariant. Everything else in the follow-ups is
  safe-direction (a missed lookup yields `None` -> undecidable, never
  false-pass). Hardening the floor before widening surface area prevents a
  silent-drop bug from contaminating the first production K measurement.
- The production K measurement is the goal-hook signal, not more
  completeness work on the mechanism. It tells us which Phase-2 tier to
  prioritize next (D-lib Value totality, C `json!` construction, B
  intra-fn dataflow, or D-fn cross-function postconditions) based on what
  the gap census actually shows on provekit-cli's 37 sites.
- #1748 and the heavy-lift #1749 surfaces are safe-direction completeness;
  they cost nothing to defer because they cannot regress invariants.

## Exact files / functions to inspect

### Before touching anything

- `implementations/rust/provekit-cli/src/cmd_mint.rs` ~lines 1726-1746 - the
  contract decl extraction with the `unwrap_or_default()` silent-drop bug at
  1736-1741.
- `implementations/rust/provekit-cli/src/cmd_mint.rs` ~lines 1800-1840 -
  check the adjacent `MintBridgeArgs` block for a sibling silent-drop on
  `panicLoci` or `bridgeCallsite`.
- `implementations/rust/provekit-canonicalizer/src/lib.rs` (wherever
  `json_to_cvalue` is defined) - confirm it cannot itself swallow a
  structural problem.
- `implementations/rust/provekit-walk/src/bin/walk_rpc.rs` `collect_panic_loci`
  and the per-decl emitter - that defines what well-formed `panicLoci`
  looks like, which sets the rejection shape on the consumer side.

### Change shape (cmd_mint.rs:1736-1741)

```rust
let panic_loci: Vec<std::sync::Arc<provekit_canonicalizer::Value>> =
    match decl.get("panicLoci").or_else(|| decl.get("panic_loci")) {
        None => Vec::new(),
        Some(v) if v.is_array() => v
            .as_array()
            .unwrap()
            .iter()
            .map(json_to_cvalue)
            .collect(),
        Some(other) => {
            return Err(format!(
                "decl {:?}: panicLoci must be an array, got {:?}",
                decl_name, other_kind
            ));
        }
    };
```

The whole fail-closed move is the absent-vs-present-malformed distinction.
Nothing else moves.

## Tests to write first (TDD)

Place in `implementations/rust/provekit-cli/tests/cmd_mint_panic_loci_extraction.rs`
(new file) or alongside existing cmd_mint tests:

1. **`panic_loci_absent_yields_empty`** - decl with no `panicLoci` field
   mints cleanly; the contract memento's `panicLoci` header is empty.
   *Asserts:* the legitimate-empty path stays a soft no-op.
2. **`panic_loci_malformed_fails_closed`** - three sub-cases:
   `panicLoci: "not-an-array"`, `panicLoci: 42`, `panicLoci: {}`. Each
   returns `Err` *before* a contract is minted.
   *Asserts:* present-but-not-an-array is a hard error, not a silent empty.
   This is the discrimination test that proves the floor.
3. **`panic_loci_well_formed_threads_through`** - decl with a valid
   `panicLoci: [{argTerm, file, line, col, callee}]` array mints a
   contract whose header carries that exact payload, byte-for-byte
   (canonicalizer-round-tripped).
   *Asserts:* the happy path didn't regress under the fail-closed
   reshaping.

Optional fourth (regression net, not strictly TDD): rerun the
`stage3-serde-totality-fixture` warm-oracle e2e after the change and
assert `dischargeSplit` byte-stable vs the #1747 result (`panicSafe=1,
falsePass=0, silentlyDropped=0`, f@25 panic-safe, g@38 undecidable).

## Minimal implementation shape (preserves all four invariants)

- **CLI language-agnostic** - the extraction reads opaque JSON; `panicLoci`
  is an opaque array of objects, no Rust-specific keys interpreted by the
  CLI. `json_to_cvalue` is a structural lift. Unchanged.
- **Kits own language semantics** - the shape of an entry (`argTerm`,
  `file`, `line`, ...) is set by walk_rpc; the CLI only validates "is this
  an array?". No semantic interpretation moves into the CLI.
- **Proof data over RPC** - untouched. `panicLoci` is already carried in
  the decl JSON the kit returns over linkerd RPC; this is a sink-side
  validation only.
- **No silent failure** - the point. `Err(...)` propagates to the CLI's
  nonzero exit. If the no-silent-failure system has a counter to
  increment, use it; otherwise the propagating error is the loud signal.

### Anti-patterns to avoid

- Silently coercing malformed values (wrapping a non-array singleton into
  a one-element array). Different shape of silent degradation, same crime.
- Sleepwalking `unwrap_or_default()` removal into adjacent extractions
  (`formals_json`, `body_discharge_eligible`, etc.). Those have their own
  contracts; not part of this slice.
- Touching `json_to_cvalue` to "make it total". Keep it as it is; the
  rejection happens at the array-check.

## Verification commands (bcargo for Rust only)

From the repo toplevel, in `implementations/rust`:

```sh
# Unit tests for the slice
./bin/bcargo test -p provekit-cli --test cmd_mint_panic_loci_extraction

# Broader provekit-cli suite (regression net for adjacent extractions)
./bin/bcargo test -p provekit-cli

# Self-check golden - MUST be byte-stable for this slice
./bin/bcargo test -p provekit-cli --test self_check_golden
```

Then the soundness gate (the warm-oracle e2e from #1747's harness; the
script already lives on battleaxe at `/tmp/serde_e2e_locus2.sh` and
points at the bcargo remote root for the worktree it was built in - if
you run from a fresh checkout, regenerate the harness so it points at
the new remote root):

```sh
ssh battleaxe 'bash /tmp/serde_e2e_locus2.sh'
# Expect: dischargeSplit {panicSafe: 1, falsePass: 0, silentlyDropped: 0, ...},
# f@25 panic-safe, g@38 undecidable.
```

CI gates that matter on the PR (mirroring #1747): `prove`×2,
`Spec CID literal lint`, `Cross-language conformance gate` (Linux).
macOS-swift is the known inherited red.

## Risks / tripwires (stop or revert if these fire)

- **Any previously-passing test now fails with the fail-closed error.**
  Means some real path mints with malformed `panicLoci` you didn't know
  about. **Do not soften the validation to make the test pass.** Stop,
  investigate the lifter source, fix at the source.
- **Golden shifts.** A correct fail-closed change cannot move any number
  in the golden (it only rejects what was already being dropped). If the
  golden moves, you have an unintended side effect. Read the diff. Do
  not regenerate blindly.
- **e2e regresses `falsePass`, `panicSafe`, `silentlyDropped`,
  `droppedSites`, or `panicCensus`** from #1747's verified state.
  **Hard stop.** This is the floor.
- **`unwrap_or_default()` removed elsewhere by accident.** Grep your
  diff: the only `unwrap_or_default()` change should be the panicLoci
  one. Adjacent extractions have their own contracts.
- **`json_to_cvalue` panics on a value type it previously coerced.**
  Propagate via `try_collect` or equivalent; the CLI must never panic on
  user data.
- **Two reverts in a row.** That's the "3+ fixes failed = architectural
  problem" pattern. Stop coding and surface to advisor.

## Production K measurement (the load-bearing step after the slice)

The production K measurement has landed in GOAL. Keep this harness section as
the recipe for reruns after any future panic-locus or K-moving slice.

### Harness

Mirror the warm-oracle protocol from `/tmp/serde_e2e_locus2.sh` but
point at provekit-cli instead of the fixture:

```
PROVEKIT_RESOLVE_ORACLE=rust-analyzer
PROVEKIT_RUST_ANALYZER=<nightly toolchain rust-analyzer>
PROVEKIT_LINKERD_BIN=<provekit-linkerd binary>
PROVEKIT_LINKERD_SOCKET=<unix socket path>
# Warm-up mint, then resolve mint within 5 min, then self-check / prove
```

Target: `implementations/rust/provekit-cli`.

### What to report

- `dischargeSplit` (panicSafe, falsePass, silentlyDropped must be
  visible).
- Per-site row for each of the 37 unwrap/expect sites: file, line,
  callee, status, dischargeMethod, reason. Categorize each by tier
  (D-lib totality, C json! construction, B intra-fn dataflow, D-fn
  cross-function post, or residue).
- A named gap census: every unproven site labelled
  `{category, tier-to-close}`. A number without an attributable category
  is the red-flag pattern from the goal hook.

### Acceptance for this step

It is OK for K to be small or zero on the first run. What is NOT OK is
an unattributable number. The honesty of the census is the deliverable
here, not the size of K.

## Advisor checkpoints (when to come back)

1. **Before code** - paste a real `panicLoci` JSON sample from a fresh
   mint so the fixture data in the tests is grounded.
2. **After tests written, before implementation** - paste the three
   tests' assertions. Confirm the discrimination test catches all three
   malformed shapes (string / number / object) and the positive test
   asserts byte-stable header content, not just absence of error.
3. **After implementation, before pushing** - paste the diff (should be
   ~10-20 LOC) plus the e2e + golden results.
4. **If the e2e moves any of `falsePass`, `panicSafe`,
   `silentlyDropped`, `droppedSites`, or `panicCensus`** - come back
   immediately. The fail-closed slice cannot move any of these.
5. **Before opening the PR** - paste the PR body. It should name the
   invariant being hardened (`silentlyDropped=0`), the discrimination
   pair as the test, and a one-line confirmation that the e2e + golden
   are unchanged.
6. **Before the first production K measurement** - paste the harness
   invocation. Confirm the warm-oracle harness is correctly pointed at
   provekit-cli (oracle engagement, target manifest, no kit-config
   footgun) so the resulting number is trustworthy.
7. **After the production K measurement** - paste the per-site
   categorization. We pick the next Phase-2 PR (D-lib, C, B, or D-fn)
   from which tier the gap census shows the most closable mass in.

Skip the advisor for: running tests, reading bot reviews, mechanical CI
checks. Those are coordinator's.

## After this slice + measurement

The remaining #1749 work is provekit-lift direct mint path panic_loci
threading. It is preventive, with no current K delta expected. After that,
the next substantive K increment comes from the Phase-2 D-lib per-type slice
named in GOAL.

The Phase-2 tier worklist remains the goal-doc reference:

- D-lib sound `serde_json::Value` totality contract + type-fact discharge
  (~12 cli sites) - what #1747 proved on the fixture; needs to land on
  production.
- C construction-tracking of `json!` literals (~7).
- B intra-fn dataflow / early-return + collection membership via
  `forward_propagator` (~5).
- D-fn cross-function postconditions.

Each tier = one PR, golden-pinned, visible delta.
