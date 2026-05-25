# Python LSP shared protocol rebaseline

Date: 2026-05-25

Authority: `protocol/specs/2026-05-25-lsp-shared-protocol.md`

Related issues: #1501, #1486, #308, #664, #1489, #1491

Scope: documentation and audit only. This note classifies the current Python
LSP and lift surfaces against the shared LSP protocol after the #1520 boundary
tightening. It does not implement the shared helper.

## Boundary ruling

The shared target route for Python is:

```text
initialize -> analyzeDocument -> lsp-document-analysis
```

The LSP coordinator is language-agnostic. It owns editor document sync, routing,
cache invalidation, conversion to editor diagnostics, and merging normalized kit,
linkerd, verifier, materialize, emit, and check facts. It must not parse Python
source, compute Python source ranges, own pytest/unittest semantics, hardcode
Python package behavior, or read Python kit shim `.proof` artifacts as body
authority.

Python parsing, source ranges, decorators, pytest/unittest interpretation,
Pydantic/deal/Hypothesis surfaces, materialize routing, emit availability,
pytest check status, and package/proof-body availability belong to the Python
kit. The kit speaks RPC and returns normalized data.

## Current Python surfaces

| Surface | Current code path | Status vs shared LSP | Notes |
|---|---|---|---|
| Legacy Python LSP helper | `implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/lsp.py` | Current owner, stale protocol | Speaks `initialize`, `parse`, `lift`, `shutdown` and reports `protocol_version = "provekit-lift/1"`. It returns declarations, call edges, warnings, and implication records, not `kind = "lsp-document-analysis"`. |
| Python source lift kit | `implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/rpc.py` | Useful kit-owned lift surface, not LSP-shaped | Speaks `initialize`, `lift`, `compile`, `shutdown`; `lift` returns `kind = "ir-document"` with IR, diagnostics, opacity report, and refusals. This is a good implementation source for `analyzeDocument`, but it is not yet the shared LSP method. |
| Python bind lift kit | `implementations/python/provekit-lift-python-source/src/provekit_lift_python_source/bind_rpc.py` | Useful normalized binding surface, not LSP-shaped | Speaks `pep/1.7.0` and returns `ir-document` entries. It owns `@sugar.bind` parsing via Python code and should remain kit-owned. |
| Forward propagation demo | `implementations/python/provekit_lsp/forward_propagator.py`, `docs/research/2026-05-05-python-lsp-forward-propagator.md` | Demo-only diagnostic producer | Uses a tiny string-constraint model and old `implication-failed` code. It is not the Python LSP architecture; it should become one producer that consumes normalized `lsp-document-analysis` facts. |
| Pytest/unittest lift | `implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/layer2.py`, `walk.py`, `lsp.py` | Kit-owned and useful, missing source-range projection | The helper already knows Python test semantics. Shared LSP needs ranges on entries/diagnostics and stable `provekit.lsp.*` codes. |
| Materialize/realize status | `implementations/python/provekit-realize-python-core`, `provekit-realize-python-requests`, `provekit-realize-python-sqlite3`, `provekit-realize-python-aiosqlite` | Kit-owned, not LSP-wired | Realizers answer over RPC and some self-resolve shim `.proof` material. LSP needs status/refusal facts from kit RPC, not coordinator file reads or Python template parsing. |
| Emit/check status | `implementations/python/provekit-emit-python-pytest` | Kit-owned, not LSP-wired | Pytest source emission belongs here. Check execution and pytest semantics must be reported by a Python kit status/check RPC, not hardcoded in coordinator code. |
| Shared coordinator/linkerd | Rust LSP/linkerd code | Must stay language-agnostic | Any Python path through legacy `parseFile` must invoke the Python helper or a lossless adapter to the shared result shape. Linkerd must not become a Python parser. |

## Feature parity classification

| Feature | Current Python state | Shared LSP target |
|---|---|---|
| Lift from source | Partial. Python helpers parse Python source and emit normalized IR, but not the shared LSP shape with ranges. | `analyzeDocument` returns `lsp-document-analysis.entries` for contracts, sugar bindings, call edges, proof/concept sites, and ranges. |
| Materialize from sugar at boundary | Partial outside LSP. Realizer kits exist; LSP does not expose materialize availability/refusal. | Kit status helper reports per-site materialize state and refusals from Python-owned logic. |
| Emit | Partial outside LSP. Pytest emitter exists. | Kit status helper reports pytest emit availability and produced/checked state; coordinator only displays normalized status. |
| Check | Missing as shared LSP status. | Python kit owns pytest/runtime check semantics and returns normalized check status over RPC. |
| Prove | Partial outside LSP. CLI verifier can consume normalized data; LSP does not map nonzero proof receipts back to Python ranges. | Coordinator merges verifier receipts for real nonzero claims; `totalClaims: 0` is not displayed as proof success. |
| Forward propagation | Demo-only. | Diagnostic producer over normalized analysis facts with stable `provekit.lsp.*` codes. |

## Child implementation gaps

1. **Python shared helper method**
   - Modify: `implementations/python/provekit-lift-py-tests/src/provekit_lift_py_tests/lsp.py` or add a thin Python helper beside `provekit-lift-python-source`.
   - Work: expose `protocol_version = "provekit-lsp-shared/1"` and `analyzeDocument`.
   - Acceptance: request returns `kind = "lsp-document-analysis"` with `kit_id = "python"` and no coordinator-side Python parsing.

2. **Source ranges for Python facts**
   - Modify: Python lift/bind/layer2 walkers.
   - Work: attach ranges for `@sugar.bind`, decorators/contracts, pytest/unittest assertions, call edges, concept sites, and proof sites.
   - Acceptance: fixture with multiple Python sites produces distinct stable ranges.

3. **Materialize status RPC**
   - Modify: Python realizer/status helper.
   - Work: report per-site materialize availability/refusal using kit-owned realizer/package/proof-body resolution.
   - Acceptance: missing template or missing shim proof is a refusal/status, not a coordinator `.proof` read.

4. **Emit/check status RPC**
   - Modify: `implementations/python/provekit-emit-python-pytest` or a Python kit status helper.
   - Work: expose pytest emission availability and check result/refusal over RPC.
   - Acceptance: LSP can display pytest emit/check state without hardcoding pytest commands in Rust coordinator code.

5. **Forward propagation rebase**
   - Modify: `implementations/python/provekit_lsp/forward_propagator.py`.
   - Work: consume normalized entries/call edges from `lsp-document-analysis` and emit stable `provekit.lsp.implication_failed`.
   - Acceptance: the old demo remains covered as a diagnostic producer, not as the full LSP.

6. **Proof receipt mapping**
   - Modify: coordinator diagnostic/status mapping after the shared Python helper exists.
   - Work: map verifier receipts back to Python ranges only for real nonzero claims.
   - Acceptance: zero-claim fixtures produce warning/unknown/vacuous status, never green proof success.

## Prohibitions

- Do not add Python parsing to the Rust CLI, shared LSP coordinator, or linkerd.
- Do not add pytest/unittest/check semantics to the coordinator.
- Do not make the coordinator read Python kit shim `.proof` files, package
  resources, or kit-generated JSON projections as body authority.
- Do not treat the old forward-propagation prototype as the entire Python LSP.
