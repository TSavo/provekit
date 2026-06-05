# SPDX-License-Identifier: Apache-2.0
#
# Witness lift surface (provekit-lift/1 NDJSON). At LIFT time this is the
# PRODUCER: it runs each test under pytest and emits a ContractDecl carrying the
# witnessed run as a `custom` EvidenceTerm. `mint` serializes it into a real
# signed .proof; `prove` then discharges it BY RECOMPUTE (the verifier's custom-
# evidence arm spawns the discharge command).
from __future__ import annotations

import json
import os
import sys
from typing import Any, List, Optional

from provekit_lift_py_tests.ir import (
    ContractDecl,
    EvidenceCertificate,
    EvidenceTerm,
    atomic,
    declarations_to_value,
)
from provekit_lift_py_tests.canonicalizer import encode_jcs

from .witness import run_and_witness

KIT_ID = "python-pytest-witness"
KIT_VERSION = "0.1.0"
KIT_DECLARATION_RPC_METHOD = "provekit.plugin.kit_declaration"


def _send(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":"), ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _recv() -> Optional[dict]:
    line = sys.stdin.readline()
    if not line:
        return None
    try:
        return json.loads(line)
    except json.JSONDecodeError:
        return None


def _iter_python_files(workspace_root: str, source_paths: List[str]) -> List[str]:
    out: List[str] = []
    for sp in source_paths:
        base = sp if os.path.isabs(sp) else os.path.join(workspace_root, sp)
        if os.path.isfile(base) and base.endswith(".py"):
            out.append(base)
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in {".git", "__pycache__", ".pytest_cache"}]
            for fn in filenames:
                if fn.endswith(".py"):
                    out.append(os.path.join(dirpath, fn))
    return sorted(set(out))


def handle_lift(msg_id: Any, params: dict) -> None:
    ws = str(params.get("workspace_root", "."))
    sps = params.get("source_paths", ["."])
    try:
        pyfiles = _iter_python_files(ws, sps)
        rels = [os.path.relpath(p, ws) for p in pyfiles]
        code_rels = [r for r in rels if not os.path.basename(r).startswith("test_")]
        test_rels = [r for r in rels if os.path.basename(r).startswith("test_")]
        decls: List[ContractDecl] = []
        for tr in test_rels:
            w = run_and_witness(ws, tr, code_rels)
            proof_data = json.dumps(
                {"codeCid": w.code_cid, "runtimeCid": w.runtime_cid, "test": w.test_id,
                 "outcome": w.outcome, "codeFiles": list(w.code_files)},
                sort_keys=True, separators=(",", ":"),
            )
            cert = EvidenceCertificate(
                tool="pytest", version=w.runtime_cid, formula_hash=w.cid, proof_data=proof_data,
            )
            ev = EvidenceTerm(proof_type="custom", certificate=cert)
            decls.append(ContractDecl(name=tr, inv=atomic("witnessed", []), evidence=ev))
        ir = json.loads(encode_jcs(declarations_to_value(decls))) if decls else []
        _send({"jsonrpc": "2.0", "id": msg_id, "result": {
            "kind": "ir-document", "ir": ir, "implications": [], "diagnostics": [], "warnings": [],
        }})
    except Exception as e:
        import traceback
        _send({"jsonrpc": "2.0", "id": msg_id, "error": {
            "code": -32603, "message": str(e), "data": traceback.format_exc()}})


def main() -> None:
    while True:
        msg = _recv()
        if msg is None:
            break
        method = msg.get("method")
        mid = msg.get("id")
        if method == "initialize":
            _send({"jsonrpc": "2.0", "id": mid, "result": {
                "name": "provekit-lsp-pytest-witness", "version": KIT_VERSION,
                "protocol_version": "provekit-lsp-shared/1", "kit_id": KIT_ID,
                "capabilities": {"source_surfaces": ["python-pytest-witness"], "entry_kinds": [],
                                 "diagnostic_codes": [], "status_kinds": ["prove"]}}})
        elif method == KIT_DECLARATION_RPC_METHOD:
            _send({"jsonrpc": "2.0", "id": mid, "result": {
                "kit": {"id": KIT_ID, "language": "python", "version": KIT_VERSION},
                "rpc": {"methods": [{"name": "initialize", "required": True},
                                    {"name": KIT_DECLARATION_RPC_METHOD, "required": True},
                                    {"name": "lift", "required": True},
                                    {"name": "shutdown", "required": False}]},
                "proofResolution": {"strategy": "pip"}, "effectKinds": [], "effectLeaves": [],
                "guardPredicates": [], "controlCarriers": [], "residueCategories": []}})
        elif method == "lift":
            handle_lift(mid, msg.get("params", {}))
        elif method == "shutdown":
            _send({"jsonrpc": "2.0", "id": mid, "result": None})
            break
        elif mid is not None:
            _send({"jsonrpc": "2.0", "id": mid, "result": None})


if __name__ == "__main__":
    main()
