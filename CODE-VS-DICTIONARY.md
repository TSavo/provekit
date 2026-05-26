# Code vs the Dictionary — the 6x census

> **OPERATING RULE (T): anything out of scope needs to be deleted.** Scope = the dictionary
> (SHARED-LANGUAGE.md). In scope (maps to a term) → keep. Out of scope → delete (it's in git;
> recoverable; CI gates the removal). The only real work is *determining* scope; once known,
> out-of-scope is an automatic delete, not a review.
>
> A (compile-IR-to-machine-code) and B (lift-binaries): **OUT OF SCOPE → DELETE (T-ruled).**


> The dictionary (SHARED-LANGUAGE.md) is the membrane. Every crate/command either maps to
> a term in it (KEEP) or it's part of the "6x more" where the problem lives (FLAG).
> First pass: name- and knowledge-based. **No FLAG is a delete order** — each needs a read
> ("which dictionary term is this?") + the byte-neutral / has-a-consumer test first.
> Surface measured off the honest-spike worktree (pre-#1537, so agent/.invariant still present).

## KEEP — maps cleanly to a dictionary term

| Crate / command | Term |
|---|---|
| provekit-cli, libprovekit-rpc, provekit-plugin-loader | CLI / one-RPC-language |
| provekit-ir-symbolic, provekit-ir-types | Contract / ProofIR |
| provekit-canonicalizer, provekit-claim-envelope, provekit-proof-envelope | Mint / envelope / provenance |
| provekit-lift, -lift-rust-tests, -proptest, -quickcheck, -contracts, -creusot, -kani, -prusti, -verus, -flux, -openapi, -native-surfaces | Lift (native-form **doors** — federation seats) |
| provekit-realize-rust-core, provekit-emit-rust-cargo-test, provekit-sugar | Emitter / materialize / sugar |
| provekit-verifier, provekit-ir-compiler-smt-lib | Verify / solvers (contract → SMT → Z3) |
| provekit-lsp, provekit-lsp-rust | LSP |
| cmd_lift, cmd_mint, cmd_prove, cmd_verify, cmd_materialize, cmd_emit, cmd_proof, cmd_hash, cmd_witness, cmd_package, cmd_plugin, cmd_init, cmd_version | the verbs of the clean model |
| cmd_protocol, cmd_verify_protocol, provekit-lsp-protocol-catalog | Protocol record |

## FLAG — does not map; the 6x. Clusters into recognizable experiment-families:

**(A) Compile-ProofIR-to-a-machine-target** — **CUT (T-agreed).** A compiler project, not in the model.
- provekit-ir-compiler (+ -stub), provekit-ir-compiler-x86-64, -wasm, -jvm-bytecode, -c, provekit-ir-codegen

**(B) Lift-a-binary, not a sugar** — **CUT (T-agreed).** Lifting compiled artifacts ≠ lifting library sugar.
- provekit-lift-asm-aarch64, -lift-asm-x86-64, -lift-evm-bytecode, -lift-jvm-bytecode

**(C) ProvekIt-native authoring surface** (the `.invariant` cousin — write contracts in *our* macro instead of lifting native forms):
- provekit-macros, provekit-macros-rt

**(D) Speculative verbs** — per-verb ruling (T):
- cmd_catalog — **CUT** (the fiction verb; "catalog is bad")
- cmd_ask — **CUT** (infection)
- cmd_search — **CUT** (infection)
- cmd_compose — **CUT** (infection)
- cmd_dump — **KEEP** (debugging, not infection)
- cmd_exam — **OUT → DELETE** (T's own steelman collapsed it: "exam is the .proof a JDK
  publishes of its own contracts" = already the proof envelope; exam-as-built = coverage
  questions over `concept_hub_version` = phantom over the fiction catalog. No third thing.
  The coverage/gap value survives elsewhere: vendor-side = the .proof; consumer-side gaps
  = verify output. Also drop: ExamManifestMemento, dispatch_exam_manifest, PEP exam-manifest surface.)
- cmd_link, cmd_implicate — unruled, pending

**(E) Demos / amplifiers** (demonstration, not product):
- provekit-showcase, provekit-mint-amp

## TS CLI — OUT → DELETE (T); TS demoted to "just a kit"

The CLI is **rust**, one brain (dictionary). TS was where we *started* (the prototype CLI);
the rust CLI is the double that became real. Cut the prototype.
- **CUT:** `implementations/typescript/src/cli.ts`, `cli.mint.ts`, `cli.*.test.ts` — TS orchestration verbs.
- **KEEP (TS kit):** `provekit-realize-typescript-{core,better-sqlite3,pg}`, `lift/typescript-source`,
  TS canonicalizer + self-contracts + RPC plugin shim. TS stays a peer kit (lift/emit/materialize, speaks the one RPC).
- Surgical (gut orchestration, keep kit) — dependency check before file-level cut, like the rust `.invariant` gut.
- STALE FRAMING to fix: Makefile header still says "TypeScript is the center surface." Demoted → just a kit.

## IN-FLIGHT — already being removed (not new finds)
- provekit-agent, -agent-claude-code, -agent-openai + cmd_agent / cmd_must / cmd_fix → removed on main (#1537).
- provekit-self-contracts `.invariant` orchestration → removal on `chore/rust-self-contracts-via-lift`.

## T-RULED
- provekit-ir-compiler-coq / -lean / -maude: **IN SCOPE → KEEP** (solver/proof-assistant discharge backends).
- migrate/transport — cmd_transport, cmd_bind, cmd_bind_migrate: **OUT OF SCOPE → DELETE** (migrate = composition of lift+materialize, not a core domain).
- C provekit-macros / -macros-rt: **OUT → DELETE** (the `.invariant` cousin).

## Read-and-placed (Claude, grounded in crate headers)
- **provekit-linker** → KEEP. Linker algebra: derives boundary↔contract bindings from
  (contracts ∪ call-edges). The "contract travels to the boundary" mechanism; feeds verify + LSP. (`cmd_link` = CLI.)
- **provekit-linkerd** → KEEP. Daemon serving the linker to per-kit LSP plugins. LSP infrastructure.
- **provekit-build + provekit-verify-build-rs** → KEEP (capability): compile-time contract
  enforcement via cargo build.rs (the dictionary's "red squigglies at compile time"). NOTE:
  two crates overlap → consolidate, verify which is live before cutting the other.
- **provekit-showcase** → OUT/DELETE. Launch demo + discharge benchmark. Demonstration, not product.

## Deep-read RESOLVED
- **provekit-walk** → **KEEP lib** (consumed by provekit-lift-rust-tests — in the rust lift
  path, NOT a standalone demo; Claude's earlier "experiment" lean was wrong). **CUT `walk_demo` bin.**
  Verify the rust-tests→walk dep is live-used (not stale Cargo entry).
- **cmd_implicate** → **CUT (dead stub).** Self-described "honest stub for v0; resolver + SMT
  emission TBD, not yet shipped." Implications stay in-scope (trinity); the non-functional verb goes; re-add when built.

## STILL NEED T (genuine product-intent calls)
- **provekit-baseline-std / -baseline-rust-std** → **OUT → DELETE (T-confirmed).** Hand-authored
  std contracts (`must()`/`contract()`, same API as the killed `.invariant` DSL) in a central
  advisory catalog = the disease one tier out. We don't author contracts for code we don't own;
  std's contracts come from std's own forms (lifted) or the language team's .proof, or we refuse.
- **provekit-mint-amp** → NOT dead: `cmd_mint` uses it as `algebraic_mint` (the
  `run_algebraic_mint` path). It's the **algebraic-memento layer** — mints sorts, equations,
  algorithms, bindings, effect signatures, language signatures/morphisms ("AMP" ≈ Algebraic
  Memento Protocol; paper 18 homomorphic-algebra direction). **NOT in the dictionary**, and
  the architect doesn't recognize it → strong lean **OUT** (accreted experiment). Cutting it
  pulls the `algebraic_mint` path from cmd_mint. NUANCE: `mint_sort`/SortMemento may be
  IR-core (FOL has sorts) → fold into IR rather than delete; the equation/algorithm/morphism
  algebra is the experiment. T call: is the algebraic layer in scope?
  → **OUT → DELETE (T + grounded).** Only consumer is cmd_mint's `algebraic_mint` subcommand;
  the core loop (ir-symbolic, claim-envelope, lifters, realizers, verifier) has ZERO dependency
  on it. Contracts carry sorts inline; emission/verify never consume equations/algorithms/
  morphisms/language-sigs. AMP's downstream was the algebraic cross-language layer = the
  migrate/homomorphic-algebra direction, already cut. Pull `algebraic_mint` from cmd_mint;
  IR sorts (in ir-types/-symbolic, kept) are untouched.
- provekit-linker, provekit-linkerd: RPC/multi-kit orchestration (KEEP-ish), or experiment?
- provekit-walk: lift-adjacent (fold into lift?), or its own thing?
- provekit-baseline-std, provekit-baseline-rust-std: stdlib contract baselines — kit content (KEEP), or 6x?
- provekit-build, provekit-verify-build-rs: build integration — infra (KEEP), or 6x?

## Tally (first pass)
- KEEP: ~22 crates+commands of the clean model.
- FLAG (A–E): ~20 crates + ~8 commands — the 6x.
- IN-FLIGHT: 3 crates + 3 commands.
- UNSURE: ~10, need T's call.

Next per FLAG/UNSURE item: read it → name the term it serves (or confirm none) → byte-neutral /
has-a-consumer test → KEEP / fold / cut.
