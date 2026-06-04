# SPDX-License-Identifier: Apache-2.0
#
# Minimal lift plugin (provekit-lift/1, NDJSON over stdio) for the
# numpy.testing assertion seat.  Reuses the pytest seat's IR serialization so
# the emitted ir-document is byte-shaped identically; only the lift function
# differs (lift_file_numpy_testing instead of lift_file_layer2).
from __future__ import annotations

import json
import os
import sys
import traceback
from typing import Any, Dict, List, Optional

from provekit_lift_py_tests.ir import declarations_to_value
from provekit_lift_py_tests.canonicalizer import encode_jcs

from .numpy_testing_layer import lift_file_numpy_testing

KIT_ID = "python-numpy-testing"
KIT_VERSION = "0.1.0"
KIT_DECLARATION_RPC_METHOD = "provekit.plugin.kit_declaration"
SHARED_LSP_PROTOCOL_VERSION = "provekit-lsp-shared/1"


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
            dirnames[:] = [
                d for d in dirnames
                if d not in {".git", ".venv", "venv", "__pycache__", ".mypy_cache", ".pytest_cache"}
            ]
            for filename in filenames:
                if filename.endswith(".py"):
                    out.append(os.path.join(dirpath, filename))
    return sorted(set(out))


def handle_initialize(msg_id: Any) -> None:
    _send({
        "jsonrpc": "2.0",
        "id": msg_id,
        "result": {
            "name": "provekit-lsp-numpy-testing",
            "version": KIT_VERSION,
            "protocol_version": SHARED_LSP_PROTOCOL_VERSION,
            "kit_id": KIT_ID,
            "capabilities": {
                "source_surfaces": ["python-numpy-testing"],
                "entry_kinds": [],
                "diagnostic_codes": ["provekit.lsp.parse_error"],
                "status_kinds": ["prove"],
            },
        },
    })


def handle_kit_declaration(msg_id: Any) -> None:
    _send({
        "jsonrpc": "2.0",
        "id": msg_id,
        "result": {
            "kit": {"id": KIT_ID, "language": "python", "version": KIT_VERSION},
            "rpc": {"methods": [
                {"name": "initialize", "required": True},
                {"name": KIT_DECLARATION_RPC_METHOD, "required": True},
                {"name": "lift", "required": True},
                {"name": "shutdown", "required": False},
            ]},
            "proofResolution": {"strategy": "pip"},
            "effectKinds": [],
            "effectLeaves": [],
            "guardPredicates": [],
            "controlCarriers": [],
            "residueCategories": [],
        },
    })


def handle_lift(msg_id: Any, params: dict) -> None:
    workspace_root = str(params.get("workspace_root", "."))
    source_paths = params.get("source_paths", ["."])
    try:
        decls: List[Any] = []
        warnings: List[Any] = []
        for path in _iter_python_files(workspace_root, source_paths):
            try:
                with open(path, "r", encoding="utf-8") as f:
                    source = f.read()
            except OSError as e:
                warnings.append({"source_path": path, "item_name": "<file>", "reason": f"read failed: {e}"})
                continue
            out = lift_file_numpy_testing(source, path)
            decls.extend(out.decls)
            warnings.extend(w.__dict__ for w in out.warnings)

        ir: List[Any] = []
        if decls:
            ir = json.loads(encode_jcs(declarations_to_value(decls)))

        _send({
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "kind": "ir-document",
                "ir": ir,
                "implications": [],
                "diagnostics": [],
                "warnings": warnings,
            },
        })
    except Exception as e:
        _send({
            "jsonrpc": "2.0",
            "id": msg_id,
            "error": {"code": -32603, "message": str(e), "data": traceback.format_exc()},
        })


def main() -> None:
    while True:
        msg = _recv()
        if msg is None:
            break
        method = msg.get("method")
        msg_id = msg.get("id")
        if method == "initialize":
            handle_initialize(msg_id)
        elif method == KIT_DECLARATION_RPC_METHOD:
            handle_kit_declaration(msg_id)
        elif method == "lift":
            handle_lift(msg_id, msg.get("params", {}))
        elif method == "shutdown":
            _send({"jsonrpc": "2.0", "id": msg_id, "result": None})
            break
        elif msg_id is not None:
            # Unknown method: reply null so the dispatcher doesn't stall.
            _send({"jsonrpc": "2.0", "id": msg_id, "result": None})


if __name__ == "__main__":
    main()
