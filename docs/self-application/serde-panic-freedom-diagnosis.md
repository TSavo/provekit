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

## Phase-0 gap confirmed: `doctor` does not catch this footgun

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

## Reproduction

Warm-oracle e2e on battleaxe: `/tmp/serde_e2e2.sh` (mints shim-std + shim-serde deps,
warm-daemon mint of the fixture, prove). Debug: `RUST_LOG=warn,provekit_verifier::runner=debug,provekit_verifier::body_discharge=debug,provekit_verifier::enumerate_callsites=debug`.
