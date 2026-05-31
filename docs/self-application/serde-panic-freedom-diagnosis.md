# Serde panic-freedom (D-lib) — full diagnosis, 2026-05-31

Goal: f `serde_json::to_string(&Value).unwrap()` → PANIC-SAFE (K=1); g
`to_string(&MyStruct).unwrap()` → UNDECIDABLE; falsePass=0. Stalled 5 agents.

## Fixed today (committed on `serde-finish`, e13763201) — all verified at their layer

1. **Fixture manifest** `examples/stage3-serde-totality-fixture/.provekit/lift/rust-implications/manifest.toml`
   was missing `method = "provekit.plugin.lift_implications"` and `phase = "consumer"`.
   So `mint` ran `bind_lift`, the implication lifter (host of the RA oracle) NEVER ran.
   This was the actual 5-agent blocker. With it fixed, the oracle resolves both
   unwraps to `result_unwrap` and emits 3 bridges + 1 honest lift-gap (g's to_string).

2. **`mint_bridge` dropped the lifter's `callsite`** (`provekit-claim-envelope`). Added
   `BridgeCallsite` + carry it into the bridge header (NOT into `bridge_content_cid` —
   callsite is not bridge identity). Verifier now reads `panic_site=true`.

3. **prove path missing the D-lib injection.** `callee_post_guard_fact` (the
   is_ok-totality guard supply) lived only in `cmd_verify`; the `prove` path
   (`runner.rs::work_one`, which the scoreboard uses) never called it. Wired in.

## Phase-0 scaffolding (no-silent-failure)

- Plugin subprocess stderr now **inherits by default** (was `Stdio::null()` — the
  thing that hid all of the above for five investigations). `PROVEKIT_PLUGIN_STDERR=null` to silence.
- Always-on `warn!` when a `method:`-seam bridge reaches the verifier without
  callsite provenance (mint-drop smoke detector), `enumerate_callsites.rs`.
- All new diagnostics via `tracing`, not `eprintln`.

## The remaining layer (K still 0) — precisely located, NOT yet fixed

f's contract post is `=(result, method:unwrap(to_string(v)))`. `enumerate_callsites`
DOES create the unwrap callsite with `panic_site=true, arg_term=to_string(v)` (ctor) —
verified in logs (2 ctor-arg method:unwrap callsites: f and g). The 2 var-arg
method:unwrap obligations that reach the UNGUARDED panic path are the shim formals
(noise), not f.

f's ctor-arg callsite enters `runner.rs::work_one` and is consumed by the
**producer-consumer / implication-composition tier** (runner.rs ~1286-1340):
`consumer_pre = is_ok(arg)` (result_unwrap pre, instantiated), and
`producer_post = locate_producer_post(to_string(v))`. Tier-0c calls
`pool.can_implies(post_hash, pre_hash)`. That lookup fails (no implication memento
in the pool), the callsite falls through this tier, and never reaches the panic-guard
discharge (runner ~1493) where the D-lib `callee_post_guard_fact` injection lives.
So `callee_post_guard_fact` is never invoked for f (logs: it only ever sees the shim
ctors Some/Ok/Err, never `to_string`). Result: f stays undecidable, K=0.

### Likely sound fix (next focused PR — gate on the discrimination test)

For f, `producer_post` and `consumer_pre` are the SAME formula: `is_ok(to_string(v))`
(the to_string totality post) and `is_ok(to_string(v))` (the unwrap pre specialized
to the same receiver). So the implication `producer_post → consumer_pre` is the
reflexive `P → P`, trivially valid.

Candidate: in tier-0c (runner.rs ~1330), discharge when `post_hash == pre_hash`
(reflexive: the producer guarantees EXACTLY what the consumer requires). This is
sound (`P → P`), and it discriminates correctly: g's `to_string(&MyStruct)` has NO
totality post (generic, `is_ok || is_err` or none), so `producer_post != consumer_pre`,
no reflexive match → g stays undecidable, falsePass=0 preserved.

VERIFY FIRST (one instrumented run): log `producer_post` and `consumer_pre` for the
2 ctor-arg method:unwrap callsites. Confirm f's are byte-identical `is_ok(to_string(v))`
and g's differ/absent BEFORE implementing. If they are NOT identical, the fix is the
alternative: route the ctor-arg panic callsite to the panic-guard block (where the
D-lib injection already lives) instead of letting tier-0c swallow it.

### Hard gate for any attempt

Re-run the e2e (`/tmp/serde_e2e2.sh` on battleaxe): REQUIRE f → panicSafe, g →
undecidable, falsePass=0, silentlyDropped=0. If g flips or falsePass>0, REVERT — this
is the verifier soundness core.

## UPDATE — the discharge cascade is the real obstacle (deepest finding)

Instrumenting `work_one` further (a `panic_site` probe at runner.rs:1288, since
removed) showed it fires ZERO times even though `method:unwrap` panic obligations
are emitted and "routing to guard-discharge" logs for `target_has_pre=true`. So the
real panic callsites reach NEITHER the producer-consumer probe (1288) NOR the D-lib
injection added in commit e13763201 (~1493). `work_one` is a multi-path cascade with
several early returns (tier-0 hash, tier-0c implication-composition, the
`!target_has_pre` body-discharge branch via `extract_body_obligation`, the panic
block) and the real callsites are discharged/abandoned by one of the earlier paths.

**Consequence:** commit e13763201's runner D-lib injection is in a branch the actual
serde callsites do not traverse (harmless — refuse-floor intact, falsePass=0 — but
inert for K). Closing K>0 requires READING THE WHOLE `work_one` (runner.rs
~1131-1700) end to end, mapping which path the `panic_site=true, arg_term=ctor`
callsites actually take, and applying the D-lib guard fact (or the reflexive
`producer_post == consumer_pre` discharge) AT THAT PATH. This is a discharge-cascade
design task in the verifier soundness core, not a localized patch — do it on a fresh
head with the discrimination test as the hard gate, NOT exhausted.

The reflexive-discharge idea (sound `P -> P` when the producer's post is byte-identical
to the consumer's pre) is still the most promising mechanism; it just has to be wired
into the path the callsites actually reach.

## Phase-0 gap CLOSED: `doctor` now catches this footgun

DONE (committed): `provekit doctor` now HARD-fails (exit 2) when a kit's consumer
surface is mis-wired. walk_rpc's `initialize` capabilities self-declare
`consumer_surfaces: { "rust-implications": { method, phase } }` (semantics in the
kit, so doctor stays language-blind); doctor's Check 5 spawns each plugin, reads
that, and verifies the manifest's `method`/`phase` match. Verified on
stage3-serde-totality-fixture: broken manifest (method+phase removed) -> exit 2 with
"add both lines"; fixed -> exit 0. This catches the exact omission that cost five
agents a day, BEFORE a silent empty-set attestation.

### Original gap (now closed; kept for context)

## Phase-0 gap was: `doctor` did not catch this footgun

`provekit doctor --target <kit>` exists and runs, but its checks are: TOML parse,
plugin-command binary exists, imports count, oracle reachability. It does NOT catch
the exact footgun that cost five agents: a `lift` manifest that omits `method` and
`phase`, so a consumer surface (rust-implications) silently runs the default `lift`
producer method and the implication lifter never fires.

Catching the OMISSION case soundly AND language-blind needs `doctor` to spawn each
plugin's `provekit.plugin.describe` RPC and cross-check the manifest's effective
`(method, phase)` against the capabilities the plugin advertises for that surface —
e.g. "plugin advertises a consumer method for surface S but the manifest runs the
default producer `lift`" -> WARN/FAIL. A name/heuristic check (e.g. "*-implications")
would violate language-blindness or over-warn on legitimate producers.

CONFIRMED SHAPE: `describe` (walk_rpc `initialize_result`, ~line 2109) today returns
only `capabilities.authoring_surfaces = ["rust","rust-bind","rust-walk-contracts"]`.
It does NOT advertise the RPC methods the plugin dispatches (`lift`,
`provekit.plugin.lift_implications`, `provekit.plugin.recognize`, ...) nor which
surfaces are consumers. So the work is two-part: (1) extend `describe.capabilities`
to add e.g. `rpc_methods: [...]` and `consumer_surfaces: ["rust-implications", ...]`
(plugin self-reports; semantics stay in the kit); (2) `doctor` cross-checks each
manifest's effective `(method, phase)` against that. This is a scoped Phase-0
(scaffolding) task, independent of the K cascade work, with NO falsePass risk (it is
a CLI diagnostic).

## SYSTEMIC FINDING: e2e panic discharge is broken for syntactic guards too

The remaining cascade bug is NOT serde/D-lib specific. The canonical
`examples/panic-freedom-fixture` (`guarded_unwrap`: `if opt.is_some() { opt.unwrap() }`,
`unguarded_unwrap`: `opt.unwrap()`) is correctly wired (manifest has
method=lift_implications + phase=consumer), yet its end-to-end `prove` scoreboard is:

    {"panicSafe": 0, "reflexive": 28, "undecidable": 133, "falsePass": 0}

`panicSafe=0` -- the SYNTACTIC-guard case does not discharge panic-safe e2e either.
So the GOAL's "Mechanism proven on a fixture (guarded->panic-safe; deterministic test
green)" refers to the UNIT test (a hand-built `CallSite{panic_site:true, guard_facts}`
in cmd_verify/body_discharge), NOT the mint->prove pipeline. The whole e2e panic
discharge path (the `work_one` cascade) drops panic callsites before the guard
discharge for both the syntactic-guard and the D-lib paths. Fixing the cascade
(see above) should lift BOTH at once -- the fix is not serde-specific.

This is exactly the "green unit test, broken product" gap the three product surfaces
exist to expose. Update the on-main GOAL "NOW" line: e2e K=0 is systemic, unit-test-only.

## Golden snapshot: design tension (why it is not a clean now-win)

A golden of the panic-freedom scoreboard is GOAL Phase-0, but the clean target is the
per-site census (`self-check --json`), which is daemon-heavy (warm RA oracle) and not
CI-portable. The daemon-free alternative (raw `prove` on the fixture) is brittle: the
aggregate dischargeSplit mixes in shim-std's own callsites (undecidable=133 is mostly
shim noise) and `prove` rows carry file=None, so the fixture's two functions can't be
isolated for a stable pin. The robust golden therefore wants either (a) self-check
gaining a fixture-scoped per-site mode, or (b) prove rows carrying file/line so the
golden pins just the fixture's sites. Best built WITH the cascade fix so it pins a
meaningful panicSafe>0 delta. Pinning panicSafe=0 now is legitimate scaffolding but
low-value until the cascade lands.

## COMPLETE MECHANISM (work_one routing, runner.rs ~1412) — two distinct breaks

`work_one` routes on whether the panic arg has a producer:

    if (producer_post, consumer_pre) both Some  -> IMPLICATION branch (~1412):
        build_implication_obligation(producer_post -> consumer_pre), solve
    else                                        -> GUARD branch (~1479):
        instantiate::run(resolved, arg_term) THEN cs.guard_facts discharge
        (this is where the D-lib callee_post_guard_fact injection from e13763201 lives)

BREAK 1 -- serde / any panic site whose receiver is itself a call:
  arg = `to_string(v)` HAS a producer (the to_string totality bridge -> post
  `is_ok(result)`), so `producer_post = Some(is_ok(to_string(v)))` and the site
  takes the IMPLICATION branch. BUT `consumer_pre` there is the UN-specialized
  pre `is_ok(formal)` (instantiate only runs in the else branch). z3 sees
  `is_ok(to_string(v)) -> is_ok(formal)` over disjoint terms -> UNSAT. The D-lib
  injection never runs for f (f never reaches the else branch).

BREAK 2 -- syntactic guard (panic-freedom-fixture `if opt.is_some(){opt.unwrap()}`):
  arg = `opt` is a VAR (no producer) -> producer_post None -> takes the GUARD
  branch. But `cs.guard_facts` is EMPTY: the kit's `cf_guarded(is_some(opt), ...)`
  wrapper that enumerate_callsites threads into guard_facts (walk_term ~269) is
  not populating it e2e. Unguarded -> is_some(formal) -> undecidable.

SOUNDNESS-CORE RISK (why this is not a quick patch): the IMPLICATION branch
(~1412) is SHARED with the non-panic eq-discharge tiers. Specializing
`consumer_pre` to `arg_term` there perturbs obligation CIDs / hash-tier lookups
for ALL callsites (the code already warns about this re `instantiate::run`). So
BREAK 1's fix must specialize ONLY the panic-site implication (or route a
producer-post panic_site through the guard branch with the D-lib is_ok fact
instead of the implication branch) without touching the shared non-panic path.

FIX SKETCH (one focused PR, gate on discrimination test f->safe / g->undecidable /
falsePass=0 on BOTH stage3-serde AND panic-freedom fixtures):
  1. BREAK 1: for a `panic_site` callsite that has a producer_post, either
     (a) instantiate consumer_pre to arg_term before building the implication
     (panic-only, so no shared-path CID drift), or (b) skip the implication
     branch for panic sites and use the guard branch + D-lib fact.
  2. BREAK 2: fix cf_guarded -> guard_facts threading so the syntactic-guard
     panic site is GUARDED e2e (verify enumerate_callsites walk_term ~269
     actually receives the cf_guarded wrapper from the lifter output).
  Both lift e2e K together; both are sound-by-construction (specialization +
  syntactic guard are exact), gated by the discrimination test.

## SEQUENCING: #1717 (Phase-1 soundness) must precede the K cascade fix

BREAK 1's fix specializes the panic `consumer_pre` to the call arg, which puts a
`forall <arg-sort>. <pre>` into the SMT-emitted implication. When the arg sort is
OPAQUE (a non-primitive Rust sort), that is exactly the `forall x:<opaque>. ...`
shape #1717 flags: the SMT emitter collapses it to literal `true`, so the negated
obligation is `(not true)` = unsat = a FALSE "cannot panic" (latent false-pass).
The else/guard branch already defends against this with
`instantiate::strip_outer_forall`; the implication branch does NOT. So the K
cascade fix MUST either close #1717 first (encode the opaque `forall` soundly or
refuse, never collapse -- detector: `forall x:<opaque>. false` must be
undecidable) or carry the same strip-outer-forall guard. Per the GOAL's Phase-0 ->
Phase-1 -> Phase-2 order: close #1717 BEFORE turning K from 0 to N, or risk
manufacturing the precise false-pass this whole effort exists to prevent.

## Reproduction

Warm-oracle e2e on battleaxe: `/tmp/serde_e2e2.sh` (mints shim-std + shim-serde deps,
warm-daemon mint of the fixture, prove). Debug: `RUST_LOG=warn,provekit_verifier::runner=debug,provekit_verifier::body_discharge=debug,provekit_verifier::enumerate_callsites=debug`.
