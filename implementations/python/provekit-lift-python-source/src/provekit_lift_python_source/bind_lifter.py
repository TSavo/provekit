from __future__ import annotations

import ast
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

from .canonical import cid_of_json

Json = Any


@dataclass
class BindLiftResult:
    ir: list[Json] = field(default_factory=list)
    diagnostics: list[Json] = field(default_factory=list)


@dataclass(frozen=True)
class _FunctionInfo:
    node: ast.FunctionDef | ast.AsyncFunctionDef


def lift_source(source: str, source_path: str) -> BindLiftResult:
    result = BindLiftResult()
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as exc:
        result.diagnostics.append(
            {
                "kind": "parse-error",
                "message": exc.msg,
                "path": source_path,
                "line": exc.lineno,
            }
        )
        return result

    collector = _DefinitionCollector()
    collector.visit(tree)
    lines = source.splitlines()
    rel_path = source_path.replace(os.sep, "/")
    for info in collector.definitions:
        result.ir.append(_entry_for_function(info.node, rel_path, lines))
    return result


def lift_paths(workspace_root: str, source_paths: Iterable[str]) -> BindLiftResult:
    result = BindLiftResult()
    root = Path(workspace_root or ".").resolve()
    paths = list(source_paths) or ["."]
    for requested in paths:
        path = Path(requested)
        full = path if path.is_absolute() else root / path
        try:
            resolved = full.resolve()
        except OSError as exc:
            result.diagnostics.append(
                {
                    "kind": "io-error",
                    "message": f"cannot resolve path '{requested}': {exc}",
                }
            )
            continue
        if not _is_relative_to(resolved, root):
            result.diagnostics.append(
                {
                    "kind": "path-traversal",
                    "message": f"path '{requested}' escapes workspace root '{root}'",
                }
            )
            continue
        files = list(_iter_python_files(resolved))
        if not files:
            result.diagnostics.append(
                {
                    "kind": "warning",
                    "message": f"path not found or not .py: {resolved}",
                }
            )
            continue
        for file_path in files:
            try:
                source = file_path.read_text(encoding="utf-8")
            except OSError as exc:
                result.diagnostics.append(
                    {
                        "kind": "io-error",
                        "message": f"cannot read '{file_path}': {exc}",
                    }
                )
                continue
            display_path = os.path.relpath(file_path, root).replace(os.sep, "/")
            file_result = lift_source(source, display_path)
            result.ir.extend(file_result.ir)
            result.diagnostics.extend(file_result.diagnostics)
    return result


class _DefinitionCollector(ast.NodeVisitor):
    def __init__(self) -> None:
        self.definitions: list[_FunctionInfo] = []

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self.definitions.append(_FunctionInfo(node=node))
        self.generic_visit(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self.definitions.append(_FunctionInfo(node=node))
        self.generic_visit(node)


def _entry_for_function(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    rel_path: str,
    lines: list[str],
) -> Json:
    concept, attr_pre, attr_post = _extract_leading_annotations(lines, node.lineno)
    term_shape = _function_shape(node)
    param_names, param_types = _signature_params(node.args)
    return_type = _annotation_text(node.returns)
    if return_type is None:
        return_type = "Any"
    elif return_type == "None":
        return_type = "()"

    return {
        "attr_post": attr_post,
        "attr_pre": attr_pre,
        "concept_annotation": concept,
        "file": rel_path,
        "fn_line": node.lineno,
        "fn_name": node.name,
        "kind": "bind-lift-entry",
        "param_names": param_names,
        "param_types": param_types,
        "return_type": return_type,
        "term_shape": term_shape,
        "term_shape_cid": cid_of_json(term_shape),
    }


def _signature_params(args: ast.arguments) -> tuple[list[str], list[str]]:
    names: list[str] = []
    types: list[str] = []
    ordered_args: list[ast.arg] = []
    ordered_args.extend(args.posonlyargs)
    ordered_args.extend(args.args)
    if args.vararg is not None:
        ordered_args.append(args.vararg)
    ordered_args.extend(args.kwonlyargs)
    if args.kwarg is not None:
        ordered_args.append(args.kwarg)
    for arg in ordered_args:
        names.append(arg.arg)
        types.append(_annotation_text(arg.annotation) or "Any")
    return names, types


def _annotation_text(annotation: ast.expr | None) -> str | None:
    if annotation is None:
        return None
    try:
        return ast.unparse(annotation)
    except Exception:
        return "Any"


def _function_shape(node: ast.FunctionDef | ast.AsyncFunctionDef) -> Json:
    statements = [stmt for stmt in node.body if not _is_docstring_stmt(stmt)]
    return {"kind": "body", "stmts": [_shape_stmt(stmt, top_level=True) for stmt in statements]}


def _shape_block(statements: list[ast.stmt]) -> Json:
    return {
        "kind": "block",
        "stmts": [
            _shape_stmt(stmt, top_level=False)
            for stmt in statements
            if not _is_docstring_stmt(stmt)
        ],
    }


def _shape_stmt(node: ast.stmt, *, top_level: bool) -> Json:
    if isinstance(node, ast.If):
        shaped: dict[str, Json] = {
            "cond": _shape_expr(node.test),
            "kind": "if",
            "then": _shape_block(node.body),
        }
        if node.orelse:
            shaped = {
                "cond": shaped["cond"],
                "else": _shape_block(node.orelse),
                "kind": shaped["kind"],
                "then": shaped["then"],
            }
        return shaped
    if isinstance(node, ast.While):
        return {
            "body": _shape_block(node.body),
            "cond": _shape_expr(node.test),
            "kind": "while",
        }
    if isinstance(node, (ast.For, ast.AsyncFor)):
        return {"body": _shape_block(node.body), "kind": "for"}
    if isinstance(node, (ast.Return, ast.Break, ast.Continue)):
        return {"kind": "exit"}
    if isinstance(node, ast.Assign):
        if top_level and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            return {"kind": "let"}
        return {"kind": "assign"}
    if isinstance(node, ast.AnnAssign):
        if top_level and isinstance(node.target, ast.Name):
            return {"kind": "let"}
        return {"kind": "assign"}
    if isinstance(node, ast.AugAssign):
        return {"kind": "assign"}
    if isinstance(node, ast.Expr):
        return _shape_expr(node.value)
    return {"kind": "opaque"}


def _shape_expr(node: ast.expr) -> Json:
    if isinstance(node, ast.BinOp):
        return {"kind": "bin", "op": _bin_op(node.op)}
    if isinstance(node, ast.Compare):
        op = _rel_op(node.ops[0]) if node.ops else "opaque-op"
        return {"kind": "rel", "op": op}
    if isinstance(node, ast.Call):
        return {"kind": "call"}
    return {"kind": "opaque"}


def _bin_op(op: ast.operator) -> str:
    if isinstance(op, ast.Add):
        return "+"
    if isinstance(op, ast.Sub):
        return "-"
    if isinstance(op, ast.Mult):
        return "*"
    if isinstance(op, ast.Div):
        return "/"
    if isinstance(op, ast.Mod):
        return "%"
    return "opaque-op"


def _rel_op(op: ast.cmpop) -> str:
    if isinstance(op, ast.Eq):
        return "=="
    if isinstance(op, ast.NotEq):
        return "!="
    if isinstance(op, ast.Lt):
        return "<"
    if isinstance(op, ast.LtE):
        return "<="
    if isinstance(op, ast.Gt):
        return ">"
    if isinstance(op, ast.GtE):
        return ">="
    return "opaque-op"


def _extract_leading_annotations(
    lines: list[str],
    fn_line: int,
) -> tuple[str | None, str | None, str | None]:
    concept: str | None = None
    attr_pre: str | None = None
    attr_post: str | None = None
    idx = fn_line - 2
    while idx >= 0:
        line = lines[idx].strip()
        if line.startswith("# concept:"):
            if concept is None:
                candidate = line[len("# concept:") :].strip()
                if candidate.startswith("UNNAMED-CONCEPT-"):
                    concept = None
                    break
                concept = candidate
            idx -= 1
            continue
        if line.startswith("# @requires:"):
            if attr_pre is None:
                attr_pre = line[len("# @requires:") :].strip()
            idx -= 1
            continue
        if line.startswith("# @ensures:"):
            if attr_post is None:
                attr_post = line[len("# @ensures:") :].strip()
            idx -= 1
            continue
        if line == "" or line.startswith("#") or line.startswith("@"):
            idx -= 1
            continue
        break
    return concept, attr_pre, attr_post


def _iter_python_files(path: Path) -> Iterable[Path]:
    if path.is_dir():
        yield from sorted(p for p in path.rglob("*.py") if p.is_file())
    elif path.is_file() and path.suffix == ".py":
        yield path


def _is_relative_to(child: Path, parent: Path) -> bool:
    try:
        child.relative_to(parent)
        return True
    except ValueError:
        return False


def _is_docstring_stmt(node: ast.stmt) -> bool:
    return (
        isinstance(node, ast.Expr)
        and isinstance(node.value, ast.Constant)
        and isinstance(node.value.value, str)
    )
