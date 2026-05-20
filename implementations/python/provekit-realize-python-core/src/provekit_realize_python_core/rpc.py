from __future__ import annotations

import json
import sys
import traceback
from typing import Any

from .literal_encoding import answers as _literal_encoding_answers
from .platform_semantics import declaration as _platform_semantics_declaration
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
    params = request.get("params") or {}

    if method == "provekit.plugin.platform_semantics":
        return {"jsonrpc": "2.0", "id": msg_id, "result": _platform_semantics_declaration()}
    if method == "provekit.plugin.literal_encoding_answers":
        return {"jsonrpc": "2.0", "id": msg_id, "result": {"answers": _literal_encoding_answers()}}
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
        contract=params.get("contract") if isinstance(params.get("contract"), dict) else None,
        transported_op=params.get("transported_op")
        if isinstance(params.get("transported_op"), dict)
        else None,
        sugar_cids=_string_list(params.get("sugar_cids")),
        sugar_plugins=params.get("sugar_plugins")
        if isinstance(params.get("sugar_plugins"), list)
        else [],
        named_term_tree=params.get("named_term_tree")
        if isinstance(params.get("named_term_tree"), dict)
        else None,
        term_shape=params.get("term_shape")
        if isinstance(params.get("term_shape"), dict)
        else None,
        operand_bindings=params.get("operand_bindings")
        if isinstance(params.get("operand_bindings"), list)
        else None,
        source_function_name=params.get("source_function_name")
        if isinstance(params.get("source_function_name"), str)
        else None,
        annotate=_annotation_enabled(params),
    )


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [str(item) for item in value]


def _annotation_enabled(params: dict[str, Any]) -> bool:
    rewrite = params.get("rewrite", params.get("provekit_rewrite"))
    if rewrite == "annotate":
        return True
    flags = params.get("flags")
    if isinstance(flags, list) and "# provekit-rewrite: annotate" in flags:
        return True
    return params.get("annotate") is True


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
