from __future__ import annotations

import json
import sys
import traceback
from typing import Any

from .realizer import emit_stub


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
        except Exception as exc:
            response = _error(None, -32603, f"{exc}\n{traceback.format_exc()}")
        _send(response)
        if method == "provekit.plugin.shutdown":
            break


def dispatch(request: dict[str, Any]) -> dict[str, Any]:
    msg_id = request.get("id")
    method = str(request.get("method", ""))
    params = request.get("params") or {}

    if method == "provekit.plugin.invoke":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": emit_stub(
                function=str(params.get("function", "")),
                params=_string_list(params.get("params")),
                param_types=_string_list(params.get("param_types")),
                return_type=str(params.get("return_type", "")),
                concept_name=str(params.get("concept_name", "")),
                contract=params.get("contract") if isinstance(params.get("contract"), dict) else None,
                sugar_cids=_string_list(params.get("sugar_cids")),
                sugar_plugins=params.get("sugar_plugins")
                if isinstance(params.get("sugar_plugins"), list)
                else [],
            ),
        }
    if method == "provekit.plugin.shutdown":
        return {"jsonrpc": "2.0", "id": msg_id, "result": None}
    return _error(msg_id, -32601, f"METHOD_NOT_FOUND: {method}")


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [str(item) for item in value]


def _send(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":"), ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _error(msg_id: Any, code: int, message: str) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "error": {"code": code, "message": message},
    }
