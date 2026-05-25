from __future__ import annotations

import json
import sys
import traceback
from typing import Any

from . import realizer
from .realizer import MissingTemplateError, emit_stub


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
    # Default ONLY when params is absent (None). Do NOT coerce a falsy non-dict
    # (e.g. []) to {} via `or`, which would bypass the isinstance(dict) guard
    # below and turn an INVALID_PARAMS error into a silent default plan.
    params = request.get("params")
    if params is None:
        params = {}

    if method == "provekit.plugin.invoke":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        try:
            result = _emit_one(params)
        except MissingTemplateError as exc:
            return _missing_template_error(msg_id, exc)
        return {"jsonrpc": "2.0", "id": msg_id, "result": result}
    if method == "provekit.plugin.emit_module":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        functions = params.get("functions")
        if not isinstance(functions, list):
            return _error(msg_id, -32602, "INVALID_PARAMS: functions must be an array")
        results: list[dict[str, Any]] = []
        missing = []
        for item in functions:
            if not isinstance(item, dict):
                continue
            try:
                results.append(_emit_one(item))
            except MissingTemplateError as exc:
                missing.extend(exc.entries)
        if missing:
            return _missing_template_error(msg_id, MissingTemplateError(tuple(missing)))
        source = "\n".join(result["source"] for result in results)
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "source": source,
                "is_stub": False,
                "extension": "py",
            },
        }
    if method == "provekit.plugin.body_template_entries":
        proof_path = realizer._resolve_shim_proof_path()
        if proof_path is None:
            return _error(msg_id, 1404, "SHIM_NOT_FOUND: provekit-shim-python-requests not installed")
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "entries": [
                    _body_template_entry_json(entry, realizer._LIBRARY_TAG)
                    for entry in realizer.entries()
                ],
                "proof_path": str(proof_path),
            },
        }
    if method == "provekit.plugin.shutdown":
        return {"jsonrpc": "2.0", "id": msg_id, "result": None}
    return _error(msg_id, -32601, f"METHOD_NOT_FOUND: {method}")


def _emit_one(params: dict[str, Any]) -> dict[str, Any]:
    return emit_stub(
        function=str(params.get("function", "")),
        params=_string_list(params.get("params")),
        param_types=_string_list(params.get("param_types")),
        return_type=str(params.get("return_type", "")),
        concept_name=str(params.get("concept_name", "")),
        named_term_tree=_dict_field(params, "named_term_tree", "namedTermTree"),
    )


def _body_template_entry_json(entry: realizer.BodyTemplateEntry, library_tag: str) -> dict[str, Any]:
    guard: dict[str, Any] = {}
    if entry.min_params is not None:
        guard["min_params"] = entry.min_params
    if entry.max_params is not None:
        guard["max_params"] = entry.max_params
    if entry.requires_param_types is not None:
        guard["requires_param_types"] = list(entry.requires_param_types)
    if entry.requires_return_type is not None:
        guard["requires_return_type"] = entry.requires_return_type
    return {
        "concept_name": entry.concept_name,
        "emission_template": {"kind": entry.template_kind, "template": entry.template},
        "signature_guard": guard,
        "target_library_tag": library_tag,
    }


def _dict_field(params: dict[str, Any], *keys: str) -> dict[str, Any] | None:
    for key in keys:
        value = params.get(key)
        if isinstance(value, dict):
            return value
    return None


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


def _missing_template_error(msg_id: Any, exc: MissingTemplateError) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [entry.to_json() for entry in exc.entries],
        },
    }
