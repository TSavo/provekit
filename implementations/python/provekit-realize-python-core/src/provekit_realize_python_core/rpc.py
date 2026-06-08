from __future__ import annotations

import base64
from fnmatch import fnmatch
from importlib import metadata as importlib_metadata
import json
from pathlib import Path
import py_compile
import sys
import traceback
from typing import Any


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

    if method == "provekit.plugin.platform_semantics":
        from .platform_semantics import declaration as _platform_semantics_declaration

        return {"jsonrpc": "2.0", "id": msg_id, "result": _platform_semantics_declaration()}
    if method == "provekit.plugin.literal_encoding_answers":
        from .literal_encoding import answers as _literal_encoding_answers

        return {"jsonrpc": "2.0", "id": msg_id, "result": {"answers": _literal_encoding_answers()}}
    if method == "provekit.plugin.invoke":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        from .realizer import MissingTemplateError

        try:
            result = _emit_one(params)
        except MissingTemplateError as exc:
            return _missing_template_error(msg_id, exc)
        return {"jsonrpc": "2.0", "id": msg_id, "result": result}
    if method == "provekit.plugin.assemble":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        from .assemble import assemble_response

        try:
            result = assemble_response(params)
        except ValueError as exc:
            return _error(msg_id, -32040, f"ASSEMBLE_FAILED: {exc}")
        except SyntaxError as exc:
            return _error(msg_id, -32040, f"ASSEMBLE_FAILED: {exc}")
        return {"jsonrpc": "2.0", "id": msg_id, "result": result}
    if method == "provekit.plugin.emit_module":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        functions = params.get("functions")
        if not isinstance(functions, list):
            return _error(msg_id, -32602, "INVALID_PARAMS: functions must be an array")
        from .realizer import MissingTemplateError

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
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        target_library_tag = params.get("target_library_tag")
        if not isinstance(target_library_tag, str):
            target_library_tag = None
        from . import realizer
        from .realizer import BodyTemplateResourceError

        try:
            entries = realizer.body_template_entries_for_library_tag(target_library_tag)
        except BodyTemplateResourceError as exc:
            return _body_template_resource_error(msg_id, exc)
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "entries": [_body_template_entry_json(entry) for entry in entries],
                "template_authority": realizer.KIT_ID,
            },
        }
    if method == "provekit.plugin.resolve_dependency_proofs":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {"proofs": _resolve_dependency_proofs()},
        }
    if method == "provekit.plugin.check":
        if not isinstance(params, dict):
            return _error(msg_id, -32602, "INVALID_PARAMS: params must be an object")
        return {"jsonrpc": "2.0", "id": msg_id, "result": _check_materialized(params)}
    if method == "provekit.plugin.shutdown":
        return {"jsonrpc": "2.0", "id": msg_id, "result": None}
    return _error(msg_id, -32601, f"METHOD_NOT_FOUND: {method}")


def _emit_one(params: dict[str, Any]) -> dict[str, Any]:
    from .realizer import emit_stub

    return emit_stub(
        function=str(params.get("function", "")),
        params=_string_list(params.get("params")),
        param_types=_string_list(params.get("param_types")),
        return_type=str(params.get("return_type", "")),
        concept_name=str(params.get("concept_name", "")),
        op_cid=params.get("op_cid", params.get("opCid"))
        if isinstance(params.get("op_cid", params.get("opCid")), str)
        else None,
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


def _resolve_dependency_proof_paths() -> list[str]:
    proof_paths: set[str] = set()
    for dist in importlib_metadata.distributions():
        for file in dist.files or ():
            if not fnmatch(Path(str(file)).name, "blake3-512:*.proof"):
                continue
            path = Path(dist.locate_file(file)).resolve()
            if path.is_file():
                proof_paths.add(str(path))
    return sorted(proof_paths)


def _resolve_dependency_proofs() -> list[dict[str, str]]:
    proofs: list[dict[str, str]] = []
    for path_text in _resolve_dependency_proof_paths():
        path = Path(path_text)
        proof_bytes = path.read_bytes()
        proofs.append(
            {
                "cid": path.name.removesuffix(".proof"),
                "bytes_base64": base64.b64encode(proof_bytes).decode("ascii"),
                "source": f"python-distribution:{path.name}",
            }
        )
    return proofs


def _check_materialized(params: dict[str, Any]) -> dict[str, Any]:
    out_dir = Path(str(params.get("out_dir", "")))
    py_files = sorted(path for path in out_dir.rglob("*.py") if path.is_file())
    errors: list[str] = []
    for path in py_files:
        try:
            py_compile.compile(str(path), doraise=True)
        except py_compile.PyCompileError as exc:
            errors.append(str(exc))
    return {
        "ok": not errors,
        "command": "python -m py_compile",
        "checked_files": [str(path) for path in py_files],
        "stderr": "\n".join(errors),
    }


def _body_template_entry_json(entry: Any) -> dict[str, Any]:
    guard: dict[str, Any] = {}
    if entry.min_params is not None:
        guard["min_params"] = entry.min_params
    if entry.max_params is not None:
        guard["max_params"] = entry.max_params
    if entry.requires_param_types is not None:
        guard["requires_param_types"] = list(entry.requires_param_types)
    if entry.requires_return_type is not None:
        guard["requires_return_type"] = entry.requires_return_type
    result = {
        "concept_name": entry.concept_name,
        "emission_template": {"kind": entry.template_kind, "template": entry.template},
        "signature_guard": guard,
    }
    if entry.op_cid is not None:
        result["op_cid"] = entry.op_cid
    if entry.target_library_tag is not None:
        result["target_library_tag"] = entry.target_library_tag
    return result


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


def _body_template_resource_error(
    msg_id: Any,
    exc: BodyTemplateResourceError,
) -> dict[str, Any]:
    from . import realizer

    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "error": {
            "code": 1404,
            "message": str(exc),
            "data": {
                "template_authority": realizer.KIT_ID,
                "target_library_tag": exc.target_library_tag,
                "missing_resources": list(exc.missing_resources),
            },
        },
    }
