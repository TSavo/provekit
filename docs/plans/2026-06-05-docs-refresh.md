# Docs refresh plan (2026-06-05)

Scope: the README was rewritten to fold in the oracle trio (source oracle,
witness oracle), the no-shim numpy vendor demo, and the inheritance capstone
(cross-proof contract conjoin). This plan is the honest inventory of which other
docs are now stale against that work, plus a prioritized refresh order.

Status (2026-06-05): P0 and P1 below are EXECUTED, not just planned.
per-language-status.md was corrected to v1.6.6 / CID `809ed1eb...`; the oracle
trio was folded into proofchain.md, architecture.md, and product.md; product.md
was reframed to cite the inheritance demo. P2 remains a plan. Two prose-sourced
errors caught during execution and corrected against the implementation: the
real catalog version is v1.6.6 (the binary, the signed asset, and `protocol.rs`
all agree; the "no source" note in P0-1 below was wrong), and `provekit lift` is
a real command (`cmd_lift.rs`, 179 lines, dispatches the lift-plugin protocol and
writes ProofIR term JSON), NOT the stub its `--help` string claims. Method
lesson: confirm command behavior and version identifiers against the `cmd_*.rs`
body and the `include_bytes!` asset, never against a help string or doc-comment.

All claims below were grounded against running code while rewriting the README:

- `discharged: 2` confirmed live via `examples/numpy-showcase/run.sh` (one z3
  consistency obligation, one witness-recompute obligation).
- The inheritance refusal confirmed via
  `implementations/python/provekit-lift-py-numpy-testing/tests/test_inheritance_e2e.py`
  (2 passed in the numpy venv: `consumer-agrees-PROVEN`,
  `consumer-contradicts-REFUSED`) and the unit test
  `cross_proof_same_named_contracts_are_conjoined` in
  `implementations/rust/provekit-verifier/src/consistency.rs` (green).
- `2909 sugar members in a 13M .proof` confirmed live via
  `examples/numpy-vendor/run.sh` on numpy 2.4.6 (version-dependent count).

## What is stale, and why

### P0: actively wrong, fix first

1. **`docs/reference/per-language-status.md`** says "protocol v1.6.3 (CID
   `blake3-512:dd0cc...`)". The built binary reports a different catalog CID
   (`provekit verify-protocol` -> `blake3-512:809ed1eb...`, v1.6.6). The doc's
   value is the stale one. The real version is v1.6.6: the embedded asset is
   `catalog-signature-v1.6.6.json` (`protocolVersion: "v1.6.6"`, CID
   `809ed1eb...`), wired via `include_bytes!` at
   `implementations/rust/provekit-cli/src/protocol.rs:50` against
   `EXPECTED_CATALOG_CID` at line 28. DONE: per-language-status.md now states
   v1.6.6 / `809ed1eb...` and points at `provekit verify-protocol` as the live
   authority so it cannot silently drift again. Backed by the passing test
   `embedded_catalog_recomputes_to_expected_cid`.

2. **The oracle trio is absent from the explanation docs.** Grep shows
   `product.md`, `architecture.md`, and `proofchain.md` use "witness" only in
   the OLD sense (implication witness, proof-file conformance witness). None
   describe the Source Oracle or the Witness Oracle, the untrusted-kit /
   rust-recomputes split, or the broken-oracle-vs-drift distinction. A reader of
   those docs would not know that a `.proof` carries identity (CIDs + loci),
   not bodies, and that bodies are resolved-and-recomputed on demand. This is
   the single biggest content gap.

### P1: incomplete given recent work

3. **`docs/explanation/architecture.md`** describes "compose/conjoin/prove/
   report" but predates cross-proof same-named-contract conjoin, which is the
   mechanism behind inheritance. It needs a section on callsite-keyed contracts
   and how the verifier conjoins same-named contracts across `.proof` files
   before the SAT check. Reference the conjoin unit test.

4. **`docs/explanation/proofchain.md`** lists envelope member kinds but does not
   include SourceMemento or WitnessMemento, the lean source mode (CIDs + spans,
   not inline bodies), or witness packages (`<cid>.witness`) deployed separately
   from the `.proof`. Update the member-kind inventory and the recompute story.

5. **`docs/explanation/product.md`** still frames composition failure as the
   motivating-but-unsolved failure mode. It is now demonstrated end to end.
   Point it at the numpy inheritance capstone as proof it is real, not
   aspirational.

6. **CLI surface drift in any doc that enumerates subcommands.** The old README
   listed `link`, plus `prove`/`verify`/`materialize` framings that did not
   match `provekit --help`. `lift` is a stub ("TS only in v1.0; planned for
   v1.2.0"); `mint` is the verb that dispatches lift. Any reference doc that
   lists subcommands should be regenerated from `--help`, and `prove` should be
   described as the six-stage verifier.

### P2: verify, may be fine

7. **`docs/quickstart-end-user.md`** has zero mentions of oracle/witness/
   inherit/recompute. Confirm its first-run flow still matches current `mint` /
   `prove` / `verify` behavior, and consider adding the numpy vendor demo as the
   showcase first run, since it is the most legible end-to-end artifact now.

8. **`docs/quickstart-extender.md`**: confirm the kit-authoring story now
   covers `resolve_witness` (the RPC method the witness oracle dispatches) and
   the lean source-resolve path, since a new kit must implement body resolution,
   not body embedding.

9. **`docs/how-to/publishing-a-proof.md`**: confirm it covers shipping a
   `.proof` plus a separately deployed witness package, per the vendor demo.

## Prioritized refresh order

1. `docs/reference/per-language-status.md` protocol-CID line (P0-1). Smallest,
   most concretely wrong, and the README now delegates the live number here.
2. Oracle trio into `docs/explanation/architecture.md` and
   `docs/explanation/proofchain.md` (P0-2, P1-3, P1-4). The biggest content gap;
   do architecture and proofchain together since they share the member-kind and
   recompute vocabulary.
3. `docs/explanation/product.md` composition-failure section -> point at the
   capstone (P1-5).
4. CLI subcommand enumerations regenerated from `--help` wherever they appear
   (P1-6).
5. Quickstarts and how-to verification pass (P2-7, P2-8, P2-9).

## Not in scope here

- The compared-to/ docs were not audited for staleness; spot-check them in a
  later pass.
- The paper ladder (`docs/papers/`) is narrative, not API surface; lower
  priority unless a paper claims a now-superseded mechanism.
