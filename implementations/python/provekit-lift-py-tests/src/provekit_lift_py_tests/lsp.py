# SPDX-License-Identifier: Apache-2.0
#
# provekit.lsp: Language Server Protocol plugin for Python.
#
# Implements the ProvekIt lift plugin protocol (provekit-lift/1): NDJSON over stdio.
# Messages:
#   { "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }
#   { "jsonrpc": "2.0", "id": 2, "method": "lift", "params": { "workspace_root": "...", "source_paths": [...] } }
#   { "jsonrpc": "2.0", "id": 3, "method": "shutdown" }
#
# Legacy parse method is retained for backward compatibility.
#
# The plugin walks Python source, lifts contracts, and returns IR JSON.

from __future__ import annotations

import json
import os
import sys
import traceback
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from .ir import (
    ContractDecl,
    BridgeDecl,
    contract_decl_to_value,
    declarations_to_value,
    call_edges_to_value,
    formula_to_value,
)
from .canonicalizer import encode_jcs, jcs_hash
from .layer2 import lift_file_layer2
from .walk import lift_production_walk
from .decorators import collect_module
from .lift.pydantic import lift_pydantic_model
from .cpython_ctypes_resolver import resolve_ctypes_calls


# ---------------------------------------------------------------------------
# Protocol types
# ---------------------------------------------------------------------------


def _send(obj: dict) -> None:
    payload = json.dumps(obj, separators=(",", ":"), ensure_ascii=False)
    sys.stdout.write(payload + "\n")
    sys.stdout.flush()


def _recv() -> Optional[dict]:
    line = sys.stdin.readline()
    if not line:
        return None
    try:
        return json.loads(line)
    except json.JSONDecodeError:
        return None


# ---------------------------------------------------------------------------
# Handlers
# ---------------------------------------------------------------------------


def handle_initialize(msg_id: Any) -> None:
    _send(
        {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "name": "provekit-lsp-python",
                "version": "0.1.0",
                "protocol_version": "provekit-lift/1",
                "capabilities": {
                    "authoring_surfaces": ["python-source"],
                    "ir_version": "v1.1.0",
                    "emits_signed_mementos": False,
                    "parse": True,
                },
            },
        }
    )


def _implications_to_json(layer2) -> List[Dict[str, Any]]:
    return [
        {
            "name": implication.name,
            "antecedent": implication.antecedent,
            "consequent": implication.consequent,
            "antecedentSlot": implication.antecedent_slot,
            "consequentSlot": implication.consequent_slot,
            "prover": implication.prover,
            "proofWitness": implication.proof_witness,
        }
        for implication in layer2.implications
    ]


def _lift_source(path: str, source: str) -> Dict[str, Any]:
    decls: List[Any] = []

    # Layer 2: pytest/unittest structural lift.
    layer2 = lift_file_layer2(source, path)
    decls.extend(layer2.decls)

    # Production walk: lift callee preconditions and mint callsite WP edges.
    production_walk = lift_production_walk(source, path)
    decls.extend(production_walk.decls)

    # Try to load the source as a module to collect @provekit.contract
    # decorators. This only works when the source is importable; for
    # standalone files we skip this path.
    # TODO: use importlib.util to load from source string.

    # Pydantic lift: if the file defines BaseModel subclasses, walk them.
    # We do this by exec-ing the source in a clean namespace and
    # inspecting for pydantic models. Only done when pydantic is available.
    try:
        pydantic_decls = _try_lift_pydantic(source)
        decls.extend(pydantic_decls)
    except Exception:
        pass

    # Build contract index for call-edge resolution.
    # Maps function/contract name -> contractCid (blake3-512 hash of JCS).
    contract_index: Dict[str, str] = {}
    for d in decls:
        if isinstance(d, ContractDecl):
            cid = jcs_hash(contract_decl_to_value(d))
            contract_index[d.name] = cid

    # Emit ctypes call-edge stream per spec #114 R1.
    ctypes_result = resolve_ctypes_calls(source, path, contract_index)
    call_edges = ctypes_result.call_edges
    call_edges_value = call_edges_to_value(call_edges)
    call_edges_array = json.loads(encode_jcs(call_edges_value))

    declarations_array: List[Any] = []
    if decls:
        value = declarations_to_value(decls)
        declarations_array = json.loads(encode_jcs(value))

    return {
        "decls": decls,
        "declarations": declarations_array,
        "callEdges": call_edges_array,
        "warnings": [w.__dict__ for w in layer2.warnings + production_walk.warnings],
        "implications": _implications_to_json(layer2) + _implications_to_json(production_walk),
    }


def handle_parse(msg_id: Any, params: dict) -> None:
    path = params.get("path", "")
    source = params.get("source", "")
    language = params.get("language", "python")

    if language != "python":
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {
                    "code": -32602,
                    "message": f"language '{language}' not supported by this plugin",
                },
            }
        )
        return

    try:
        lifted = _lift_source(path, source)
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": {
                    "declarations": lifted["declarations"],
                    "callEdges": lifted["callEdges"],
                    "warnings": lifted["warnings"],
                    "implications": lifted["implications"],
                },
            }
        )

    except Exception as e:
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {
                    "code": -32603,
                    "message": str(e),
                    "data": traceback.format_exc(),
                },
            }
        )


def _iter_python_files(workspace_root: str, source_paths: List[Any]) -> List[str]:
    root = os.path.abspath(workspace_root or ".")
    paths = source_paths or ["."]
    out: List[str] = []
    for source_path in paths:
        raw = str(source_path)
        path = raw if os.path.isabs(raw) else os.path.join(root, raw)
        if os.path.isfile(path):
            if path.endswith(".py"):
                out.append(path)
            continue
        if not os.path.isdir(path):
            continue
        for dirpath, dirnames, filenames in os.walk(path):
            dirnames[:] = [
                d for d in dirnames
                if d not in {".git", ".venv", "venv", "__pycache__", ".mypy_cache", ".pytest_cache"}
            ]
            for filename in filenames:
                if filename.endswith(".py"):
                    out.append(os.path.join(dirpath, filename))
    return sorted(set(out))


def handle_lift(msg_id: Any, params: dict) -> None:
    workspace_root = str(params.get("workspace_root", "."))
    source_paths = params.get("source_paths", ["."])

    try:
        decls: List[Any] = []
        warnings: List[Any] = []
        implications: List[Any] = []
        for path in _iter_python_files(workspace_root, source_paths):
            try:
                with open(path, "r", encoding="utf-8") as f:
                    source = f.read()
            except OSError as e:
                warnings.append({
                    "source_path": path,
                    "item_name": "<file>",
                    "reason": f"read failed: {e}",
                })
                continue
            lifted = _lift_source(path, source)
            decls.extend(lifted["decls"])
            warnings.extend(lifted["warnings"])
            implications.extend(lifted["implications"])

        ir: List[Any] = []
        if decls:
            ir = json.loads(encode_jcs(declarations_to_value(decls)))

        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": {
                    "kind": "ir-document",
                    "ir": ir,
                    "implications": implications,
                    "diagnostics": [],
                    "warnings": warnings,
                },
            }
        )
    except Exception as e:
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {
                    "code": -32603,
                    "message": str(e),
                    "data": traceback.format_exc(),
                },
            }
        )


def _try_lift_pydantic(source: str) -> List[ContractDecl]:
    """Attempt to exec the source and lift any Pydantic BaseModels."""
    try:
        import pydantic
    except ImportError:
        return []

    namespace: dict = {}
    exec(source, namespace)

    decls: List[ContractDecl] = []
    for obj in namespace.values():
        if isinstance(obj, type) and hasattr(obj, "model_fields"):
            decls.extend(lift_pydantic_model(obj))
    return decls


def handle_shutdown(msg_id: Any) -> None:
    _send(
        {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": None,
        }
    )
    sys.exit(0)


# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------


def main() -> None:
    """Run the LSP plugin main loop (NDJSON over stdio)."""
    while True:
        msg = _recv()
        if msg is None:
            break
        msg_id = msg.get("id")
        method = msg.get("method")
        params = msg.get("params", {})

        if method == "initialize":
            handle_initialize(msg_id)
        elif method == "parse":
            handle_parse(msg_id, params)
        elif method == "lift":
            handle_lift(msg_id, params)
        elif method == "shutdown":
            handle_shutdown(msg_id)
        else:
            _send(
                {
                    "jsonrpc": "2.0",
                    "id": msg_id,
                    "error": {
                        "code": -32601,
                        "message": f"method '{method}' not found",
                    },
                }
            )


if __name__ == "__main__":
    main()
