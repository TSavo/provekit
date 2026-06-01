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
honest scoreboard with ZERO silent drops and ZERO false "cannot panic". Once
that holds in Rust, the same substrate proves it in Python - that is the
architectural thesis at v2.

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
5. **Cross-language at v2.** The same substrate proves Python; the second
   language is cheaper than the first.

## Where we are (2026-06-01)

### Phase 0 - SCAFFOLDING - DONE

- `self-check --json` on production crates: shipped.
- Golden snapshot in CI: shipped (`provekit-cli/tests/self_check_golden.rs`
  under conformance gate; readable diff on regression).
- `provekit doctor` HARD-fails (exit 2) on the manifest method/phase footgun
  (#1742); language-blind via plugin-self-declared `consumer_surfaces`.
- No-silent-failure floor PRs (recent arc):
  - **#1747** panic-locus preservation + guard-branch routing (the mechanism
    for sound panic-safe discharge: callsite-scoped producer-post resolution,
    content-gated `panic-safe` tag).
  - **#1750** fail-closed `panicLoci` extraction in cmd_mint (silent-drop on
    malformed input became a loud Err naming the field and offending type).
  - **#1752** (closes #1748) multi-line receiver/unwrap line preservation
    (walk_rpc emits producer.start_line; verifier untouched).
  - **#1753** (closes #1751) convergent oracle harness with K=3 stable-pass
    gate; tracing-to-file with stderr clean.
  - **#1755** (closes #1754) mid-run `.provekit/imports/` mutation guard
    (catches concurrent bcargo/rsync wiping deps; loud Err with symmetric
    diff).
  - **#1756** (#1749 walk envelope surface) threads `panic_loci` through the
    provekit-walk single-contract envelope mint path and keys `EnvelopeCache`
    by `contract_cid` plus panic-loci fingerprint, so header provenance cannot
    silently stale or disappear.
- Plugin subprocess stderr inherits by default (the `Stdio::null` that hid
  load-bearing bugs is gone); counted `warn!` on missing callsite provenance;
  tracing throughout, not eprintln.

### Phase 1 - SOUNDNESS - DONE

- **#1717**: opaque-sorted `forall` encoded soundly (or refused). Detector
  `forall x:<opaque>. false` is undecidable, not collapsed to `true`. CLOSED.

### Phase 2 - SUBSTANTIVE K - IN FLIGHT

- **Mechanism proven on fixture** (stage3-serde-totality-fixture, warm-oracle
  e2e, regressed-and-confirmed across #1752):
  `to_string(&Value).unwrap()` discharges **panic-safe**;
  `to_string(&MyStruct).unwrap()` stays undecidable (refuse-floor intact).
  dischargeSplit `{panicSafe:1, falsePass:0, silentlyDropped:0}`.
- **Production K measurement on `provekit-cli`** (convergent harness, K=3
  stable, 2026-06-01):
  - `silentlyDropped=0, falsePass=0, droppedSites=[]` (hard floor HELD).
  - `panicSafe=0, panicCensus=32` (mechanism not yet applied to production).
  - `(attempted, resolved) = (3764, 3358)` stable across 3 passes; the 406
    unresolved receivers are honest oracle ceiling (generics, dyn dispatch,
    macros), not cold-pass artifacts.

## Current census (provekit-cli, 32 unproven sites, named by category)

| Category | Count | Closing mechanism |
|---|---|---|
| residue | 10 | Mutex `.lock().expect()` (9) + platform_semantics filesystem invariant (1). Honest residue; "lock is total" would be unsound. |
| D-lib | 9 | serde_json totality. Splits: 2 `&Value` (close via existing #1747 mechanism); 6 libprovekit-blessable derived-Serialize types (PluginRegistryMemento, RealizedSource, Sort, Dialect, Term, RealizeRequest); 1 `to_string_pretty(&PluginRegistryMemento)`. |
| C | 7 | `json!` construction tracking in cmd_protocol.rs (`payload["k"].as_str().unwrap()` pattern; literal field is built as String, must propagate). |
| B | 4 | Intra-fn `assert!(x.is_some()/is_ok())` propagation; plus `len==1 -> next()` guard. |
| D-fn | 2 | Cross-function postconditions: catalog primitive `.cid()`, `Cid::parse` on literal. |
| oracle-residue | 0 | None in panicCensus; the 406 unresolved receivers are non-panic obligations. |
| unknown | 0 | Every site has a named category. Honest. |

The honest read: ~22 closable sites (D-lib + C + B + D-fn), 10 named residue.
v1 is "K covering the closable categories, residue named, hard floor never
violated." K = N is not required and not honest.

## The metric (the one number we watch)

`provekit self-check` over provekit's own crates emits, deterministically:
- enumerated callsites, **silently-dropped = 0** (hard invariant)
- discharge split: panic-safe **K**, reflexive (labeled), vacuous, undecidable
  **M**, **false-pass = 0** (hard invariant)
- panic census: each unproven site -> { category, tier-that-would-close-it }

Progress = K rising as tiers ship, M shrinking toward the honest residue, the
invariants never violated. "Fully working" is K covering the provable buckets
with the residue NAMED, not K == N.

## The path (dependency-ordered)

### Phase 0 - SCAFFOLDING - DONE
See "Where we are."

### Phase 1 - SOUNDNESS - DONE
- #1717: CLOSED.

### Phase 2 - SUBSTANTIVE K - IN FLIGHT
Each tier ships as one PR, golden-pinned, with visible scoreboard delta.

- **D-lib per-type for libprovekit** (next slice): per-type infallibility
  contracts for the 6 derived-Serialize types in libprovekit. Cleaner target
  than provekit-cli (libprovekit owns these types). Expected K delta on
  libprovekit's self-check: +6-8 sites. Discrimination triplet mandatory
  (positive blessed / negative unregistered concrete / negative generic
  `T: Serialize`).
- **D-lib `&Value` for provekit-cli**: closes the 2 kit_dispatch.rs `&Value`
  sites via the existing #1747 mechanism.
- **C `json!` construction tracking**: closes the 7 cmd_protocol.rs sites.
  New mechanism (track that `payload["k"]` returns Value::String when the
  literal built `k` as a string); design checkpoint required.
- **B intra-fn `assert!` propagation**: closes the 4 sites where
  `assert!(x.is_some()/is_ok())` precedes the unwrap. New lifter surface,
  bounded.
- **D-fn cross-function postconditions**: closes the 2 remaining sites
  (`Cid::parse` on literal, catalog primitive `.cid()`).
- **#1749 remaining surface**: provekit-lift direct mint paths get
  `panic_loci` threading. cmd_mint and provekit-walk single-contract envelope
  are done. Preventive, no current K delta, but locks the
  no-silent-degradation boundary.

### Phase 3 - RESIDUE NAMED + V1 RELEASE
- The 10 residue sites get an explicit `residue` category in the panicCensus
  output (not "unproven"; honest residue with reason).
- `provekit doctor` aggregates the no-silent-failure surfaces (#1742 manifest
  validation, #1750 fail-closed extraction, #1753 convergence, #1755 mutation
  guard) into a startup health check.
- v1 release tag: "Rust v1 done." `provekit self-check` on any Rust crate
  produces an honest panic-freedom audit with a named gap census.

### Phase 4 - SUBSTRATE PROMOTION + PYTHON PARITY
Validation of the architectural thesis: the second language is cheaper than
the first.

**Substrate promotion (between v1 and Python pivot):**
- `concept:panic-freedom` hub: promote panic-freedom from "in the Rust kit"
  to "expressed as a concept hub with `is_ok(result)` as a language-agnostic
  totality predicate." The M+N transport for Python.
- Doctor cross-kit envelope: doctor validates the substrate protocol any kit
  must implement, not just Rust kit specifics.
- Contract shape audit: read-through of every contract type's fields for
  Rust-shaped vocabulary leaks; promote leaks to concept-level or push back
  into the kit.

**Python parity (the v2 product win):**
- Python kit lift: Python source -> ProofIR, with panic-equivalent semantics
  (KeyError, IndexError, AttributeError, AssertionError from `assert`, None
  dereference).
- Python self-contracts: type totality blessings for `pydantic.BaseModel`,
  `@dataclass`, `typing.Final`, the immutable/total Python types.
- Python self-check on a real Python target: honest scoreboard with named
  gap census. **This is v2 - "point provekit at your Python code, get a
  trustworthy correctness verdict."**
- v2 release tag: two languages, one substrate, honest scoreboards on both.

**Post-v2 (architectural validation, not gating):**
- Cross-language cycle proof (Rust <-> ProofIR <-> Python, byte-identical
  CIDs after formatter normalization).
- Multi-language scoreboard composition.
- Rust libraries shipping proofs that Python consumers reuse.

## The discipline (every change obeys these; the lessons of 2026-05-30)
1. **Golden-pinned**: if it changes a discharge number, it updates the golden
   with a one-line why.
2. **Observable**: it emits a loud, structured, counted signal; no silent
   drop/refuse/fallback. Concurrent harness mutation is loud (#1755);
   malformed input is loud (#1750); cold oracle either loops to convergence
   or fails loud (#1753).
3. **Language-blind verifier**: no language-specific names in the CLI or
   verifier; semantics live in the kit; refuse-floor preserved per language.
4. **Refuse-floor**: never a vacuous or false "cannot panic"; unproven stays
   honestly undecidable.
5. **CID-not-name**: identity is content CID; name is opaque sugar /
   resolution index.
6. **Self-describing**: new wiring is validated by `doctor` and documented in
   the runbook.
7. **Scaffolding before features**: the observability surface exists before
   the thing it observes.

## Non-goals / honest bounds
- K != N. Genuine residue stays unproven, by design and honestly.
- "Make the number go up by labeling a partial function total" is the vacuous
  trap; only sound contracts (a real pre that discharges, or a true
  type-level totality) count.
- Cross-language cycle proof is not gating v2. The v2 product win is "Python
  users can find correctness bugs"; cycle proof is architectural validation
  that lands post-v2.

## Pointers

- **Plan file**: `docs/self-application/PLAN-panic-loci-completion.md`.
  Current slice queue, advisor checkpoints, verification commands.
- **Runbook**: `docs/self-application/KIT-SETUP-AND-SELF-APPLICATION.md`.
  One command: `scripts/self-apply.sh`.
- **Diagnosis**: `docs/self-application/serde-panic-freedom-diagnosis.md`.
  Full arc of the #1747 root-cause hunt.
- **Recent PRs (this arc)**:
  - #1747 panic-locus preservation + guard-branch routing.
  - #1750 fail-closed panicLoci extraction.
  - #1752 (#1748) multi-line emitter fix.
  - #1753 (#1751) convergent oracle harness + tracing.
  - #1755 (#1754) mid-run imports mutation guard.
  - #1756 (#1749 walk envelope) panic_loci threading + EnvelopeCache
    fingerprint key.
- **Open follow-up**: #1749 provekit-lift direct mint path panic_loci
  threading.
- **Key files**: `provekit-verifier/src/{runner.rs, enumerate_callsites.rs,
  body_discharge.rs, handshake.rs, load_all_proofs.rs}`,
  `provekit-walk/src/{lift.rs, bin/walk_rpc.rs, envelope.rs}`,
  `provekit-cli/src/{cmd_self_check.rs, cmd_mint.rs}`,
  `provekit-ir-compiler-smt-lib/src/generated.rs`,
  rust-std shim `examples/provekit-shim-rust-std`.
- **Dispatch**: Codex (`gpt-5.5`, `model_reasoning_effort=xhigh`), isolated
  worktree, FULLY INLINE briefs (no file refs - they do not survive MCP
  serialization). Coordinator owns all gh/PR ops + review. Standing arc:
  see PLAN-panic-loci-completion.md.
