# Next unit: `provekit prove` discharges a custom-witness contract via kit recompute

State at bank (2026-06-04, branch codex/python-proven-case-1-nullguard):
- Producer + recompute-discharge PROVEN at the kit level
  (`provekit-lift-py-pytest-witness`, commits 890dfd7e6 + fda48f1a5): emit a
  content-addressed witness `.proof` (EvidenceTerm{proofType:"custom"} shape),
  `discharge_from_proof` settles BY RECOMPUTE, anti-forgery tested.
- The Rust verifier IGNORES `ContractDecl.evidence`; `proof_type` is never
  branched on; discharge is solver-only. The `custom` arm is DARK.

The unit lights that arm by DELEGATION (verifier stays language-blind; the kit
owns recompute because it owns the runtime — same principle as kit-owns-.proof-
resolution). Do NOT make the Rust verifier run Python.

## Decide FIRST (it sets the verdict enum, don't discover it mid-build)
Verdict tier: is "witnessed" a first-class `ObligationVerdict` variant
(proven / witnessed / refused) or `Discharged` with a witnessed reason string?
The report must NEVER conflate witnessed-by-execution with proven-by-solver.
Recommend a distinct, labeled tier (admissibility honesty). T's call.

## Steps, in dependency order
0. **Producer mints a contract-with-evidence.** Today the witness is a standalone
   envelope. Make the pytest-witness lifter emit, at mint time, a
   `ContractDecl(name=test_id, inv=<claim>, evidence=EvidenceTerm{custom,witness})`
   so the project `.proof` carries a contract whose discharge evidence IS the
   witness. Wire it as a lift surface (lsp + manifest), mirroring the seats.
1. **RPC method** `provekit.plugin.discharge_witness` (mirror lift/lift_implications):
   req = {witness fields (codeCid,runtimeCid,test,outcome), workspace_root,
   source_paths}; resp = {verdict, reason, recomputedWitnessCid}. Add to
   kit_declaration rpc.methods.
2. **Kit handler (Python lsp):** implement `discharge_witness` → call the existing
   `verify`/`discharge_from_proof` recompute → return verdict. Mostly wiring.
3. **Verifier consumer (Rust):** in the contract→obligation path, if
   `decl.evidence` is `Some(custom)`, route NOT to the symbolic solver but to a
   witness-discharge path that RPCs the owning kit's `discharge_witness` and maps
   the response to the verdict tier from the "decide first" call. This is the
   literal reading of `ContractDecl.evidence` + branch on `proof_type`.
4. **Pin/runtime binding:** the kit must re-run against code that hashes to the
   witness's codeCid. First version: re-run against the project being proven and
   verify the codeCid binding (`verify` already does this). Carry/resolve the
   pinned code in the RPC.
5. **E2E acceptance through REAL `provekit prove`:** a project whose `.proof`
   carries a custom-witness contract → `provekit prove` discharges via kit
   recompute; mutate the impl → `provekit prove` REFUSES. Unit tests are not
   enough; the gate is the real command.

## Files to touch
- protocol/specs (RPC method shape), provekit-verifier/src (contract→obligation
  dispatch + verdict tier), provekit-lift-py-pytest-witness (mint surface + lsp
  discharge_witness handler), an examples/ witness project for the e2e gate.

## PRECISE Rust grounding (done 2026-06-04 — next session is pure execution)
- Kit half is DONE + committed (fd6c3528a): producer, recompute, content-addressed
  witness `.proof`, anti-forgery, AND the discharge CLI the verifier will spawn:
  `provekit-pytest-witness-discharge <witness.proof> <project> <code...>` ->
  stdout JSON {verdict,reason}, exit 0 iff DISCHARGED, fail-closed.
- Verifier delegates to provers by SPAWNING binaries (Command::new), NOT RPC
  (z3/coq/maude all do this). So the witness arm spawns the discharge CLI.
- Verdict enum: `provekit-verifier/src/types.rs:867` ObligationVerdict
  {Discharged,Unsatisfied,Undecidable,Disagreement} + as_str at :875. Add
  `Witnessed` -> compiler forces every match site (runner.rs counters ~322-424 &
  595-697, report rendering). Exhaustiveness is compiler-checked = safe.
- Insertion point: `consistency.rs::verify_consistency` per-candidate `.map()`,
  BEFORE `emit_asserted(inv)`. `body.get("evidence")` is present on the contract
  memento (serialize.rs:177 writes it). If
  `evidence.proofType == "custom"` -> spawn discharge CLI, map verdict (DISCHARGED
  -> Witnessed; else Unsatisfied), skip the SAT path.
- TWO real wiring decisions (the reason this is atomic, not a patch):
  1. project_dir: the consistency pass only gets the MementoPool, not the source
     tree. Thread the `provekit prove <project>` path down to verify_consistency
     so the discharge CLI can recompute against the pinned source.
  2. tool->command config: resolve which discharge binary to spawn (certificate
     carries tool="pytest"); add a registry entry like the solver registry.
- STEP 0 prerequisite: a kit mint-surface that emits a `ContractDecl(inv=claim,
  evidence=EvidenceTerm{custom,witness})` into the project `.proof`, so the
  verifier ENCOUNTERS a contract-with-evidence. Today the witness is a standalone
  envelope. For the e2e you can hand-author this `.proof` first to unblock the
  Rust work, then build the real surface.
- Soundness backstop: the e2e discrimination gate (mutate impl -> `provekit prove`
  must REFUSE) empirically catches any branch error regardless of session state.

See [[project_provekit_witnessed_vs_consistency]], [[project_provekit_kit_owns_proof_resolution]], [[project_provekit_pin_all_three]].
