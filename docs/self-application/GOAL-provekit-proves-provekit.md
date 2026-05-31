# GOAL: provekit proves provekit, for real (the durable north star)

This is the living goal. Any session or agent picking up the self-application
work starts here. It defines what "a fully working product" means, the single
metric that tracks it, the dependency-ordered path, and the discipline every
change must obey. Update it as state moves; do not let it go stale.

## The promise

provekit is a verifier PRODUCT: point it at real source code and get a sound,
honest verdict about correctness properties, starting with panic-freedom (which
`unwrap`/index/`expect` sites cannot panic, which are unproven, and WHY). The
proof that it is real and not a demo is that it proves ITSELF: run on provekit's
own production crates (`provekit-cli`, `libprovekit`) it produces a substantive,
honest scoreboard with ZERO silent drops and ZERO false "cannot panic".

"Fully working" =
1. **Sound and honest.** Every call site is enumerated, categorized, and either
   discharged with a real proof, refused as honestly undecidable (with the
   reason), or named as residue. Always-on invariants: silently-dropped = 0,
   false-pass = 0, no partial function labeled "total" to inflate a number.
2. **Substantive.** On provekit's own code, K (provably panic-safe sites) covers
   the provable categories via SOUND reasoning, not a fixture and not a vacuous
   label. The reasoning tiers that close the real buckets are shipped.
3. **Observable and self-checking.** The verdict is one command
   (`provekit self-check`), pinned by a golden in CI, with a `doctor` that
   validates kit wiring up front. Progress is a number you watch move;
   regressions scream with a readable diff.
4. **Self-describing.** The wiring is executable knowledge (`doctor` + the
   runbook), not tribal knowledge or stale prose.

## Where we are (2026-05-31, honest)

- Honesty layer on main (#1716): 291/311 body-discharge-eligible (was 1/310),
  0 silently dropped (was 549), reflexive discharges LABELED (never sold as
  proof). The old "211 real-solver" was the pre-gate spine rubber-stamping
  tautologies; that fool's gold is gone.
- Sound panic-freedom on main (#1718): a guarded `x.unwrap()` discharges
  panic-safe, an unguarded one stays undecidable (deterministic test
  `cmd_verify::tests::guarded_panic_safe_unguarded_undecidable`). The verifier is
  LANGUAGE-BLIND; the rule is generic: "target carries a non-trivial pre ->
  discharge it under the call-site guard facts."
- On provekit-cli itself (2026-05-31, full oracle-engaged `self-check` run, warm
  `provekit-linkerd` daemon): **K (panicSafe) = 0**, **silentlyDropped = 0**,
  **falsePass = 0** (both hard invariants HELD). K=0 is HONEST: provekit-cli has
  ZERO syntactically-guarded panic sites, so the oracle disambiguates each leaf
  but cannot manufacture a guard. The oracle MATERIALLY improved the census vs
  the syntactic-only run (catalog `be345b4d` -> `ec72ab4d`):
  panic-site-unproven 26 -> 12, bridges emitted 1276 -> 2108, no-contract-for-callee
  4043 -> 3225, undecidable 1948 -> 2160. The 12 residue are all `.expect()`,
  9 of them `kit_dispatch.rs` Mutex `lock().expect()` (renders `LockResult`, a
  `Result` alias) = genuine POISONING RESIDUE ("lock is total" would be unsound).
- PRODUCT-PATH WIRING (operational, not yet a one-command surface): the mint
  lifter resolves via a RESIDENT `provekit-linkerd` daemon, not a cold per-mint
  RA. A cold mint finishes before the daemon's ~52s index, so queries refuse and
  the census is syntactic-only. The oracle-engaged census requires the daemon
  PRE-WARMED (`PROVEKIT_LINKERD_BIN` set, one warm-up mint, then `self-check`
  within the 5-min idle window). Folding this warm-up into `self-check`/`doctor`
  (so `--oracle` guarantees a warm daemon or loudly says it could not) is the
  remaining Phase-0 wiring.
- Known debt: #1717 (SMT emitter collapses an opaque-sorted `forall` to literal
  `true` on the negated path = latent false-pass; the panic path routes around
  it). name-in-CID preimage (contradicts names-are-sugar).
- RESOLVED (2026-05-31): the "monorepo rust-analyzer never quiesces" stall was
  NOT a distinct infra bug needing a warm-index daemon. It was a SILENT failure:
  rust-analyzer shells out `cargo metadata`, and when the ambient cargo is a
  different vintage than the RA binary that call fails on a flag mismatch (e.g.
  `--lockfile-path`); RA then resolves NOTHING yet reports `quiescent=true,
  health=warning` -- a K=0 census that looks honest but is a broken oracle.
  `ra_oracle` now pins RA's `CARGO`/`RUSTUP_TOOLCHAIN` to the toolchain the RA
  binary lives in. With the pin, RA indexes the FULL provekit-cli workspace and
  quiesces `health=ok` in ~52s, resolving real cli method receivers via hover
  (`serde_json::to_string_pretty(&report).unwrap()` -> `core::result::Result` ->
  stem `result`). Validated by `provekit-walk`'s `hover_probe` bin against
  `examples/oracle-hover-fixture` AND against provekit-cli itself. The cli-K
  FOUNDATION (resolution + leaf disambiguation) is unblocked; the remaining lever
  to raise K is the Phase 2 reasoning tiers, not infra.

## The metric (the one number we watch)

`provekit self-check` over provekit's own crates emits, deterministically:
- enumerated callsites, **silently-dropped = 0** (hard invariant)
- discharge split: panic-safe **K**, reflexive (labeled), vacuous, undecidable
  **M**, **false-pass = 0** (hard invariant)
- panic census: each unproven site -> { category, tier-that-would-close-it }

Progress = K rising as tiers ship, M shrinking toward the honest residue, the
invariants never violated. "Fully working" is K covering the provable buckets
with the residue NAMED, not K == N.

## The cli census (the grounded worklist, 37 production sites)

| Bucket | Count | Why safe | Tier to close |
|---|---|---|---|
| `serde_json::to_string(&Value)` | ~12 | `Value` is genuinely total-serializable (keys always String, Number finite) | D-lib: sound `Value`-totality contract + type-fact discharge. NOT a vacuous "total" label. |
| Mutex `lock().expect()` | ~8 | errs only on poisoning (another thread panicked holding it) | RESIDUE. "lock is total" would be UNSOUND. Name it residue or do a global no-panic-while-held analysis. |
| `json!` field `payload["k"].as_str().unwrap()` | ~7 | literal built with key `k` as string lines above | C: construction tracking |
| `map.get(key).expect("present")` from own keyset | ~4 | key drawn from the map's keys | B/E: collection membership |
| misc (`len==1 -> next()`, fn invariant, blake3 tagged post) | ~3 | length->nonempty / dataflow / cross-fn post | B / B / D-fn |

Honest correction to "catalog work closes 62%": only the ~12 `&Value` sites
close cleanly by catalog, and ONLY because `Value` totality is a TRUE fact about
the type (a sound contract, not a vacuous label). The Mutex bucket is residue by
construction. The rest is real reasoning work.

## The path (dependency-ordered)

### Phase 0 - SCAFFOLDING FIRST (this is why 2026-05-30 hurt). No feature work until these exist.
- `provekit self-check --json`: the scoreboard + gap census as one deterministic
  command. (Replaces hand-grepping runs and hand-categorizing sites.)
- Golden snapshot of `self-check` in CI: a regression is a readable diff with the
  reason, not a bare red gate. (The conformance break we spelunked for an hour
  would have been one golden line.)
- No-silent-failure, system-wide: every drop/refuse/fallback emits a loud,
  counted, structured signal; build-time silent degradations (the manifest
  empty-set attestation, an undeclared SMT symbol) become HARD ERRORS.
- `provekit doctor`: validate a kit's config/manifest up front (catch the
  manifest path footgun before it silently produces an empty-set attestation).

### Phase 1 - Close the soundness holes (a verifier with a latent false-pass is not a product)
- #1717: encode opaque-sorted quantifiers soundly, or REFUSE them; never collapse
  to `true`. Detector: `forall x:<opaque>. false` must be undecidable.
- (deep, optional) strip `name` from the contract CID preimage; resolve by
  (concept/library, CID). Large blast radius; gate behind a golden.

### Phase 2 - Make K substantive (the reasoning tiers; each one PR, golden-pinned, visible delta)
- Tier D-lib: sound `serde_json::Value` totality contract + type-fact discharge. ~12 sites.
- Tier C: construction tracking for `json!` literals. ~7 sites.
- Tier B: intra-fn dataflow / early-return narrowing + collection membership
  (wire the existing forward_propagator facts into guard establishment). ~5 sites.
- Tier D-fn: cross-function postconditions as assumable facts. misc.

### Phase 3 - Honest residue + product packaging
- Name the irreducible residue (Mutex poisoning, external invariants) AS residue
  in the scoreboard.
- The panic-audit becomes a first-class deliverable: point provekit at ANY Rust
  crate -> an honest K/M census. That IS the product.

## The discipline (every change obeys these; the lessons of 2026-05-30)
1. Golden-pinned: if it changes a discharge number, it updates the golden with a one-line why.
2. Observable: it emits a loud, structured, counted signal; no silent drop/refuse/fallback.
3. Language-blind: no language-specific names in the CLI or verifier; semantics live in the kit.
4. Refuse-floor: never a vacuous or false "cannot panic"; unproven stays honestly undecidable.
5. CID-not-name: identity is content CID; name is opaque sugar / resolution index.
6. Self-describing: new wiring is validated by `doctor` and documented in the runbook.
7. Scaffolding before features: the observability surface exists before the thing it observes.

## Non-goals / honest bounds
- K != N. Genuine residue stays unproven, by design and honestly.
- "Make the number go up by labeling a partial function total" is the vacuous
  trap; only sound contracts (a real pre that discharges, or a true type-level
  totality) count.

## Pointers
- Runbook: `docs/self-application/KIT-SETUP-AND-SELF-APPLICATION.md`. One command: `scripts/self-apply.sh`.
- Issues: #1717 (opaque-`forall` false-pass).
- Key files: `provekit-verifier/src/{runner.rs, enumerate_callsites.rs, body_discharge.rs}`,
  `provekit-walk/src/{lift.rs, bin/walk_rpc.rs}`, `provekit-ir-compiler-smt-lib/src/generated.rs`,
  the rust-std shim `examples/provekit-shim-rust-std`.
- Dispatch: substrate code goes to Codex (`gpt-5.5`, `model_reasoning_effort=xhigh`), isolated
  worktree, FULLY INLINE briefs (no file refs - they do not survive MCP serialization). The
  coordinator owns all gh/PR ops and review.
