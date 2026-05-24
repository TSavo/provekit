"""PEP 1.7.0 newline-delimited JSON-RPC server for the pytest emitter plugin.

Reads one JSON-RPC request per line on stdin, writes one response per line to
stdout. Mirrors the RPC conventions of the python realize kits
(``provekit-realize-python-sqlite3``) and the emitter protocol shape of the
java sibling (``provekit-emit-java-junit``).

Supported methods:

- ``provekit.plugin.describe``  - plugin self-description (capabilities +
  supported predicates).
- ``provekit.plugin.invoke``    - emit a pytest test module from an
  :class:`~provekit_emit_python_pytest.emitter.EmitPlan` carried in
  ``params``; returns an :class:`~provekit_emit_python_pytest.emitter.Emission`.
- ``provekit.plugin.shutdown``  - exit.

There is no body-emit, no assembly, no platform semantics: the emitter is a
predicate -> assertion table plus a test-module shell.
"""

from __future__ import annotations

import json
import sys
import traceback
from typing import Any

from .emitter import EmitPlan, emit
from .plugin_memento import PLUGIN_MEMENTO


def run_rpc() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        method = ""
        try:
            request = json.loads(line)
            method = str(request.get("method", ""))
            response = dispatch(request)
        except json.JSONDecodeError as exc:
            response = _error(None, -32700, f"PARSE_ERROR: {exc}")
        except Exception as exc:  # noqa: BLE001 - surface any plugin error to the host
            response = _error(None, -32603, f"{exc}\n{traceback.format_exc()}")
        _send(response)
        if method == "provekit.plugin.shutdown":
            break


def dispatch(request: dict[str, Any]) -> dict[str, Any]:
    msg_id = request.get("id")
    method = str(request.get("method", ""))
    params = request.get("params")
    if params is None:
        params = {}

    if method == "provekit.plugin.describe":
        # The result IS the plugin memento (loader.rs:parse_and_validate).
        # The loader recomputes header.cid and refuses on mismatch, so the
        # memento must be the full {envelope, header, metadata} shape, not a
        # flat capability object. Capabilities live inside header.content.
        return {"jsonrpc": "2.0", "id": msg_id, "result": PLUGIN_MEMENTO}

    if method == "provekit.plugin.invoke":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        plan = EmitPlan.from_params(params)
        emission = emit(plan)
        return {"jsonrpc": "2.0", "id": msg_id, "result": emission.to_json()}

    if method == "provekit.plugin.shutdown":
        return {"jsonrpc": "2.0", "id": msg_id, "result": None}

    return _error(msg_id, -32601, f"METHOD_NOT_FOUND: {method}")


def _send(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":"), ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _error(msg_id: Any, code: int, message: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "error": {"code": code, "message": message},
    }
