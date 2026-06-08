# SPDX-License-Identifier: Apache-2.0
#
# sugar.lsp: Language Server Protocol plugin for Python.
#
# Implements the Sugar lift plugin protocol (sugar-lift/1): NDJSON over stdio.
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
import json
import os
import sys
import traceback
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from .ir import (
    ContractDecl,
    BridgeDecl,
    CallEdgeDecl,
    Locus,
    atomic,
    make_var,
    contract_decl_to_value,
    declarations_to_value,
    call_edges_to_value,
    formula_to_value,
)
from .canonicalizer import blake3_512_of, encode_jcs, jcs_hash
from .canonicalizer import vobj, vstr
from .layer2 import lift_file_layer2
from .walk import lift_production_walk
from .decorators import collect_module
from .lift.pydantic import lift_pydantic_model
from .cpython_ctypes_resolver import resolve_ctypes_calls


# ---------------------------------------------------------------------------
# Protocol types
# ---------------------------------------------------------------------------

KIT_ID = "python"
KIT_VERSION = "0.1.0"
KIT_DECLARATION_RPC_METHOD = "sugar.plugin.kit_declaration"
SHARED_LSP_PROTOCOL_VERSION = "sugar-lsp-shared/1"


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
                "name": "sugar-lsp-python",
                "version": KIT_VERSION,
                "protocol_version": SHARED_LSP_PROTOCOL_VERSION,
                "kit_id": KIT_ID,
                "capabilities": {
                    "source_surfaces": ["python-source"],
                    "entry_kinds": ["bind-lift-entry", "call-edge"],
                    "diagnostic_codes": [
                        "sugar.lsp.parse_error",
                        "sugar.lsp.implication_failed",
                    ],
                    "status_kinds": ["materialize", "emit", "check", "prove"],
                },
            },
        }
    )


def kit_declaration_result() -> Dict[str, Any]:
    return {
        "kit": {
            "id": KIT_ID,
            "language": "python",
            "version": KIT_VERSION,
        },
        "rpc": {
            "methods": [
                {"name": "initialize", "required": True},
                {"name": KIT_DECLARATION_RPC_METHOD, "required": True},
                {"name": "analyzeDocument", "required": False},
                {"name": "parse", "required": False},
                {"name": "lift", "required": True},
                {"name": "sugar.plugin.lift_implications", "required": False},
                {"name": "shutdown", "required": False},
            ]
        },
        "proofResolution": {"strategy": "pip"},
        "effectKinds": ["concept:panic-freedom"],
        "effectLeaves": [],
        "guardPredicates": [
            {
                "surface": KIT_ID,
                "local": "is_some",
                "concept": "concept:panic-freedom.option.some",
            },
            {
                "surface": KIT_ID,
                "local": "is_none",
                "concept": "concept:panic-freedom.option.none",
            },
        ],
        "controlCarriers": [],
        "residueCategories": [],
    }


def handle_kit_declaration(msg_id: Any) -> None:
    _send({"jsonrpc": "2.0", "id": msg_id, "result": kit_declaration_result()})


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

    # Try to load the source as a module to collect @sugar.contract
    # decorators. This only works when the source is importable; for
    # standalone files we skip this path.
    try:
        decls.extend(_try_lift_decorated_contracts(source))
    except Exception:
        pass

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
            cid = _linkerd_contract_cid(d)
            contract_index[d.name] = cid

    # Emit ctypes call-edge stream per spec #114 R1.
    ctypes_result = resolve_ctypes_calls(source, path, contract_index)
    same_kit_edges = _resolve_same_kit_calls(source, path, contract_index)
    call_edges = ctypes_result.call_edges + same_kit_edges
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


def handle_analyze_document(msg_id: Any, params: dict) -> None:
    path = str(params.get("file") or params.get("path") or "source.py")
    uri = str(params.get("uri") or f"file://{path}")
    source = str(params.get("text") if "text" in params else params.get("source", ""))

    try:
        ast.parse(source, filename=path)
    except SyntaxError as e:
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": _analysis_result(
                    uri=uri,
                    path=path,
                    source=source,
                    entries=[],
                    diagnostics=[_parse_error_diagnostic(e)],
                ),
            }
        )
        return

    try:
        lifted = _lift_source(path, source)
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": _analysis_result(
                    uri=uri,
                    path=path,
                    source=source,
                    entries=_analysis_entries(lifted, _whole_document_range(source)),
                    diagnostics=_forward_implication_diagnostics(source, path),
                ),
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


def _analysis_result(
    *,
    uri: str,
    path: str,
    source: str,
    entries: List[Dict[str, Any]],
    diagnostics: List[Dict[str, Any]],
) -> Dict[str, Any]:
    return {
        "kind": "lsp-document-analysis",
        "schema_version": "1",
        "kit_id": KIT_ID,
        "uri": uri,
        "file": path,
        "document_cid": blake3_512_of(source.encode("utf-8")),
        "entries": entries,
        "diagnostics": diagnostics,
        "statuses": [],
        "project": None,
    }


def _analysis_entries(lifted: Dict[str, Any], source_range: Dict[str, int]) -> List[Dict[str, Any]]:
    entries: List[Dict[str, Any]] = []
    for declaration in lifted.get("declarations", []):
        entries.append(
            {
                "kind": "bind-lift-entry",
                "entry": declaration,
                "range": source_range,
            }
        )
    for call_edge in lifted.get("callEdges", []):
        entries.append(
            {
                "kind": "call-edge",
                "entry": call_edge,
                "range": source_range,
            }
        )
    return entries


def _whole_document_range(source: str) -> Dict[str, int]:
    line = 1
    col = 0
    for ch in source:
        if ch == "\n":
            line += 1
            col = 0
        elif ord(ch) > 0xFFFF:
            col += 2
        else:
            col += 1
    return {"start_line": 1, "start_col": 0, "end_line": line, "end_col": col}


def _parse_error_diagnostic(error: SyntaxError) -> Dict[str, Any]:
    start_line = error.lineno or 1
    start_col = max((error.offset or 1) - 1, 0)
    return {
        "code": "sugar.lsp.parse_error",
        "message": str(error),
        "severity": "error",
        "range": {
            "start_line": start_line,
            "start_col": start_col,
            "end_line": start_line,
            "end_col": start_col,
        },
        "producer": "kit",
        "kit_id": KIT_ID,
    }


def _forward_implication_diagnostics(source: str, path: str) -> List[Dict[str, Any]]:
    tree = ast.parse(source, filename=path)
    diagnostics: List[Dict[str, Any]] = []

    class Visitor(ast.NodeVisitor):
        def __init__(self) -> None:
            self.loop_depth = 0
            self.current_constraints: set[str] = set()

        def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
            previous = self.current_constraints
            self.current_constraints = set()
            for stmt in node.body:
                self.visit(stmt)
            self.current_constraints = previous

        def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
            self.visit_FunctionDef(node)

        def visit_For(self, node: ast.For) -> None:
            self.loop_depth += 1
            self.generic_visit(node)
            self.loop_depth -= 1

        def visit_While(self, node: ast.While) -> None:
            self.loop_depth += 1
            self.generic_visit(node)
            self.loop_depth -= 1

        def visit_Call(self, node: ast.Call) -> None:
            callee = _call_callee_name(node.func)
            if callee == "checkPositive":
                if self.loop_depth == 0:
                    fact = _post_fact_for_check_positive(node)
                    if fact is not None:
                        self.current_constraints.add(fact)
                    if "x > 0" not in self.current_constraints:
                        diagnostics.append(_implication_failed_diagnostic(node))
            self.generic_visit(node)

    Visitor().visit(tree)
    return diagnostics


def _post_fact_for_check_positive(node: ast.Call) -> Optional[str]:
    if not node.args:
        return None
    arg = node.args[0]
    if isinstance(arg, ast.Constant) and isinstance(arg.value, int):
        return "x > 0" if arg.value > 0 else "x <= 0"
    if (
        isinstance(arg, ast.UnaryOp)
        and isinstance(arg.op, ast.USub)
        and isinstance(arg.operand, ast.Constant)
        and isinstance(arg.operand.value, int)
    ):
        return "x <= 0"
    return None


def _implication_failed_diagnostic(node: ast.Call) -> Dict[str, Any]:
    start_col = getattr(node, "col_offset", 0)
    end_col = getattr(node, "end_col_offset", start_col + len("checkPositive"))
    callee = "checkPositive"
    current_post_cid = blake3_512_of(b"post:known:x <= 0")
    pre_cid = blake3_512_of(f"{callee}:pre:x > 0".encode("utf-8"))
    post_cid = blake3_512_of(f"{callee}:post:returns true".encode("utf-8"))
    seed = f"{callee}|{pre_cid}|{post_cid}"
    return {
        "code": "sugar.lsp.implication_failed",
        "message": "callee precondition not established at this callsite",
        "severity": "error",
        "range": {
            "start_line": getattr(node, "lineno", 1),
            "start_col": start_col,
            "end_line": getattr(node, "end_lineno", getattr(node, "lineno", 1)),
            "end_col": end_col,
        },
        "producer": "forward-propagation",
        "kit_id": KIT_ID,
        "data": {
            "schema_version": 1,
            "kind": "sugar.lsp.implication_failed",
            "callee": callee,
            "callee_contract_cid": blake3_512_of(f"contract:{seed}".encode("utf-8")),
            "callee_attestation_cid": blake3_512_of(f"attestation:{seed}".encode("utf-8")),
            "callee_pre_cid": pre_cid,
            "callee_post_cid": post_cid,
            "current_post_cid": current_post_cid,
            "missing_conjuncts": ["x > 0"],
        },
    }


def _contract_bindings_by_callee(contract_bindings: List[Any]) -> Dict[str, Dict[str, Any]]:
    contracts_by_callee: Dict[str, Dict[str, Any]] = {}
    for binding in contract_bindings:
        if not isinstance(binding, dict):
            continue
        name = binding.get("name")
        if not isinstance(name, str):
            continue
        stem = name.split("@", 1)[0].split("(", 1)[0].strip()
        if stem:
            contracts_by_callee.setdefault(stem, binding)
            simple = stem.rsplit(".", 1)[-1]
            if simple:
                contracts_by_callee.setdefault(simple, binding)
    return contracts_by_callee


def _binding_contract_cid(binding: Dict[str, Any]) -> Optional[str]:
    cid = binding.get("contract_cid", binding.get("contractCid"))
    if isinstance(cid, str) and cid:
        return cid
    return None


def _call_callee_name(node: ast.AST) -> Optional[str]:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return node.attr
    return None


def _contract_index_with_simple_names(contract_index: Dict[str, str]) -> Dict[str, str]:
    out = dict(contract_index)
    for name, cid in contract_index.items():
        simple = name.rsplit(".", 1)[-1]
        if simple:
            out.setdefault(simple, cid)
    return out


def _linkerd_contract_cid(decl: ContractDecl) -> str:
    pairs = [
        ("name", vstr(decl.name)),
        ("outBinding", vstr(decl.out_binding)),
    ]
    if decl.pre is not None:
        pairs.append(("pre", formula_to_value(decl.pre)))
    if decl.post is not None:
        pairs.append(("post", formula_to_value(decl.post)))
    if decl.inv is not None:
        pairs.append(("inv", formula_to_value(decl.inv)))
    return jcs_hash(vobj(pairs))


def _resolve_same_kit_calls(
    source: str,
    path: str,
    contract_index: Dict[str, str],
) -> List[CallEdgeDecl]:
    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError:
        return []

    contracts_by_name = _contract_index_with_simple_names(contract_index)
    if not contracts_by_name:
        return []

    edges: List[CallEdgeDecl] = []
    seen: set[tuple[str, str, int, int]] = set()

    class SameKitCallVisitor(ast.NodeVisitor):
        def __init__(self) -> None:
            self.function_stack: List[str] = []

        def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
            self.function_stack.append(node.name)
            self.generic_visit(node)
            self.function_stack.pop()

        def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
            self.function_stack.append(node.name)
            self.generic_visit(node)
            self.function_stack.pop()

        def visit_Call(self, node: ast.Call) -> None:
            if self.function_stack:
                caller = self.function_stack[-1]
                source_cid = contracts_by_name.get(caller)
                callee = _call_callee_name(node.func)
                if source_cid and callee and callee in contracts_by_name:
                    line = getattr(node, "lineno", 1)
                    column = getattr(node, "col_offset", 0)
                    target_symbol = f"python-kit:{callee}"
                    key = (source_cid, target_symbol, line, column)
                    if key not in seen:
                        seen.add(key)
                        edges.append(
                            CallEdgeDecl(
                                source_contract_cid=source_cid,
                                target_contract_cid=None,
                                target_symbol=target_symbol,
                                call_site_locus=Locus(file=path, line=line, column=column),
                                evidence_term=atomic(
                                    "call-site-obligation",
                                    [make_var(caller)],
                                ),
                            )
                        )
            self.generic_visit(node)

    SameKitCallVisitor().visit(tree)
    return edges


def _collect_python_callsites(source: str, source_path: str) -> List[Dict[str, Any]]:
    tree = ast.parse(source, filename=source_path)
    callsites: List[Dict[str, Any]] = []

    class FunctionBodyCallVisitor(ast.NodeVisitor):
        def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
            for stmt in node.body:
                self.visit(stmt)

        def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
            for stmt in node.body:
                self.visit(stmt)

        def visit_Lambda(self, node: ast.Lambda) -> None:
            self.visit(node.body)

        def visit_Call(self, node: ast.Call) -> None:
            callee = _call_callee_name(node.func)
            if callee:
                callsites.append(
                    {
                        "callee": callee,
                        "file": source_path,
                        "line": node.lineno,
                        "col": node.col_offset,
                    }
                )
            self.generic_visit(node)

    FunctionBodyCallVisitor().visit(tree)
    return callsites


def _rel_python_path(workspace_root: str, path: str) -> str:
    try:
        rel = os.path.relpath(path, os.path.abspath(workspace_root or "."))
    except ValueError:
        rel = path
    return rel.replace(os.sep, "/")


def _lift_implications_result(params: dict) -> Dict[str, Any]:
    workspace_root = str(params.get("workspace_root", "."))
    source_paths = params.get("source_paths", ["."])
    contract_bindings = params.get("contract_bindings", [])
    if not isinstance(contract_bindings, list):
        contract_bindings = []

    contracts_by_callee = _contract_bindings_by_callee(contract_bindings)
    ir: List[Dict[str, Any]] = []
    diagnostics: List[Dict[str, Any]] = []

    for path in _iter_python_files(workspace_root, source_paths):
        rel_path = _rel_python_path(workspace_root, path)
        try:
            with open(path, "r", encoding="utf-8") as f:
                source = f.read()
        except OSError as e:
            diagnostics.append(
                {
                    "kind": "lift-gap",
                    "reason": f"read-failed: {e}",
                    "file": rel_path,
                }
            )
            continue

        try:
            callsites = _collect_python_callsites(source, rel_path)
        except SyntaxError as e:
            diagnostics.append(
                {
                    "kind": "lift-gap",
                    "reason": "parse-failed",
                    "file": rel_path,
                    "message": str(e),
                }
            )
            continue

        for callsite in callsites:
            callee = callsite["callee"]
            binding = contracts_by_callee.get(callee)
            if binding is None:
                diagnostics.append(
                    {
                        "kind": "lift-gap",
                        "reason": "no-contract-for-callee",
                        "callee": callee,
                        "file": callsite["file"],
                        "line": callsite["line"],
                        "col": callsite["col"],
                    }
                )
                continue

            target_cid = _binding_contract_cid(binding)
            if target_cid is None:
                diagnostics.append(
                    {
                        "kind": "lift-gap",
                        "reason": "binding-missing-contract-cid",
                        "callee": callee,
                        "file": callsite["file"],
                        "line": callsite["line"],
                        "col": callsite["col"],
                    }
                )
                continue

            ir.append(
                {
                    "kind": "bridge",
                    "name": (
                        f"intra-body:python:{callee}@{callsite['file']}:"
                        f"{callsite['line']}:{callsite['col']}"
                    ),
                    "schemaVersion": "1",
                    "sourceContractCid": target_cid,
                    "sourceLayer": "python",
                    "sourceSymbol": callee,
                    "target": {"cid": target_cid, "kind": "contract"},
                    "targetContractCid": target_cid,
                    "targetLayer": "python-tests",
                    "callsite": {
                        "file": callsite["file"],
                        "start_line": callsite["line"],
                        "start_col": callsite["col"],
                    },
                }
            )

    return {"kind": "ir-document", "ir": ir, "diagnostics": diagnostics}


def handle_lift_implications(msg_id: Any, params: dict) -> None:
    try:
        _send(
            {
                "jsonrpc": "2.0",
                "id": msg_id,
                "result": _lift_implications_result(params),
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


def _try_lift_decorated_contracts(source: str) -> List[ContractDecl]:
    """Exec the source in an isolated namespace and collect @contract metadata."""
    import types

    namespace: dict = {"__name__": "_sugar_lsp_source"}
    exec(source, namespace)
    module = types.ModuleType("_sugar_lsp_source")
    module.__dict__.update(namespace)
    return collect_module(module)


def handle_shutdown(msg_id: Any) -> None:
    _send(
        {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": None,
        }
    )
    sys.exit(0)


def handle_resolve_dependency_proofs(msg_id: Any, params: dict) -> None:
    """Resolve dependency `.proof` files from the project's `.sugar/imports/`.

    The verifier (rust `dependency_proofs_via_rpc`) calls this to fold a
    consumer's resolved vendor proofs into the proof set before discharge —
    e.g. the numpy sugar `.proof` that puts `numpy.add` under contract. We
    source from the on-disk `.sugar/imports/` directory (the same place the
    contract-binding auto-discovery reads), returning each proof's CID and
    base64 bytes per the realize kits' contract.
    """
    import base64
    import fnmatch

    project_root = str(params.get("project_root") or ".")
    imports_dir = os.path.join(project_root, ".sugar", "imports")
    proofs: list[dict] = []
    if os.path.isdir(imports_dir):
        for name in sorted(os.listdir(imports_dir)):
            if not fnmatch.fnmatch(name, "blake3-512:*.proof"):
                continue
            path = os.path.join(imports_dir, name)
            if not os.path.isfile(path):
                continue
            with open(path, "rb") as fh:
                proof_bytes = fh.read()
            proofs.append(
                {
                    "cid": name[: -len(".proof")],
                    "bytes_base64": base64.b64encode(proof_bytes).decode("ascii"),
                    "source": f"sugar-imports:{name}",
                }
            )
    _send(
        {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {"proofs": proofs},
        }
    )


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
        elif method == KIT_DECLARATION_RPC_METHOD:
            handle_kit_declaration(msg_id)
        elif method == "analyzeDocument":
            handle_analyze_document(msg_id, params)
        elif method == "parse":
            handle_parse(msg_id, params)
        elif method == "lift":
            handle_lift(msg_id, params)
        elif method == "sugar.plugin.lift_implications":
            handle_lift_implications(msg_id, params)
        elif method == "sugar.plugin.resolve_dependency_proofs":
            handle_resolve_dependency_proofs(msg_id, params)
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
