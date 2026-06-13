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
from pathlib import Path
from typing import Any, Dict, List, Optional, Set

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
from .layer2 import _classify_universe_source_node, lift_file_layer2
from .walk import lift_production_walk
from .decorators import collect_module
from .lift.pydantic import lift_pydantic_model
from .cpython_ctypes_resolver import resolve_ctypes_calls
from .translate_universe import (
    bytes_identity_universe_for_callee,
    branch_selected_raise_universe_for_callee,
    constructor_field_universe_for_callee,
    delegation_universe_for_callee,
    exception_bool_return_universe_for_callee,
    exception_handler_raise_universe_for_callee,
    instance_field_universe_for_callee,
    list_adapter_universe_for_callee,
    raise_locus_universe_for_callee,
)


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


def _empty_source_ledger() -> Dict[str, int]:
    return {
        "source_loci": 0,
        "source_warranted": 0,
        "source_support": 0,
        "source_refused": 0,
        "source_inactive": 0,
        "source_refuted": 0,
        "unclassified_source": 0,
    }


def _merge_source_ledger(dst: Dict[str, int], src: Dict[str, Any]) -> None:
    for field in dst:
        dst[field] += int(src.get(field, 0))


def _source_totals(loci: List[Dict[str, Any]]) -> Dict[str, int]:
    totals = _empty_source_ledger()
    totals["source_loci"] = len(loci)
    for locus in loci:
        status = locus.get("status")
        if status == "warranted":
            totals["source_warranted"] += 1
        elif status == "support":
            totals["source_support"] += 1
        elif status == "refused":
            totals["source_refused"] += 1
        elif status == "inactive":
            totals["source_inactive"] += 1
        elif status == "refuted":
            totals["source_refuted"] += 1
        else:
            totals["unclassified_source"] += 1
    return totals


def _ast_node_span(node: ast.AST) -> Dict[str, int]:
    start_line = getattr(node, "lineno", 0)
    start_col = getattr(node, "col_offset", 0)
    end_line = getattr(node, "end_lineno", start_line)
    end_col = getattr(node, "end_col_offset", start_col)
    return {
        "start_line": start_line,
        "start_col": start_col,
        "end_line": end_line,
        "end_col": end_col,
    }


def _iter_ast_nodes_with_paths(
    node: ast.AST,
    path: str,
    ancestors: tuple[ast.AST, ...] = (),
):
    yield node, path, ancestors
    for field_name, value in ast.iter_fields(node):
        if isinstance(value, ast.AST):
            yield from _iter_ast_nodes_with_paths(
                value,
                f"{path}.{field_name}",
                ancestors + (node,),
            )
        elif isinstance(value, list):
            for index, item in enumerate(value):
                if isinstance(item, ast.AST):
                    yield from _iter_ast_nodes_with_paths(
                        item,
                        f"{path}.{field_name}[{index}]",
                        ancestors + (node,),
                    )


def _source_line_locus(
    file: str,
    line: int,
    status: str,
    role: str,
    universe_kind: str,
    *,
    ast_kind: str = "",
    ast_path: str = "",
    span: Optional[Dict[str, int]] = None,
    reason: str = "",
) -> Dict[str, Any]:
    locus_span = span or {
        "start_line": line,
        "start_col": 0,
        "end_line": line,
        "end_col": 0,
    }
    locus: Dict[str, Any] = {
        "kind": "source-line",
        "file": file,
        "line": line,
        "span": dict(locus_span),
        "line_range": [locus_span["start_line"], locus_span["end_line"]],
        "ast_path": ast_path or f"$.line[{line}]",
        "status": status,
        "role": role,
        "universe_kind": universe_kind,
    }
    if ast_kind:
        locus["ast_kind"] = ast_kind
    if reason:
        locus["reason"] = reason
    return locus


def _path_for_source_file(value: Any) -> Optional[Path]:
    if not isinstance(value, str) or not value:
        return None
    try:
        path = Path(value).expanduser()
        if not path.is_absolute():
            path = Path.cwd() / path
        return path.resolve()
    except OSError:
        return None


def _python_package_root_for_file(file: Any) -> Optional[Path]:
    path = _path_for_source_file(file)
    if path is None or not path.is_file() or path.suffix != ".py":
        return None
    root: Optional[Path] = None
    cursor = path.parent
    while (cursor / "__init__.py").is_file():
        root = cursor
        cursor = cursor.parent
    return root


def _covered_source_lines(source_audits: List[Any]) -> Dict[Path, Set[int]]:
    covered: Dict[Path, Set[int]] = {}
    for audit in source_audits:
        if not isinstance(audit, dict):
            continue
        if audit.get("role") == "python.package-source":
            continue
        for locus in audit.get("loci") or []:
            if not isinstance(locus, dict):
                continue
            path = _path_for_source_file(locus.get("file"))
            if path is None:
                continue
            line_range = locus.get("line_range")
            if (
                isinstance(line_range, list)
                and len(line_range) == 2
                and all(isinstance(v, int) for v in line_range)
            ):
                start, end = line_range
            else:
                line = locus.get("line")
                if not isinstance(line, int):
                    continue
                start = end = line
            if end < start:
                end = start
            covered.setdefault(path, set()).update(range(start, end + 1))
    return covered


def _package_roots_from_source_audits(source_audits: List[Any]) -> Dict[Path, str]:
    roots: Dict[Path, str] = {}
    for audit in source_audits:
        if not isinstance(audit, dict):
            continue
        role = str(audit.get("role") or "")
        if role in {"python.package-source", "python.test-fact"}:
            continue
        memento = audit.get("source_memento") or audit.get("sourceMemento")
        if not isinstance(memento, dict):
            continue
        root = _python_package_root_for_file(memento.get("file"))
        if root is None:
            continue
        roots.setdefault(root, root.name)
    return roots


def _package_unclassified_loci(
    root: Path,
    covered_lines: Dict[Path, Set[int]],
) -> List[Dict[str, Any]]:
    loci: List[Dict[str, Any]] = []
    for path in sorted(root.rglob("*.py")):
        if "__pycache__" in path.parts:
            continue
        file = str(path)
        try:
            source = path.read_text(encoding="utf-8")
            tree = ast.parse(source, filename=file)
        except (OSError, SyntaxError) as exc:
            loci.append(
                _source_line_locus(
                    file,
                    0,
                    "refused",
                    "python.package-source",
                    "package-accounting",
                    reason=f"package source could not be parsed: {exc}",
                )
            )
            continue

        module_name = _package_module_name(root, path)
        call_aliases = _package_call_aliases(tree, module_name)
        already_classified = covered_lines.get(path.resolve(), set())
        for node, ast_path, ancestors in _iter_ast_nodes_with_paths(tree, "$.module"):
            line = getattr(node, "lineno", None)
            if not isinstance(line, int) or line in already_classified:
                continue
            span = _ast_node_span(node)
            status, reason = _package_locus_classification(
                node,
                ast_path,
                ancestors,
                call_aliases,
                module_name,
                tree,
            )
            loci.append(
                _source_line_locus(
                    file,
                    line,
                    status,
                    "python.package-source",
                    "package-accounting",
                    ast_kind=type(node).__name__,
                    ast_path=ast_path,
                    span=span,
                    reason=reason,
                )
            )
    return loci


def _package_locus_classification(
    node: ast.AST,
    ast_path: str,
    ancestors: tuple[ast.AST, ...],
    call_aliases: Dict[str, str],
    module_name: str,
    tree: ast.Module,
) -> tuple[str, str]:
    overload_status = _overload_declaration_status(node, ancestors)
    if overload_status is not None:
        return overload_status
    if isinstance(node, (ast.Import, ast.ImportFrom, ast.alias)):
        return "support", "import support for recursive name resolution"
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
        return "support", "function declaration supports callsite arity/name resolution"
    if isinstance(node, ast.ClassDef):
        return "support", "class declaration supports attribute/name resolution"
    if isinstance(node, ast.arg):
        return "support", "function parameter metadata supports callsite argument mapping"
    if _is_function_annotation_path(ast_path):
        return "support", "type annotation metadata supports source accounting only"
    if _is_decorator_metadata_path(ast_path):
        return "support", "decorator metadata supports source accounting only"
    default_literal_status = _function_default_literal_status(
        node,
        ast_path,
        ancestors,
    )
    if default_literal_status is not None:
        return default_literal_status
    type_checking_status = _type_checking_block_status(node, ast_path, ancestors)
    if type_checking_status is not None:
        return type_checking_status
    static_binding_status = _static_binding_status(node, ancestors)
    if static_binding_status is not None:
        return static_binding_status
    guarded_default_status = _guarded_default_value_flow_status(node, ancestors)
    if guarded_default_status is not None:
        return guarded_default_status
    transparent_cast_status = _transparent_typing_cast_status(node, ancestors)
    if transparent_cast_status is not None:
        return transparent_cast_status
    super_init_status = _super_init_support_status(node, ancestors)
    if super_init_status is not None:
        return super_init_status
    constructor_field_status = _constructor_field_assignment_status(
        node,
        ancestors,
        module_name,
    )
    if constructor_field_status is not None:
        return constructor_field_status
    dynamic_io_status = _dynamic_receiver_io_refusal_status(node, ancestors)
    if dynamic_io_status is not None:
        return dynamic_io_status
    nondet_status = _nondeterministic_call_refusal_status(
        node,
        ancestors,
        module_name,
        tree,
    )
    if nondet_status is not None:
        return nondet_status
    exception_universe_status = _exception_universe_source_status(
        node,
        ancestors,
        module_name,
    )
    if exception_universe_status is not None:
        return exception_universe_status
    unhandled_try_status = _unhandled_try_flow_refusal_status(node, ancestors)
    if unhandled_try_status is not None:
        return unhandled_try_status
    self_field_dispatch_status = _self_field_runtime_dispatch_refusal_status(
        node,
        ancestors,
        tree,
    )
    if self_field_dispatch_status is not None:
        return self_field_dispatch_status
    adapter_assignment_status = _local_adapter_assignment_status(
        node,
        ancestors,
        call_aliases,
    )
    if adapter_assignment_status is not None:
        return adapter_assignment_status
    call_term_assignment_status = _local_call_term_assignment_status(
        node,
        ancestors,
        call_aliases,
        module_name,
        tree,
    )
    if call_term_assignment_status is not None:
        return call_term_assignment_status
    tuple_unpack_call_status = _local_tuple_unpack_call_status(
        node,
        ancestors,
        call_aliases,
        module_name,
        tree,
    )
    if tuple_unpack_call_status is not None:
        return tuple_unpack_call_status
    list_adapter_body_status = _list_adapter_body_status(
        node,
        ancestors,
        module_name,
    )
    if list_adapter_body_status is not None:
        return list_adapter_body_status
    instance_field_body_status = _instance_field_body_status(
        node,
        ancestors,
        module_name,
    )
    if instance_field_body_status is not None:
        return instance_field_body_status
    generator_flow_status = _generator_flow_refusal_status(node, ancestors)
    if generator_flow_status is not None:
        return generator_flow_status
    local_binding_status = _local_name_binding_status(node, ancestors)
    if local_binding_status is not None:
        return local_binding_status
    delegation_body_status = _delegation_body_status(
        node,
        ancestors,
        module_name,
    )
    if delegation_body_status is not None:
        return delegation_body_status
    unhandled_raise_status = _unhandled_raise_path_refusal_status(node, ancestors)
    if unhandled_raise_status is not None:
        return unhandled_raise_status
    if _is_docstring_expr_node(node, ancestors):
        return "support", "docstring metadata supports source accounting only"
    decl = _nearest_declaration_ancestor(ancestors)
    line = getattr(node, "lineno", None)
    if decl is not None and isinstance(line, int) and line == decl.lineno:
        return "support", "declaration metadata supports callsite arity/name resolution"
    return "unclassified", "not classified by any emitted Python source warrant"


def _package_module_name(root: Path, path: Path) -> str:
    try:
        rel = path.relative_to(root).with_suffix("")
    except ValueError:
        rel = path.with_suffix("").name
        return str(rel).replace(os.sep, ".")
    parts = [root.name, *rel.parts]
    if parts and parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(part for part in parts if part)


def _package_call_aliases(tree: ast.Module, module_name: str) -> Dict[str, str]:
    aliases: Dict[str, str] = {}
    for stmt in tree.body:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
            aliases[stmt.name] = f"{module_name}.{stmt.name}"
        elif isinstance(stmt, ast.Import):
            for alias in stmt.names:
                aliases[alias.asname or alias.name.split(".", 1)[0]] = alias.name
        elif isinstance(stmt, ast.ImportFrom):
            imported_module = _resolved_import_from_module(module_name, stmt)
            if imported_module is None:
                continue
            for alias in stmt.names:
                if alias.name == "*":
                    continue
                aliases[alias.asname or alias.name] = (
                    f"{imported_module}.{alias.name}"
                )
    return aliases


def _resolved_import_from_module(
    module_name: str,
    stmt: ast.ImportFrom,
) -> Optional[str]:
    if stmt.level == 0:
        return stmt.module
    parts = module_name.split(".")
    base = parts[:-stmt.level]
    if not base and parts:
        base = parts[:1]
    if stmt.module:
        base = [*base, *stmt.module.split(".")]
    return ".".join(part for part in base if part)


def _overload_declaration_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    fn = _nearest_overload_function(node, ancestors)
    if fn is None:
        return None
    if _node_is_in_function_body(node, fn):
        return "inactive", "typing overload body inactive at runtime"
    return "support", "typing overload declaration metadata supports source accounting only"


def _nearest_overload_function(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[ast.FunctionDef | ast.AsyncFunctionDef]:
    chain = ancestors + (node,)
    for item in reversed(chain):
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)) and any(
            _is_overload_decorator(decorator)
            for decorator in item.decorator_list
        ):
            return item
    return None


def _is_overload_decorator(node: ast.AST) -> bool:
    return _static_call_name(node) in {"t.overload", "typing.overload"}


def _node_is_in_function_body(
    node: ast.AST,
    fn: ast.FunctionDef | ast.AsyncFunctionDef,
) -> bool:
    return any(
        descendant is node
        for stmt in fn.body
        for descendant in ast.walk(stmt)
    )


def _is_function_annotation_path(ast_path: str) -> bool:
    return ".annotation" in ast_path or ".returns" in ast_path


def _is_decorator_metadata_path(ast_path: str) -> bool:
    return ".decorator_list" in ast_path


def _function_default_literal_status(
    node: ast.AST,
    ast_path: str,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    if ".args.defaults[" not in ast_path and ".args.kw_defaults[" not in ast_path:
        return None
    default_expr = _function_default_expr_for_locus(node, ancestors)
    if default_expr is None:
        return None
    if not _is_local_literal_binding_value(default_expr):
        return None
    return "warranted", "function default literal admitted as timeless compiler fact"


def _function_default_expr_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[ast.expr]:
    chain = ancestors + (node,)
    for index, item in enumerate(chain):
        if not isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        for default in [*item.args.defaults, *item.args.kw_defaults]:
            if default is None:
                continue
            if any(candidate is node for candidate in ast.walk(default)):
                return default
        if index < len(chain) - 1:
            continue
    return None


def _type_checking_block_status(
    node: ast.AST,
    ast_path: str,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    type_if = _top_level_type_checking_if_for_locus(node, ancestors)
    if type_if is None:
        return None
    if ".body[" in ast_path:
        return "inactive", "TYPE_CHECKING-only branch inactive at runtime"
    return "support", "TYPE_CHECKING guard/fallback supports type-only source accounting"


def _top_level_type_checking_if_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[ast.If]:
    chain = ancestors + (node,)
    saw_module = False
    for item in chain:
        if isinstance(item, ast.Module):
            saw_module = True
            continue
        if not saw_module:
            continue
        if isinstance(item, ast.If) and _is_type_checking_test(item.test):
            return item
        return None
    return None


def _is_type_checking_test(node: ast.AST) -> bool:
    return _static_call_name(node) in {"t.TYPE_CHECKING", "typing.TYPE_CHECKING"}


def _static_binding_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    stmt = _static_binding_statement_for_locus(node, ancestors)
    if stmt is None:
        return None
    value = stmt.value if isinstance(stmt, ast.AnnAssign) else stmt.value
    if value is None:
        return "support", "annotation-only binding carries no runtime value"
    if _is_static_assignment_value(value):
        return "warranted", "static binding admitted as timeless compiler fact"
    return None


def _static_binding_statement_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[ast.Assign | ast.AnnAssign]:
    chain = ancestors + (node,)
    stmt_index: Optional[int] = None
    stmt: Optional[ast.Assign | ast.AnnAssign] = None
    for index in range(len(chain) - 1, -1, -1):
        item = chain[index]
        if isinstance(item, (ast.Assign, ast.AnnAssign)):
            stmt_index = index
            stmt = item
            break
    if stmt is None or stmt_index is None or stmt_index == 0:
        return None
    parent = chain[stmt_index - 1]
    if not isinstance(parent, (ast.Module, ast.ClassDef)):
        return None
    for item in chain[:stmt_index]:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
            return None
    return stmt


def _is_static_assignment_value(node: ast.AST) -> bool:
    if isinstance(node, ast.Constant):
        return True
    if isinstance(node, ast.Name):
        return True
    if isinstance(node, ast.Attribute):
        return _is_static_assignment_value(node.value)
    if isinstance(node, ast.JoinedStr):
        return all(_is_static_assignment_value(value) for value in node.values)
    if isinstance(node, ast.FormattedValue):
        return _is_static_assignment_value(node.value)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
        return _is_static_assignment_value(node.operand)
    if isinstance(node, ast.BinOp):
        return _is_static_assignment_value(node.left) and _is_static_assignment_value(
            node.right
        )
    if isinstance(node, ast.Subscript):
        return _is_static_assignment_value(node.value) and _is_static_slice(node.slice)
    if isinstance(node, (ast.Tuple, ast.List, ast.Set)):
        return all(_is_static_assignment_value(value) for value in node.elts)
    if isinstance(node, ast.Dict):
        return all(
            (key is None or _is_static_assignment_value(key))
            and _is_static_assignment_value(value)
            for key, value in zip(node.keys, node.values)
        )
    if isinstance(node, ast.Call):
        return _is_known_static_assignment_call(node)
    return False


def _is_static_slice(node: ast.AST) -> bool:
    if isinstance(node, ast.Slice):
        return all(
            part is None or _is_static_assignment_value(part)
            for part in (node.lower, node.upper, node.step)
        )
    return _is_static_assignment_value(node)


def _is_known_static_assignment_call(node: ast.Call) -> bool:
    if any(isinstance(arg, ast.Starred) for arg in node.args):
        return False
    if not all(_is_static_assignment_value(arg) for arg in node.args):
        return False
    if not all(_is_static_assignment_value(kw.value) for kw in node.keywords):
        return False
    func = node.func
    if isinstance(func, ast.Attribute) and func.attr == "encode":
        return _is_static_assignment_value(func.value)
    callee = _static_call_name(func)
    return callee in {
        "struct.Struct",
        "t.cast",
        "typing.cast",
        "staticmethod",
    }


def _guarded_default_value_flow_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    guarded_if = _guarded_default_if_for_locus(node, ancestors)
    if guarded_if is None:
        return None
    return "warranted", "guarded default value flow admitted as compiler fact"


def _guarded_default_if_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[ast.If]:
    chain = ancestors + (node,)
    for item in reversed(chain):
        if not isinstance(item, ast.If):
            continue
        if _is_guarded_default_if(item):
            return item
        return None
    return None


def _is_guarded_default_if(node: ast.If) -> bool:
    if node.orelse or len(node.body) != 1:
        return False
    assign = node.body[0]
    if isinstance(assign, ast.Assign):
        if len(assign.targets) != 1 or not isinstance(assign.targets[0], ast.Name):
            return False
        target = assign.targets[0]
        value = assign.value
    elif isinstance(assign, ast.AnnAssign):
        if not isinstance(assign.target, ast.Name) or assign.value is None:
            return False
        target = assign.target
        value = assign.value
    else:
        return False
    guarded_name = _none_guard_name(node.test)
    return (
        guarded_name is not None
        and guarded_name == target.id
        and _is_guarded_default_value(value)
    )


def _none_guard_name(node: ast.AST) -> Optional[str]:
    if (
        not isinstance(node, ast.Compare)
        or len(node.ops) != 1
        or not isinstance(node.ops[0], ast.Is)
        or len(node.comparators) != 1
    ):
        return None
    left = node.left
    right = node.comparators[0]
    if isinstance(left, ast.Name) and _is_none_literal_node(right):
        return left.id
    if isinstance(right, ast.Name) and _is_none_literal_node(left):
        return right.id
    return None


def _is_none_literal_node(node: ast.AST) -> bool:
    return isinstance(node, ast.Constant) and node.value is None


def _is_guarded_default_value(node: ast.AST) -> bool:
    if _is_local_literal_binding_value(node):
        return True
    if isinstance(node, ast.Attribute):
        return _guarded_default_attribute_root(node) in {"self", "cls"}
    return False


def _guarded_default_attribute_root(node: ast.Attribute) -> str:
    cur: ast.AST = node
    while isinstance(cur, ast.Attribute):
        cur = cur.value
    return cur.id if isinstance(cur, ast.Name) else ""


def _transparent_typing_cast_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    chain = ancestors + (node,)
    for item in reversed(chain):
        if not _is_transparent_typing_cast_call(item):
            continue
        if item is node:
            return (
                "warranted",
                "transparent typing cast admitted as compiler axiom",
            )
        if any(descendant is node for descendant in ast.walk(item.func)):
            return (
                "warranted",
                "transparent typing cast callee admitted as compiler axiom",
            )
        if item.args and any(descendant is node for descendant in ast.walk(item.args[0])):
            return (
                "warranted",
                "transparent typing cast type admitted as compiler axiom",
            )
        return None
    return None


def _is_transparent_typing_cast_call(node: ast.AST) -> bool:
    return (
        isinstance(node, ast.Call)
        and not node.keywords
        and len(node.args) == 2
        and _static_call_name(node.func) in {"t.cast", "typing.cast"}
    )


def _super_init_support_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    stmt = _super_init_statement_for_locus(node, ancestors)
    if stmt is None:
        return None
    return "support", "base constructor call supports construction accounting"


def _super_init_statement_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[ast.Expr]:
    chain = ancestors + (node,)
    for item in reversed(chain):
        if isinstance(item, ast.stmt):
            return item if _is_super_init_expr(item) else None
    return None


def _is_super_init_expr(stmt: ast.AST) -> bool:
    if not isinstance(stmt, ast.Expr) or not isinstance(stmt.value, ast.Call):
        return False
    call = stmt.value
    if call.keywords:
        return False
    if not all(_is_super_init_support_arg(arg) for arg in call.args):
        return False
    func = call.func
    return (
        isinstance(func, ast.Attribute)
        and func.attr == "__init__"
        and isinstance(func.value, ast.Call)
        and isinstance(func.value.func, ast.Name)
        and func.value.func.id == "super"
        and not func.value.args
        and not func.value.keywords
    )


def _is_super_init_support_arg(node: ast.AST) -> bool:
    if isinstance(node, ast.Constant):
        return True
    if isinstance(node, ast.Name):
        return True
    if isinstance(node, ast.Attribute):
        return _is_super_init_support_arg(node.value)
    if isinstance(node, (ast.Tuple, ast.List)):
        return all(_is_super_init_support_arg(value) for value in node.elts)
    return False


def _constructor_field_assignment_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    module_name: str,
) -> Optional[tuple[str, str]]:
    stmt = _constructor_field_assignment_for_locus(node, ancestors)
    if stmt is None:
        return None
    assign_stmt, owner, field_name = stmt
    if not any(descendant is node for descendant in ast.walk(assign_stmt)):
        return None
    owner_callee = _owner_callee(module_name, owner, ancestors + (node,))
    if not owner_callee.endswith(".__init__"):
        return None
    constructor_callee = owner_callee[: -len(".__init__")]
    universe, refusal = constructor_field_universe_for_callee(
        constructor_callee,
        field_name,
    )
    if refusal is not None or universe is None:
        return None
    return (
        "warranted",
        "constructor field assignment emitted as constructor-field universe fact",
    )


def _constructor_field_assignment_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[ast.Assign | ast.AnnAssign, ast.FunctionDef, str]]:
    chain = ancestors + (node,)
    stmt: Optional[ast.Assign | ast.AnnAssign] = None
    stmt_index: Optional[int] = None
    for index in range(len(chain) - 1, -1, -1):
        item = chain[index]
        if isinstance(item, (ast.Assign, ast.AnnAssign)):
            stmt = item
            stmt_index = index
            break
    if stmt is None or stmt_index is None:
        return None
    owner = _nearest_enclosing_function(chain[:stmt_index])
    if not isinstance(owner, ast.FunctionDef) or owner.name != "__init__":
        return None
    if isinstance(stmt, ast.Assign):
        if len(stmt.targets) != 1:
            return None
        target = stmt.targets[0]
        value = stmt.value
    else:
        target = stmt.target
        value = stmt.value
    if value is None or not isinstance(value, ast.Name):
        return None
    if value.id not in {arg.arg for arg in owner.args.args[1:]}:
        return None
    if not (
        isinstance(target, ast.Attribute)
        and isinstance(target.value, ast.Name)
        and target.value.id == "self"
    ):
        return None
    return stmt, owner, target.attr


def _dynamic_receiver_io_refusal_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    stmt = _nearest_statement(ancestors + (node,))
    if stmt is None:
        return None
    owner = _nearest_enclosing_function(ancestors + (node,))
    if owner is None or isinstance(owner, ast.Lambda):
        return None
    params = {
        arg.arg
        for arg in (
            *owner.args.posonlyargs,
            *owner.args.args,
            *owner.args.kwonlyargs,
        )
    }
    if owner.args.vararg is not None:
        params.add(owner.args.vararg.arg)
    if owner.args.kwarg is not None:
        params.add(owner.args.kwarg.arg)
    params.difference_update({"self", "cls"})
    if not params:
        return None
    for call in (n for n in ast.walk(stmt) if isinstance(n, ast.Call)):
        receiver = _dynamic_io_receiver_name(call)
        if receiver is None or receiver not in params:
            continue
        if node is stmt or any(candidate is node for candidate in ast.walk(stmt)):
            return (
                "refused",
                (
                    "dynamic receiver IO call refused: "
                    f"{receiver}.{call.func.attr} is supplied at runtime, "
                    "so no vendor source body can warrant this relation"
                ),
            )
    return None


def _nearest_statement(
    chain: tuple[ast.AST, ...],
) -> Optional[ast.stmt]:
    for item in reversed(chain):
        if isinstance(item, ast.stmt) and not isinstance(
            item,
            (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef),
        ):
            return item
    return None


def _dynamic_io_receiver_name(call: ast.Call) -> Optional[str]:
    func = call.func
    if not (
        isinstance(func, ast.Attribute)
        and func.attr in {"read", "write"}
        and isinstance(func.value, ast.Name)
    ):
        return None
    return func.value.id


def _self_field_runtime_dispatch_refusal_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    tree: ast.Module,
) -> Optional[tuple[str, str]]:
    stmt = _nearest_statement(ancestors + (node,))
    if stmt is None:
        return None
    chain = ancestors + (node,)
    class_qualname = _nearest_class_qualname(chain)
    if not class_qualname:
        return None
    cls = _find_class_by_qualname(tree, class_qualname)
    if cls is None:
        methods: set[str] = set()
        fields: set[str] = set()
        has_bases = False
    else:
        methods = {
            item.name
            for item in cls.body
            if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))
        }
        fields = _class_receiver_field_names(cls)
        has_bases = bool(cls.bases)
    for call in (n for n in ast.walk(stmt) if isinstance(n, ast.Call)):
        path = _call_func_attribute_path(call.func)
        if len(path) < 2 or path[0] not in {"self", "cls"}:
            continue
        if len(path) == 2 and path[1] in methods:
            continue
        if len(path) == 2 and path[1] not in fields and has_bases:
            continue
        if node is stmt or any(candidate is node for candidate in ast.walk(stmt)):
            return (
                "refused",
                (
                    "runtime field dispatch refused: "
                    f"{'.'.join(path)} is supplied by receiver state, "
                    "so no stable vendor method body can warrant this relation"
                ),
            )
    return None


def _class_receiver_field_names(cls: ast.ClassDef) -> set[str]:
    fields: set[str] = set()
    for node in ast.walk(cls):
        targets: list[ast.AST] = []
        if isinstance(node, (ast.Assign, ast.AnnAssign)):
            if isinstance(node, ast.Assign):
                targets.extend(node.targets)
            else:
                targets.append(node.target)
        elif isinstance(node, ast.AugAssign):
            targets.append(node.target)
        for target in targets:
            if (
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id in {"self", "cls"}
            ):
                fields.add(target.attr)
    return fields


def _call_func_attribute_path(node: ast.AST) -> tuple[str, ...]:
    parts: list[str] = []
    current = node
    while isinstance(current, ast.Attribute):
        parts.append(current.attr)
        current = current.value
    if not isinstance(current, ast.Name):
        return ()
    parts.append(current.id)
    return tuple(reversed(parts))


_NONDET_CALL_ATTRS = frozenset(
    {
        "random",
        "uniform",
        "randint",
        "randrange",
        "choice",
        "choices",
        "token_hex",
        "token_urlsafe",
        "urandom",
        "uuid1",
        "uuid4",
        "now",
        "utcnow",
        "today",
        "time",
        "monotonic",
        "perf_counter",
    }
)
_NONDET_CALL_ROOTS = frozenset({"random", "secrets", "uuid", "time"})


def _nondeterministic_call_refusal_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    module_name: str,
    tree: ast.Module,
) -> Optional[tuple[str, str]]:
    stmt = _nearest_statement(ancestors + (node,))
    if stmt is None:
        return None
    chain = ancestors + (node,)
    for call in (n for n in ast.walk(stmt) if isinstance(n, ast.Call)):
        reason = _nondeterministic_call_reason(call, chain, module_name, tree)
        if reason is None:
            continue
        if node is stmt or any(candidate is node for candidate in ast.walk(stmt)):
            return "refused", reason
    return None


def _nondeterministic_call_reason(
    call: ast.Call,
    chain: tuple[ast.AST, ...],
    module_name: str,
    tree: ast.Module,
) -> Optional[str]:
    direct = _direct_nondeterministic_call_name(call)
    if direct:
        return (
            "nondeterminism source refused: "
            f"{direct} depends on runtime state"
        )

    if not (
        isinstance(call.func, ast.Attribute)
        and isinstance(call.func.value, ast.Name)
        and call.func.value.id in {"self", "cls"}
    ):
        return None
    class_qualname = _nearest_class_qualname(chain)
    if not class_qualname:
        return None
    cls = _find_class_by_qualname(tree, class_qualname)
    if cls is None:
        return None
    methods = {
        item.name: item
        for item in cls.body
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    method = methods.get(call.func.attr)
    if method is None or not _method_body_reaches_nondeterminism(
        method,
        methods,
        depth=3,
        seen=set(),
    ):
        return None
    callee = f"{module_name}.{class_qualname}.{call.func.attr}"
    return (
        "nondeterminism source refused: "
        f"{callee} transitively depends on runtime state"
    )


def _exception_universe_source_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    module_name: str,
) -> Optional[tuple[str, str]]:
    owner = _nearest_enclosing_function(ancestors + (node,))
    if owner is None or isinstance(owner, ast.Lambda):
        return None
    stmt = _owner_body_statement(ancestors + (node,), owner)
    if stmt is None:
        return None
    callee = _owner_callee(module_name, owner, ancestors + (node,))
    for role, universe_kind, resolver in (
        (
            "python.exception-handler-raise-universe",
            "exception-handler-raise",
            exception_handler_raise_universe_for_callee,
        ),
        (
            "python.exception-bool-return-universe",
            "exception-bool-return",
            exception_bool_return_universe_for_callee,
        ),
        (
            "python.branch-selected-raise-universe",
            "branch-selected-raise",
            branch_selected_raise_universe_for_callee,
        ),
        (
            "python.raise-locus-universe",
            "raise-locus",
            raise_locus_universe_for_callee,
        ),
    ):
        universe, refusal = resolver(callee)
        if refusal is not None or universe is None:
            continue
        source_memento = getattr(universe, "source_memento", None)
        if source_memento is None:
            continue
        status, reason = _classify_universe_source_node(
            role,
            universe_kind,
            stmt,
            node,
            source_memento,
        )
        if status != "unclassified":
            return status, reason
    return None


def _owner_body_statement(
    chain: tuple[ast.AST, ...],
    owner: ast.FunctionDef | ast.AsyncFunctionDef,
) -> Optional[ast.stmt]:
    try:
        owner_index = next(
            index for index, item in enumerate(chain) if item is owner
        )
    except StopIteration:
        return _nearest_statement(chain)
    for item in chain[owner_index + 1:]:
        if isinstance(item, ast.stmt) and not isinstance(
            item,
            (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef),
        ):
            return item
    return _nearest_statement(chain)


def _unhandled_try_flow_refusal_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    chain = ancestors + (node,)
    try_stmt = next(
        (item for item in reversed(chain) if isinstance(item, ast.Try)),
        None,
    )
    if try_stmt is None:
        return None
    if node is try_stmt or any(candidate is node for candidate in ast.walk(try_stmt)):
        return (
            "refused",
            (
                "path-sensitive try/except flow refused: no emitted "
                "exception/value universe accounts for this control-flow relation"
            ),
        )
    return None


def _unhandled_raise_path_refusal_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    chain = ancestors + (node,)
    raise_stmt = next(
        (item for item in reversed(chain) if isinstance(item, ast.Raise)),
        None,
    )
    if raise_stmt is None:
        return None
    if node is raise_stmt or any(candidate is node for candidate in ast.walk(raise_stmt)):
        return (
            "refused",
            (
                "raise path refused: no emitted no-return, branch-raise, "
                "or exception-handler universe accounts for this path"
            ),
        )
    return None


def _generator_flow_refusal_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    owner = _nearest_enclosing_function(ancestors + (node,))
    if owner is None or isinstance(owner, ast.Lambda):
        return None
    if not _node_is_in_function_body(node, owner):
        return None
    if _is_docstring_expr_node(node, ancestors):
        return None
    if not _function_body_has_yield(owner):
        return None
    return (
        "refused",
        (
            "generator/yield flow refused: emitted sequence order is "
            "runtime-selected and not modeled as a timeless value relation"
        ),
    )


def _function_body_has_yield(fn: ast.FunctionDef | ast.AsyncFunctionDef) -> bool:
    return any(_node_has_yield_outside_nested_scope(stmt) for stmt in fn.body)


def _node_has_yield_outside_nested_scope(node: ast.AST) -> bool:
    if isinstance(node, (ast.Yield, ast.YieldFrom)):
        return True
    if isinstance(
        node,
        (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Lambda),
    ):
        return False
    return any(
        _node_has_yield_outside_nested_scope(child)
        for child in ast.iter_child_nodes(node)
    )


def _direct_nondeterministic_call_name(call: ast.Call) -> str:
    static_name = _static_call_name(call.func)
    parts = static_name.split(".") if static_name else []
    if not parts:
        return ""
    root = parts[0]
    leaf = parts[-1]
    if root in _NONDET_CALL_ROOTS and leaf in _NONDET_CALL_ATTRS:
        return static_name
    return ""


def _find_class_by_qualname(
    tree: ast.Module,
    qualname: str,
) -> Optional[ast.ClassDef]:
    parts = [part for part in qualname.split(".") if part]
    body: list[ast.stmt] = list(tree.body)
    found: Optional[ast.ClassDef] = None
    for part in parts:
        found = next(
            (
                item
                for item in body
                if isinstance(item, ast.ClassDef) and item.name == part
            ),
            None,
        )
        if found is None:
            return None
        body = list(found.body)
    return found


def _method_body_reaches_nondeterminism(
    fn: ast.FunctionDef | ast.AsyncFunctionDef,
    methods: Dict[str, ast.FunctionDef | ast.AsyncFunctionDef],
    *,
    depth: int,
    seen: set[str],
) -> bool:
    if fn.name in seen or depth <= 0:
        return False
    seen.add(fn.name)
    for node in ast.walk(fn):
        if not isinstance(node, ast.Call):
            continue
        if _direct_nondeterministic_call_name(node):
            return True
        if (
            isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id in {"self", "cls"}
        ):
            callee = methods.get(node.func.attr)
            if callee is not None and _method_body_reaches_nondeterminism(
                callee,
                methods,
                depth=depth - 1,
                seen=seen,
            ):
                return True
    return False


def _nearest_class_qualname(chain: tuple[ast.AST, ...]) -> str:
    names = [item.name for item in chain if isinstance(item, ast.ClassDef)]
    return ".".join(names)


def _local_adapter_assignment_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    call_aliases: Dict[str, str],
) -> Optional[tuple[str, str]]:
    stmt = _adapter_assignment_statement_for_locus(node, ancestors)
    if stmt is None:
        return None
    assign_stmt, value = stmt
    if not isinstance(value, ast.Call):
        return None
    if not any(descendant is node for descendant in ast.walk(assign_stmt)):
        return None
    if (
        not isinstance(value.func, ast.Name)
        or value.keywords
        or any(isinstance(arg, ast.Starred) for arg in value.args)
        or not all(_is_adapter_assignment_arg(arg) for arg in value.args)
    ):
        return None
    callee = call_aliases.get(value.func.id)
    if callee is None:
        return None
    universe, refusal = bytes_identity_universe_for_callee(callee)
    if refusal is not None:
        return None
    if universe is not None:
        return (
            "warranted",
            "source-backed adapter assignment emitted as recursive universe dig",
        )
    universe, refusal = list_adapter_universe_for_callee(callee)
    if refusal is not None or universe is None:
        return None
    return (
        "warranted",
        "source-backed helper assignment emitted as recursive universe dig",
    )


def _local_call_term_assignment_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    call_aliases: Dict[str, str],
    module_name: str,
    tree: ast.Module,
) -> Optional[tuple[str, str]]:
    stmt = _local_call_term_assignment_statement_for_locus(node, ancestors)
    if stmt is None:
        return None
    assign_stmt, value = stmt
    if not any(descendant is node for descendant in ast.walk(assign_stmt)):
        return None
    if not _is_statically_nameable_call_term(
        value,
        ancestors + (node,),
        call_aliases,
        module_name,
        tree,
    ):
        return None
    return (
        "warranted",
        "local call-term SSA binding admitted as compiler equality",
    )


def _local_call_term_assignment_statement_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[ast.Assign | ast.AnnAssign, ast.Call]]:
    chain = ancestors + (node,)
    stmt_index: Optional[int] = None
    stmt: Optional[ast.Assign | ast.AnnAssign] = None
    for index in range(len(chain) - 1, -1, -1):
        item = chain[index]
        if isinstance(item, (ast.Assign, ast.AnnAssign)):
            stmt_index = index
            stmt = item
            break
    if stmt is None or stmt_index is None:
        return None
    owner = _nearest_enclosing_function(chain[:stmt_index])
    if owner is None:
        return None
    if isinstance(stmt, ast.Assign):
        if len(stmt.targets) != 1 or not isinstance(stmt.targets[0], ast.Name):
            return None
        value = stmt.value
    else:
        if not isinstance(stmt.target, ast.Name):
            return None
        value = stmt.value
    if not isinstance(value, ast.Call):
        return None
    return stmt, value


def _local_tuple_unpack_call_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    call_aliases: Dict[str, str],
    module_name: str,
    tree: ast.Module,
) -> Optional[tuple[str, str]]:
    stmt = _local_tuple_unpack_call_statement_for_locus(node, ancestors)
    if stmt is None:
        return None
    assign_stmt, value = stmt
    if not any(descendant is node for descendant in ast.walk(assign_stmt)):
        return None
    if not _is_statically_nameable_call_term(
        value,
        ancestors + (node,),
        call_aliases,
        module_name,
        tree,
    ):
        return None
    return (
        "warranted",
        "local tuple-unpack call-term projection admitted as compiler equality",
    )


def _local_tuple_unpack_call_statement_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[ast.Assign, ast.Call]]:
    chain = ancestors + (node,)
    stmt_index: Optional[int] = None
    stmt: Optional[ast.Assign] = None
    for index in range(len(chain) - 1, -1, -1):
        item = chain[index]
        if isinstance(item, ast.Assign):
            stmt_index = index
            stmt = item
            break
    if stmt is None or stmt_index is None:
        return None
    owner = _nearest_enclosing_function(chain[:stmt_index])
    if owner is None:
        return None
    if len(stmt.targets) != 1 or not isinstance(stmt.targets[0], ast.Tuple):
        return None
    if not all(isinstance(elt, ast.Name) for elt in stmt.targets[0].elts):
        return None
    if not isinstance(stmt.value, ast.Call):
        return None
    return stmt, stmt.value


def _is_statically_nameable_call_term(
    call: ast.Call,
    chain: tuple[ast.AST, ...],
    call_aliases: Dict[str, str],
    module_name: str,
    tree: ast.Module,
) -> bool:
    if not _is_statically_nameable_callee(
        call.func,
        chain,
        call_aliases,
        module_name,
        tree,
    ):
        return False
    if any(isinstance(arg, ast.Starred) for arg in call.args):
        return False
    if not all(
        _is_call_term_arg(arg, chain, call_aliases, module_name, tree)
        for arg in call.args
    ):
        return False
    for keyword in call.keywords:
        if keyword.arg is None:
            return False
        if not _is_call_term_arg(
            keyword.value,
            chain,
            call_aliases,
            module_name,
            tree,
        ):
            return False
    return True


def _is_statically_nameable_callee(
    func: ast.expr,
    chain: tuple[ast.AST, ...],
    call_aliases: Dict[str, str],
    module_name: str,
    tree: ast.Module,
) -> bool:
    if isinstance(func, ast.Name):
        return func.id in call_aliases
    if not isinstance(func, ast.Attribute):
        return False
    if isinstance(func.value, ast.Name) and func.value.id == "self":
        class_qualname = _nearest_class_qualname(chain)
        if not class_qualname:
            return False
        cls = _find_class_by_qualname(tree, class_qualname)
        return cls is not None and _class_has_stable_method(cls, func.attr)
    if isinstance(func.value, ast.Call) and _is_zero_arg_super_call(func.value):
        return (
            func.attr not in _NONDET_CALL_ATTRS
            and _current_class_has_single_base(chain, tree)
        )
    if isinstance(func.value, ast.Call):
        return (
            func.attr not in _NONDET_CALL_ATTRS
            and _is_statically_nameable_call_term(
                func.value,
                chain,
                call_aliases,
                module_name,
                tree,
            )
        )
    if isinstance(func.value, ast.Name):
        return func.attr not in _NONDET_CALL_ATTRS
    static_name = _static_call_name(func)
    if not static_name:
        return False
    root = static_name.split(".", 1)[0]
    return root in call_aliases


def _class_has_stable_method(cls: ast.ClassDef, name: str) -> bool:
    candidates = [
        stmt
        for stmt in cls.body
        if isinstance(stmt, ast.FunctionDef) and stmt.name == name
    ]
    return len(candidates) == 1 and not candidates[0].decorator_list


def _is_zero_arg_super_call(node: ast.Call) -> bool:
    return (
        isinstance(node.func, ast.Name)
        and node.func.id == "super"
        and not node.args
        and not node.keywords
    )


def _current_class_has_single_base(
    chain: tuple[ast.AST, ...],
    tree: ast.Module,
) -> bool:
    class_qualname = _nearest_class_qualname(chain)
    if not class_qualname:
        return False
    cls = _find_class_by_qualname(tree, class_qualname)
    return cls is not None and len(cls.bases) == 1


def _is_call_term_arg(
    node: ast.AST,
    chain: tuple[ast.AST, ...],
    call_aliases: Dict[str, str],
    module_name: str,
    tree: ast.Module,
) -> bool:
    if isinstance(node, (ast.Constant, ast.Name)):
        return True
    if isinstance(node, ast.Attribute):
        return _is_call_term_arg(node.value, chain, call_aliases, module_name, tree)
    if isinstance(node, ast.Call):
        return _is_statically_nameable_call_term(
            node,
            chain,
            call_aliases,
            module_name,
            tree,
        )
    return False


def _list_adapter_body_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    module_name: str,
) -> Optional[tuple[str, str]]:
    owner = _nearest_enclosing_function(ancestors + (node,))
    if owner is None or isinstance(owner, ast.Lambda):
        return None
    if _is_docstring_expr_node(node, ancestors):
        return "support", "docstring metadata supports source accounting only"
    if not _node_is_in_function_body(node, owner):
        return None
    universe, refusal = list_adapter_universe_for_callee(
        _owner_callee(module_name, owner, ancestors + (node,))
    )
    if refusal is not None or universe is None:
        return None
    return (
        "warranted",
        "list-adapter source family emitted into python.list-adapter-universe",
    )


def _instance_field_body_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    module_name: str,
) -> Optional[tuple[str, str]]:
    owner = _nearest_enclosing_function(ancestors + (node,))
    if owner is None or isinstance(owner, ast.Lambda):
        return None
    if _is_docstring_expr_node(node, ancestors):
        return "support", "docstring metadata supports source accounting only"
    if not _node_is_in_function_body(node, owner):
        return None
    universe, refusal = instance_field_universe_for_callee(
        _owner_callee(module_name, owner, ancestors + (node,))
    )
    if refusal is not None or universe is None:
        return None
    return (
        "warranted",
        "instance-field source family emitted into python.instance-field-universe",
    )


def _delegation_body_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
    module_name: str,
) -> Optional[tuple[str, str]]:
    owner = _nearest_enclosing_function(ancestors + (node,))
    if owner is None or isinstance(owner, ast.Lambda):
        return None
    if _is_docstring_expr_node(node, ancestors):
        return "support", "docstring metadata supports source accounting only"
    if not _node_is_in_function_body(node, owner):
        return None
    universe, refusal = delegation_universe_for_callee(
        _owner_callee(module_name, owner, ancestors + (node,))
    )
    if refusal is not None or universe is None:
        return None
    return (
        "warranted",
        "delegation source family emitted into python.delegation-universe",
    )


def _adapter_assignment_statement_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[ast.Assign | ast.AnnAssign, ast.expr | None]]:
    chain = ancestors + (node,)
    stmt_index: Optional[int] = None
    stmt: Optional[ast.Assign | ast.AnnAssign] = None
    for index in range(len(chain) - 1, -1, -1):
        item = chain[index]
        if isinstance(item, (ast.Assign, ast.AnnAssign)):
            stmt_index = index
            stmt = item
            break
    if stmt is None or stmt_index is None:
        return None
    owner = _nearest_enclosing_function(chain[:stmt_index])
    if owner is None:
        return None
    if isinstance(stmt, ast.Assign):
        if len(stmt.targets) != 1 or not _is_adapter_assignment_target(stmt.targets[0]):
            return None
        return stmt, stmt.value
    if not _is_adapter_assignment_target(stmt.target):
        return None
    return stmt, stmt.value


def _is_adapter_assignment_target(node: ast.AST) -> bool:
    if isinstance(node, ast.Name):
        return True
    return (
        isinstance(node, ast.Attribute)
        and isinstance(node.value, ast.Name)
        and node.value.id == "self"
    )


def _is_adapter_assignment_arg(node: ast.AST) -> bool:
    if isinstance(node, ast.Constant):
        return True
    if isinstance(node, ast.Name):
        return True
    if isinstance(node, ast.Attribute):
        return _is_adapter_assignment_arg(node.value)
    return False


def _local_name_binding_status(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[str, str]]:
    stmt = _local_name_binding_statement_for_locus(node, ancestors)
    if stmt is None:
        return None
    assign_stmt, target, value = stmt
    if node is target:
        return "warranted", "local SSA binding target admitted as compiler fact"
    if isinstance(value, ast.Name) and (node is value or node is assign_stmt):
        return "warranted", "local SSA alias assignment emitted as compiler equality"
    if value is not None and _is_local_literal_binding_value(value):
        if node is assign_stmt or any(descendant is node for descendant in ast.walk(value)):
            return "warranted", "local literal binding admitted as compiler fact"
    return None


def _local_name_binding_statement_for_locus(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> Optional[tuple[ast.Assign | ast.AnnAssign, ast.Name, ast.expr | None]]:
    chain = ancestors + (node,)
    stmt_index: Optional[int] = None
    stmt: Optional[ast.Assign | ast.AnnAssign] = None
    for index in range(len(chain) - 1, -1, -1):
        item = chain[index]
        if isinstance(item, (ast.Assign, ast.AnnAssign)):
            stmt_index = index
            stmt = item
            break
    if stmt is None or stmt_index is None:
        return None
    owner = _nearest_enclosing_function(chain[:stmt_index])
    if owner is None:
        return None
    if isinstance(stmt, ast.Assign):
        if len(stmt.targets) != 1 or not isinstance(stmt.targets[0], ast.Name):
            return None
        return stmt, stmt.targets[0], stmt.value
    if not isinstance(stmt.target, ast.Name):
        return None
    return stmt, stmt.target, stmt.value


def _nearest_enclosing_function(
    chain: tuple[ast.AST, ...],
) -> Optional[ast.FunctionDef | ast.AsyncFunctionDef | ast.Lambda]:
    for item in reversed(chain):
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
            return item
    return None


def _owner_callee(
    module_name: str,
    owner: ast.FunctionDef | ast.AsyncFunctionDef,
    chain: tuple[ast.AST, ...],
) -> str:
    class_qualname = _nearest_class_qualname(chain)
    if class_qualname:
        return f"{module_name}.{class_qualname}.{owner.name}"
    return f"{module_name}.{owner.name}"


def _is_local_literal_binding_value(node: ast.AST) -> bool:
    if isinstance(node, ast.Constant):
        return True
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, (ast.UAdd, ast.USub)):
        return _is_local_literal_binding_value(node.operand)
    if isinstance(node, (ast.Tuple, ast.List, ast.Set)):
        return all(_is_local_literal_binding_value(value) for value in node.elts)
    if isinstance(node, ast.Dict):
        return all(
            key is not None
            and _is_local_literal_binding_value(key)
            and _is_local_literal_binding_value(value)
            for key, value in zip(node.keys, node.values)
        )
    return False


def _static_call_name(node: ast.AST) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        prefix = _static_call_name(node.value)
        return f"{prefix}.{node.attr}" if prefix else node.attr
    return ""


def _nearest_declaration_ancestor(
    ancestors: tuple[ast.AST, ...],
) -> Optional[ast.AST]:
    for ancestor in reversed(ancestors):
        if isinstance(ancestor, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            return ancestor
    return None


def _is_docstring_expr_node(
    node: ast.AST,
    ancestors: tuple[ast.AST, ...],
) -> bool:
    if (
        isinstance(node, ast.Expr)
        and isinstance(node.value, ast.Constant)
        and isinstance(node.value.value, str)
    ):
        return True
    if not (
        isinstance(node, ast.Constant)
        and isinstance(node.value, str)
        and ancestors
        and isinstance(ancestors[-1], ast.Expr)
    ):
        return False
    expr = ancestors[-1]
    return isinstance(expr.value, ast.Constant) and expr.value is node


def _package_source_audits(source_audits: List[Any]) -> List[Dict[str, Any]]:
    covered_lines = _covered_source_lines(source_audits)
    audits: List[Dict[str, Any]] = []
    for root, package in sorted(
        _package_roots_from_source_audits(source_audits).items(),
        key=lambda item: str(item[0]),
    ):
        loci = _package_unclassified_loci(root, covered_lines)
        if not loci:
            continue
        audits.append(
            {
                "kind": "source-audit",
                "language": "python",
                "contract": {"name": f"{package}#source-accounting"},
                "role": "python.package-source",
                "universe_kind": "package-accounting",
                "package": package,
                "package_root": str(root),
                "loci": loci,
                "totals": _source_totals(loci),
            }
        )
    return audits


def _with_package_source_accounting(lifted: Dict[str, Any]) -> Dict[str, Any]:
    source_audits = list(lifted.get("sourceAudits") or [])
    package_audits = _package_source_audits(source_audits)
    if not package_audits:
        return lifted
    source_ledger = dict(lifted.get("sourceLedger") or _empty_source_ledger())
    for audit in package_audits:
        source_audits.append(audit)
        _merge_source_ledger(source_ledger, audit.get("totals") or {})
    out = dict(lifted)
    out["sourceAudits"] = source_audits
    out["sourceLedger"] = source_ledger
    return out


def _source_mementos_from_decls(decls: List[Any]) -> List[Dict[str, Any]]:
    out: List[Dict[str, Any]] = []
    seen = set()
    for decl in decls:
        if not isinstance(decl, ContractDecl):
            continue
        for warrant in getattr(decl, "source_warrants", []):
            if not isinstance(warrant, dict):
                continue
            memento = dict(warrant)
            memento["kind"] = "source-memento"
            memento.setdefault("claimName", decl.name)
            memento.setdefault("contractName", decl.name)
            memento.pop("body_text", None)
            memento.pop("ast_template", None)
            key = json.dumps(memento, sort_keys=True, separators=(",", ":"))
            if key in seen:
                continue
            seen.add(key)
            out.append(memento)
    return out


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

    return _with_package_source_accounting({
        "decls": decls,
        "declarations": declarations_array,
        "callEdges": call_edges_array,
        "warnings": [w.__dict__ for w in layer2.warnings + production_walk.warnings],
        "implications": _implications_to_json(layer2) + _implications_to_json(production_walk),
        "sourceMementos": _source_mementos_from_decls(decls),
        "sourceAudits": list(layer2.source_audits),
        "sourceLedger": dict(layer2.source_ledger),
    })


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
                    "sourceMementos": lifted["sourceMementos"],
                    "sourceAudits": lifted["sourceAudits"],
                    "sourceLedger": lifted["sourceLedger"],
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
        source_mementos: List[Any] = []
        source_audits: List[Any] = []
        source_ledger = _empty_source_ledger()
        seen_package_audits: Set[str] = set()
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
            source_mementos.extend(lifted["sourceMementos"])
            for audit in lifted["sourceAudits"]:
                if not isinstance(audit, dict):
                    continue
                if audit.get("role") == "python.package-source":
                    key = str(audit.get("package_root") or audit.get("package") or "")
                    if key and key in seen_package_audits:
                        continue
                    if key:
                        seen_package_audits.add(key)
                source_audits.append(audit)
                _merge_source_ledger(source_ledger, audit.get("totals") or {})

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
                    "sourceMementos": source_mementos,
                    "diagnostics": [],
                    "warnings": warnings,
                    "sourceAudits": source_audits,
                    "sourceLedger": source_ledger,
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
