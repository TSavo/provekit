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

import ast
import importlib
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
from .canonicalizer import blake3_512_of, encode_jcs, jcs_hash
from .layer2 import lift_file_layer2
from .walk import lift_production_walk
from .decorators import collect_module
from .lift.pydantic import lift_pydantic_model
from .cpython_ctypes_resolver import resolve_ctypes_calls


SHARED_PROTOCOL_VERSION = "provekit-lsp-shared/1"
LEGACY_PROTOCOL_VERSION = "provekit-lift/1"
KIT_ID = "python"
PROTOCOL_CATALOG_CID = blake3_512_of(SHARED_PROTOCOL_VERSION.encode("utf-8"))
REALIZER_RPC_BY_LIBRARY = {
    "python": ("provekit_realize_python_core.rpc", "provekit.plugin.platform_semantics"),
    "requests": ("provekit_realize_python_requests.rpc", "provekit.plugin.body_template_entries"),
    "sqlite3": ("provekit_realize_python_sqlite3.rpc", "provekit.plugin.body_template_entries"),
    "aiosqlite": ("provekit_realize_python_aiosqlite.rpc", "provekit.plugin.body_template_entries"),
}


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
                "protocol_version": SHARED_PROTOCOL_VERSION,
                "legacy_protocol_versions": [LEGACY_PROTOCOL_VERSION],
                "kit_id": KIT_ID,
                "protocol_catalog_cid": PROTOCOL_CATALOG_CID,
                "capabilities": {
                    "methods": ["initialize", "analyzeDocument", "parse", "lift", "shutdown"],
                    "source_surfaces": ["python-source"],
                    "entry_kinds": [
                        "bind-lift-entry",
                        "library-sugar-binding-entry",
                        "call-edge",
                        "contract",
                    ],
                    "diagnostic_codes": [
                        "provekit.lsp.parse_error",
                        "provekit.lsp.lift_gap",
                        "provekit.lsp.materialize_unavailable",
                        "provekit.lsp.materialize_refused",
                        "provekit.lsp.emit_unavailable",
                    ],
                    "status_kinds": ["lift", "materialize", "emit", "check", "prove"],
                    "authoring_surfaces": ["python-source"],
                    "ir_version": "v1.1.0",
                    "emits_signed_mementos": False,
                    "parse": True,
                    "analyzeDocument": True,
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


def handle_analyze_document(msg_id: Any, params: dict) -> None:
    kit_id = str(params.get("kit_id") or KIT_ID)
    language = str(params.get("language") or "python")
    if kit_id != KIT_ID or language != "python":
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {
                    "code": -32602,
                    "message": f"document not supported by python kit: kit_id={kit_id!r}, language={language!r}",
                },
            }
        )
        return

    try:
        _send({"jsonrpc": "2.0", "id": msg_id, "result": _analyze_document(params)})
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


def _analyze_document(params: dict[str, Any]) -> dict[str, Any]:
    source = str(params.get("text", params.get("source", "")))
    uri = str(params.get("uri") or "")
    file_param = str(params.get("file") or params.get("path") or uri or "<memory>")
    workspace_root = str(params.get("workspace_root") or "")
    display_file = _display_file(file_param, workspace_root)
    fallback_range = _whole_document_range(source)
    entries: list[dict[str, Any]] = []
    diagnostics: list[dict[str, Any]] = []

    try:
        ast.parse(source, filename=display_file)
    except SyntaxError as exc:
        diagnostics.append(_syntax_diagnostic(exc, fallback_range))
        return _analysis_result(
            params,
            source,
            uri,
            display_file,
            entries,
            diagnostics,
            _status_facts(entries, diagnostics, fallback_range),
        )

    lifted = _lift_source(display_file, source)
    entries.extend(_contract_entries(lifted.get("declarations", []), fallback_range))
    entries.extend(_call_edge_entries(lifted.get("callEdges", []), fallback_range))
    diagnostics.extend(_warning_diagnostics(lifted.get("warnings", []), fallback_range))
    entries.extend(_bind_lift_entries(source, display_file, diagnostics, fallback_range))

    return _analysis_result(
        params,
        source,
        uri,
        display_file,
        entries,
        diagnostics,
        _status_facts(entries, diagnostics, fallback_range),
    )


def _analysis_result(
    params: dict[str, Any],
    source: str,
    uri: str,
    display_file: str,
    entries: list[dict[str, Any]],
    diagnostics: list[dict[str, Any]],
    statuses: list[dict[str, Any]],
) -> dict[str, Any]:
    accepted = params.get("accepted_protocol_catalog_cids")
    protocol_catalog_cid = (
        str(accepted[0])
        if isinstance(accepted, list) and accepted and isinstance(accepted[0], str)
        else PROTOCOL_CATALOG_CID
    )
    return {
        "kind": "lsp-document-analysis",
        "schema_version": "1",
        "kit_id": KIT_ID,
        "uri": uri,
        "file": display_file,
        "document_cid": blake3_512_of(source.encode("utf-8")),
        "protocol_catalog_cid": protocol_catalog_cid,
        "entries": entries,
        "diagnostics": diagnostics,
        "statuses": statuses,
        "project": None,
    }


def _display_file(file_param: str, workspace_root: str) -> str:
    if file_param.startswith("file://"):
        return file_param
    if workspace_root and os.path.isabs(file_param):
        try:
            rel = os.path.relpath(file_param, workspace_root)
            if not rel.startswith(".."):
                return rel.replace(os.sep, "/")
        except ValueError:
            pass
    return file_param.replace(os.sep, "/")


def _whole_document_range(source: str) -> dict[str, int]:
    lines = source.splitlines()
    if not lines:
        return {"start_line": 1, "start_col": 0, "end_line": 1, "end_col": 0}
    return {
        "start_line": 1,
        "start_col": 0,
        "end_line": len(lines),
        "end_col": len(lines[-1]),
    }


def _syntax_diagnostic(exc: SyntaxError, fallback_range: dict[str, int]) -> dict[str, Any]:
    line = exc.lineno or fallback_range["start_line"]
    col = max((exc.offset or 1) - 1, 0)
    return {
        "code": "provekit.lsp.parse_error",
        "message": exc.msg,
        "severity": "error",
        "range": {
            "start_line": line,
            "start_col": col,
            "end_line": line,
            "end_col": col + 1,
        },
        "producer": "kit",
        "kit_id": KIT_ID,
    }


def _warning_diagnostics(warnings: list[Any], fallback_range: dict[str, int]) -> list[dict[str, Any]]:
    diagnostics: list[dict[str, Any]] = []
    for warning in warnings:
        message = ""
        if isinstance(warning, dict):
            message = str(warning.get("reason") or warning.get("message") or warning)
        else:
            message = str(warning)
        diagnostics.append(
            {
                "code": "provekit.lsp.lift_gap",
                "message": message,
                "severity": "warning",
                "range": fallback_range,
                "producer": "kit",
                "kit_id": KIT_ID,
                "data": warning,
            }
        )
    return diagnostics


def _contract_entries(declarations: list[Any], fallback_range: dict[str, int]) -> list[dict[str, Any]]:
    return [
        {"kind": "contract", "entry": declaration, "range": fallback_range}
        for declaration in declarations
        if isinstance(declaration, dict)
    ]


def _call_edge_entries(call_edges: list[Any], fallback_range: dict[str, int]) -> list[dict[str, Any]]:
    return [
        {"kind": "call-edge", "entry": call_edge, "range": fallback_range}
        for call_edge in call_edges
        if isinstance(call_edge, dict)
    ]


def _bind_lift_entries(
    source: str,
    display_file: str,
    diagnostics: list[dict[str, Any]],
    fallback_range: dict[str, int],
) -> list[dict[str, Any]]:
    try:
        bind_lifter = importlib.import_module("provekit_lift_python_source.bind_lifter")
    except ImportError as exc:
        diagnostics.append(
            {
                "code": "provekit.lsp.lift_gap",
                "message": f"python bind lifter unavailable: {exc}",
                "severity": "information",
                "range": fallback_range,
                "producer": "kit",
                "kit_id": KIT_ID,
            }
        )
        return []

    result = bind_lifter.lift_source(source, display_file, layer="all")
    for diagnostic in getattr(result, "diagnostics", []):
        diagnostics.append(_bind_diagnostic(diagnostic, fallback_range))

    entries: list[dict[str, Any]] = []
    for entry in getattr(result, "ir", []):
        if not isinstance(entry, dict):
            continue
        kind = str(entry.get("kind") or "python-lift-entry")
        if kind not in {"bind-lift-entry", "library-sugar-binding-entry"}:
            continue
        entries.append(
            {
                "kind": kind,
                "entry": entry,
                "range": _entry_range(entry, fallback_range),
            }
        )
    return entries


def _bind_diagnostic(diagnostic: Any, fallback_range: dict[str, int]) -> dict[str, Any]:
    line = diagnostic.get("line") if isinstance(diagnostic, dict) else None
    range_ = (
        {"start_line": int(line), "start_col": 0, "end_line": int(line), "end_col": 1}
        if isinstance(line, int)
        else fallback_range
    )
    kind = str(diagnostic.get("kind") if isinstance(diagnostic, dict) else "lift-gap")
    code = "provekit.lsp.parse_error" if kind == "parse-error" else "provekit.lsp.lift_gap"
    return {
        "code": code,
        "message": str(diagnostic.get("message") if isinstance(diagnostic, dict) else diagnostic),
        "severity": "error" if code == "provekit.lsp.parse_error" else "warning",
        "range": range_,
        "producer": "kit",
        "kit_id": KIT_ID,
        "data": diagnostic,
    }


def _entry_range(entry: dict[str, Any], fallback_range: dict[str, int]) -> dict[str, int]:
    body_source = entry.get("body_source")
    if isinstance(body_source, dict):
        span = body_source.get("span")
        if isinstance(span, dict) and _valid_range(span):
            return {
                "start_line": int(span["start_line"]),
                "start_col": int(span["start_col"]),
                "end_line": int(span["end_line"]),
                "end_col": int(span["end_col"]),
            }
    line = entry.get("fn_line") or entry.get("line")
    if isinstance(line, int):
        return {"start_line": line, "start_col": 0, "end_line": line, "end_col": 1}
    return fallback_range


def _valid_range(span: dict[str, Any]) -> bool:
    return all(isinstance(span.get(key), int) for key in ("start_line", "start_col", "end_line", "end_col"))


def _status_facts(
    entries: list[dict[str, Any]],
    diagnostics: list[dict[str, Any]],
    fallback_range: dict[str, int],
) -> list[dict[str, Any]]:
    range_ = entries[0]["range"] if entries else fallback_range
    lift_state = "refused" if any(d["severity"] == "error" for d in diagnostics) else "available"
    statuses = [
        {
            "kind": "lift",
            "range": range_,
            "state": lift_state,
            "producer": "kit",
            "message": f"Python kit analyzed document with {len(entries)} normalized entries.",
            "data": {"entry_count": len(entries)},
        }
    ]
    statuses.append(_materialize_status(entries, range_))
    statuses.append(
        _rpc_backend_status(
            kind="emit",
            producer="emit",
            module_name="provekit_emit_python_pytest.rpc",
            method="provekit.plugin.describe",
            range_=range_,
        )
    )
    statuses.append(
        _missing_backend_status(
            kind="check",
            producer="check",
            range_=range_,
            message="Python kit has no check-status RPC for pytest execution yet.",
        )
    )
    statuses.append(
        {
            "kind": "prove",
            "range": range_,
            "state": "unknown",
            "producer": "verifier",
            "message": "No nonzero proof receipt was supplied to the Python kit; not reporting proof success.",
            "data": {"proof_receipt": None},
        }
    )
    return statuses


def _materialize_status(entries: list[dict[str, Any]], range_: dict[str, int]) -> dict[str, Any]:
    library_tags = sorted(
        {
            str(entry["entry"].get("target_library_tag"))
            for entry in entries
            if entry.get("kind") == "library-sugar-binding-entry"
            and isinstance(entry.get("entry"), dict)
            and entry["entry"].get("target_library_tag")
        }
    )
    library_tag = library_tags[0] if library_tags else "python"
    route = REALIZER_RPC_BY_LIBRARY.get(library_tag)
    if route is None:
        return _missing_backend_status(
            kind="materialize",
            producer="materialize",
            range_=range_,
            message=f"Python kit has no registered materialize RPC for library tag {library_tag!r}.",
        )
    module_name, method = route
    return _rpc_backend_status(
        kind="materialize",
        producer="materialize",
        module_name=module_name,
        method=method,
        range_=range_,
    )


def _rpc_backend_status(
    *,
    kind: str,
    producer: str,
    module_name: str,
    method: str,
    range_: dict[str, int],
) -> dict[str, Any]:
    try:
        module = importlib.import_module(module_name)
        response = module.dispatch({"jsonrpc": "2.0", "id": 1, "method": method, "params": {}})
    except ImportError as exc:
        return _missing_backend_status(
            kind=kind,
            producer=producer,
            range_=range_,
            message=f"{module_name} is not importable: {exc}",
        )
    except Exception as exc:
        return {
            "kind": kind,
            "range": range_,
            "state": "refused",
            "producer": producer,
            "message": f"{module_name}.{method} refused: {exc}",
            "data": {"module": module_name, "method": method},
        }

    if isinstance(response, dict) and "result" in response:
        return {
            "kind": kind,
            "range": range_,
            "state": "available",
            "producer": producer,
            "message": f"{module_name} answered {method}.",
            "data": {"module": module_name, "method": method},
        }
    return {
        "kind": kind,
        "range": range_,
        "state": "refused",
        "producer": producer,
        "message": str(response.get("error", response)) if isinstance(response, dict) else str(response),
        "data": {"module": module_name, "method": method, "response": response},
    }


def _missing_backend_status(
    *,
    kind: str,
    producer: str,
    range_: dict[str, int],
    message: str,
) -> dict[str, Any]:
    return {
        "kind": kind,
        "range": range_,
        "state": "unavailable",
        "producer": producer,
        "message": message,
    }


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
        elif method == "analyzeDocument":
            handle_analyze_document(msg_id, params)
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
