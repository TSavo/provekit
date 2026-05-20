from __future__ import annotations

import ast
import json
import os
import re
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path
from typing import Any, Iterable

from provekit_lift_py_tests.canonicalizer import blake3_512_of, encode_jcs
from provekit_lift_py_tests.decorators import _parse_expr_string
from provekit_lift_py_tests.ir import formula_to_value

from .canonical import cid_of_json

Json = Any
CID_RE = re.compile(r"^blake3-512:[0-9a-f]{128}$")
CONTRACT_COMMENT_KIND = "provekit-contract-comment-sugar"
CONCEPT_CITATION_COMMENT_KIND = "provekit-concept-citation-comment-sugar"
CONTRACT_COMMENT_ROLE_MAP = {
    "pre": "pre",
    "post": "post",
    "invariant": "inv",
    "throws": "throws",
    "observation": "observation",
}


class _ConceptCitationRefusal(Exception):
    """Raised on substrate-identity contradiction (spec section 6 rows 7-8).
    The surrounding relift refuses entirely; no IR entry is emitted
    for the function whose source contained the contradiction."""

    def __init__(self, diag_kind: str, rel_path: str, line_no: int, message: str):
        super().__init__(message)
        self.diag_kind = diag_kind
        self.rel_path = rel_path
        self.line_no = line_no
        self.message = message


@dataclass
class BindLiftResult:
    ir: list[Json] = field(default_factory=list)
    diagnostics: list[Json] = field(default_factory=list)


@dataclass(frozen=True)
class _FunctionInfo:
    node: ast.FunctionDef | ast.AsyncFunctionDef


@dataclass(frozen=True)
class _ShapeResult:
    shape: Json
    operand_bindings: list[Json]


@dataclass(frozen=True)
class _CommentOccurrence:
    line_no: int
    surface: str


def lift_source(source: str, source_path: str, layer: str = "all") -> BindLiftResult:
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
    source_lines = source.splitlines(keepends=True)
    rel_path = source_path.replace(os.sep, "/")
    for info in collector.definitions:
        try:
            if layer == "library-bindings":
                entry = _library_binding_entry_for_function(
                    info.node, rel_path, lines, source_lines
                )
                if entry is not None:
                    result.ir.append(entry)
                continue
            result.ir.append(
                _entry_for_function(info.node, rel_path, lines, result.diagnostics)
            )
        except _ConceptCitationRefusal as exc:
            result.diagnostics.append(
                {
                    "kind": exc.diag_kind,
                    "message": exc.message,
                    "path": exc.rel_path,
                    "line": exc.line_no,
                }
            )
    return result


def lift_paths(
    workspace_root: str, source_paths: Iterable[str], layer: str = "all"
) -> BindLiftResult:
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
            file_result = lift_source(source, display_path, layer=layer)
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
    diagnostics: list[Json],
) -> Json:
    shape_result = _function_shape_with_bindings(node, lines)
    term_shape = shape_result.shape
    param_names = _signature_param_names(node.args)
    witnesses = []
    witnesses.extend(_contract_comment_witnesses(lines, node, rel_path, diagnostics))
    witnesses.extend(_decorator_contract_witnesses(node, param_names, rel_path, diagnostics))
    _concept_citation_comments(lines, node, rel_path, diagnostics)

    return {
        "kind": "bind-lift-entry",
        "param_names": param_names,
        "term_shape": term_shape,
        "term_shape_cid": cid_of_json(term_shape),
        "operand_bindings": shape_result.operand_bindings,
        "source_function_name": node.name,
        "witnesses": witnesses,
    }


def _library_binding_entry_for_function(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    rel_path: str,
    lines: list[str],
    source_lines: list[str],
) -> Json | None:
    binding = _sugar_bind_decorator(node)
    if binding is None:
        return None

    shape_result = _function_shape_with_bindings(node, lines)
    term_shape = shape_result.shape
    param_names = _signature_param_names(node.args)
    param_types = [
        _annotation_surface(arg.annotation) for arg in _ordered_signature_args(node.args)
    ]
    return_type = _annotation_surface(node.returns)
    signature_shape = {
        "param_names": param_names,
        "param_types": param_types,
        "return_type": return_type,
    }
    body_source = _body_source_locator(node, rel_path, source_lines)

    return {
        "body_source": body_source,
        "concept_name": binding["concept_name"],
        "kind": "library-sugar-binding-entry",
        "loss_record_contribution": {
            "form": "literal",
            "value": {"entries": []},
        },
        "param_names": param_names,
        "param_types": param_types,
        "return_type": return_type,
        "signature_shape_cid": cid_of_json(signature_shape),
        "source_function_name": node.name,
        "target_language": "python",
        "target_library_tag": binding["target_library_tag"],
        "term_shape": term_shape,
        "term_shape_cid": cid_of_json(term_shape),
    }


def _sugar_bind_decorator(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
) -> dict[str, str] | None:
    for decorator in node.decorator_list:
        if not isinstance(decorator, ast.Call) or not _is_sugar_bind_func(decorator.func):
            continue
        concept = _keyword_str(decorator, "concept")
        library = _keyword_str(decorator, "library")
        if concept and library:
            return {"concept_name": concept, "target_library_tag": library}
    return None


def _is_sugar_bind_func(func: ast.expr) -> bool:
    if not isinstance(func, ast.Attribute) or func.attr != "bind":
        return False
    value = func.value
    if isinstance(value, ast.Name):
        return value.id == "sugar"
    if isinstance(value, ast.Attribute) and value.attr == "sugar":
        return isinstance(value.value, ast.Name) and value.value.id == "provekit"
    return False


def _keyword_str(call: ast.Call, name: str) -> str | None:
    for keyword in call.keywords:
        if keyword.arg == name and isinstance(keyword.value, ast.Constant):
            if isinstance(keyword.value.value, str) and keyword.value.value:
                return keyword.value.value
    return None


def _ordered_signature_args(args: ast.arguments) -> list[ast.arg]:
    ordered_args: list[ast.arg] = []
    ordered_args.extend(args.posonlyargs)
    ordered_args.extend(args.args)
    if args.vararg is not None:
        ordered_args.append(args.vararg)
    ordered_args.extend(args.kwonlyargs)
    if args.kwarg is not None:
        ordered_args.append(args.kwarg)
    return ordered_args


def _annotation_surface(annotation: ast.expr | None) -> str | None:
    if annotation is None:
        return None
    return ast.unparse(annotation)


def _body_source_locator(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    rel_path: str,
    source_lines: list[str],
) -> Json:
    start_line = node.lineno
    start_col = node.col_offset
    if node.decorator_list:
        first = min(node.decorator_list, key=lambda decorator: decorator.lineno)
        start_line = first.lineno
        start_col = 0
    end_line = node.end_lineno or node.lineno
    end_col = node.end_col_offset or 0
    span_text = "".join(source_lines[start_line - 1 : end_line])
    return {
        "file": rel_path,
        "source_cid": blake3_512_of(span_text.encode("utf-8")),
        "span": {
            "start_line": start_line,
            "start_col": start_col,
            "end_line": end_line,
            "end_col": end_col,
        },
    }


def _signature_param_names(args: ast.arguments) -> list[str]:
    names: list[str] = []
    for arg in _ordered_signature_args(args):
        names.append(arg.arg)
    return names


def _function_shape(node: ast.FunctionDef | ast.AsyncFunctionDef) -> Json:
    return _function_shape_with_bindings(node).shape


def _function_shape_with_bindings(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    lines: list[str] | None = None,
) -> _ShapeResult:
    statements = [stmt for stmt in node.body if not _is_docstring_stmt(stmt)]
    comments = _trivia_comment_occurrences(lines, node) if lines is not None else []
    return _shape_block_with_bindings(statements, comments)


def _shape_block(statements: list[ast.stmt]) -> Json:
    return _shape_block_with_bindings(statements).shape


def _shape_block_with_bindings(
    statements: list[ast.stmt],
    comments: list[_CommentOccurrence] | None = None,
) -> _ShapeResult:
    shapes: list[Json] = []
    binding_groups: list[list[Json]] = []
    leaf_only: _ShapeResult | None = None
    pending_comments = sorted(comments or [], key=lambda comment: comment.line_no)
    comment_index = 0
    for stmt in statements:
        if _is_docstring_stmt(stmt):
            continue
        stmt_line = getattr(stmt, "lineno", 0)
        while (
            comment_index < len(pending_comments)
            and pending_comments[comment_index].line_no < stmt_line
        ):
            comment = pending_comments[comment_index]
            shapes.append(_comment_shape(comment.surface))
            binding_groups.append([])
            comment_index += 1
        candidate = _shape_stmt_with_bindings(stmt, top_level=False)
        shape = candidate.shape
        if _shape_has_operator_identity(shape):
            shapes.append(shape)
            binding_groups.append(candidate.operand_bindings)
        elif leaf_only is None and candidate.operand_bindings:
            leaf_only = candidate
    while comment_index < len(pending_comments):
        comment = pending_comments[comment_index]
        shapes.append(_comment_shape(comment.surface))
        binding_groups.append([])
        comment_index += 1
    if not shapes and leaf_only is not None:
        return _ShapeResult({}, _sort_operand_bindings(leaf_only.operand_bindings))
    return _collapse_operation_shape_results(shapes, binding_groups)


def _shape_stmt(node: ast.stmt, *, top_level: bool) -> Json:
    return _shape_stmt_with_bindings(node, top_level=top_level).shape


def _shape_stmt_with_bindings(node: ast.stmt, *, top_level: bool) -> _ShapeResult:
    if isinstance(node, ast.If):
        test = _shape_expr_with_bindings(node.test)
        body = _shape_block_with_bindings(node.body)
        orelse = _shape_block_with_bindings(node.orelse)
        return _operator_shape_result(
            "concept:conditional",
            [test, body, orelse],
        )
    if isinstance(node, ast.While):
        return _operator_shape_result(
            "concept:while",
            [_shape_expr_with_bindings(node.test), _shape_block_with_bindings(node.body)],
        )
    if isinstance(node, (ast.For, ast.AsyncFor)):
        return _operator_shape_result("concept:for", [_shape_block_with_bindings(node.body)])
    if isinstance(node, ast.Return):
        if node.value is None:
            return _empty_shape_result()
        return _shape_expr_with_bindings(node.value)
    if isinstance(node, ast.Pass):
        return _operator_shape_result("concept:skip", [])
    if isinstance(node, ast.Break):
        return _operator_shape_result("concept:break", [])
    if isinstance(node, ast.Continue):
        return _operator_shape_result("concept:continue", [])
    if isinstance(node, ast.Assign):
        target = _shape_expr_with_bindings(node.targets[0]) if node.targets else _empty_shape_result()
        return _operator_shape_result("concept:assign", [target, _shape_expr_with_bindings(node.value)])
    if isinstance(node, ast.AnnAssign):
        if node.value is not None:
            return _operator_shape_result(
                "concept:assign",
                [_shape_expr_with_bindings(node.target), _shape_expr_with_bindings(node.value)],
            )
        return _empty_shape_result()
    if isinstance(node, ast.AugAssign):
        return _bin_operator_shape_result(
            node.op,
            [_shape_expr_with_bindings(node.target), _shape_expr_with_bindings(node.value)],
        )
    if isinstance(node, ast.Expr):
        return _shape_expr_with_bindings(node.value)
    return _empty_shape_result()


def _shape_expr(node: ast.expr) -> Json:
    return _shape_expr_with_bindings(node).shape


def _shape_expr_with_bindings(node: ast.expr) -> _ShapeResult:
    if isinstance(node, ast.BinOp):
        return _bin_operator_shape_result(
            node.op,
            [_shape_expr_with_bindings(node.left), _shape_expr_with_bindings(node.right)],
        )
    if isinstance(node, ast.BoolOp):
        op = _bool_op(node.op)
        values = [_shape_expr_with_bindings(value) for value in node.values]
        if op is None:
            return _empty_shape_result()
        return _operator_shape_result(op, values)
    if isinstance(node, ast.UnaryOp):
        op = _unary_op(node.op)
        if op is None:
            return _empty_shape_result()
        return _operator_shape_result(op, [_shape_expr_with_bindings(node.operand)])
    if isinstance(node, ast.IfExp):
        return _operator_shape_result(
            "concept:conditional",
            [
                _shape_expr_with_bindings(node.test),
                _shape_expr_with_bindings(node.body),
                _shape_expr_with_bindings(node.orelse),
            ],
        )
    if isinstance(node, ast.Compare):
        return _compare_shape_result(node)
    if isinstance(node, ast.Call):
        args = [_shape_expr_with_bindings(node.func)]
        args.extend(_shape_expr_with_bindings(arg) for arg in node.args)
        args.extend(_shape_expr_with_bindings(keyword.value) for keyword in node.keywords)
        return _operator_shape_result("concept:call", args)
    symbol = _operand_symbol(node)
    if symbol is not None:
        return _ShapeResult({}, [{"position": [], "symbol": symbol}])
    return _empty_shape_result()


def _shape_has_operator_identity(value: Json) -> bool:
    if isinstance(value, dict):
        if "concept_name" in value or "op_cid" in value:
            return True
        return any(_shape_has_operator_identity(child) for child in value.values())
    if isinstance(value, list):
        return any(_shape_has_operator_identity(child) for child in value)
    return False


def _collapse_operation_shapes(shapes: list[Json]) -> Json:
    if not shapes:
        return {}
    if len(shapes) == 1:
        return shapes[0]
    return _operator_shape("concept:seq", shapes)


def _collapse_operation_shape_results(
    shapes: list[Json],
    binding_groups: list[list[Json]],
) -> _ShapeResult:
    if not shapes:
        return _empty_shape_result()
    if len(shapes) == 1:
        return _ShapeResult(shapes[0], _sort_operand_bindings(binding_groups[0]))
    bindings: list[Json] = []
    for index, group in enumerate(binding_groups):
        bindings.extend(_prefix_bindings(group, index))
    return _ShapeResult(
        _operator_shape("concept:seq", shapes),
        _sort_operand_bindings(bindings),
    )


def _bin_operator_shape(op: ast.operator, args: list[Json]) -> Json:
    atom = _bin_op(op)
    if atom is None:
        return {}
    return _operator_shape(atom, args)


def _bin_operator_shape_result(op: ast.operator, args: list[_ShapeResult]) -> _ShapeResult:
    atom = _bin_op(op)
    if atom is None:
        return _empty_shape_result()
    return _operator_shape_result(atom, args)


def _compare_shape_result(node: ast.Compare) -> _ShapeResult:
    if not node.ops or len(node.ops) != len(node.comparators):
        return _empty_shape_result()
    operands: list[ast.expr] = [node.left, *node.comparators]
    comparisons: list[_ShapeResult] = []
    for index, raw_op in enumerate(node.ops):
        op = _rel_op(raw_op)
        if op is None:
            return _empty_shape_result()
        comparisons.append(
            _operator_shape_result(
                op,
                [
                    _shape_expr_with_bindings(operands[index]),
                    _shape_expr_with_bindings(operands[index + 1]),
                ],
            )
        )
    result = comparisons[0]
    for comparison in comparisons[1:]:
        result = _operator_shape_result("concept:ite", [result, comparison, _bool_literal(False)])
    return result


def _bool_literal(value: bool) -> _ShapeResult:
    return _ShapeResult({}, [{"position": [], "symbol": "True" if value else "False"}])


def _operator_shape(concept_name: str, args: list[Json]) -> Json:
    op_cid = _concept_op_cid(concept_name)
    if op_cid is None:
        return {}
    return {
        "args": [_operand_slot(arg) for arg in args],
        "concept_name": concept_name,
        "op_cid": op_cid,
    }


def _comment_shape(surface: str) -> Json:
    op_cid = _concept_op_cid("concept:comment")
    if op_cid is None:
        return {}
    return {
        "args": [{"kind": "literal", "value": surface}],
        "concept_name": "concept:comment",
        "op_cid": op_cid,
    }


def _operator_shape_result(concept_name: str, args: list[_ShapeResult]) -> _ShapeResult:
    shape = _operator_shape(concept_name, [arg.shape for arg in args])
    if not shape:
        return _empty_shape_result()
    bindings: list[Json] = []
    for index, arg in enumerate(args):
        bindings.extend(_prefix_bindings(arg.operand_bindings, index))
    return _ShapeResult(shape, _sort_operand_bindings(bindings))


def _empty_shape_result() -> _ShapeResult:
    return _ShapeResult({}, [])


def _prefix_bindings(bindings: list[Json], prefix: int) -> list[Json]:
    return [
        {"position": [prefix, *binding["position"]], "symbol": binding["symbol"]}
        for binding in bindings
    ]


def _sort_operand_bindings(bindings: list[Json]) -> list[Json]:
    return sorted(bindings, key=lambda binding: binding["position"])


def _operand_symbol(node: ast.AST) -> str | None:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Constant):
        value = node.value
        if isinstance(value, bool):
            return "True" if value else "False"
        if isinstance(value, int):
            return str(value)
        if isinstance(value, float):
            return repr(value)
        if isinstance(value, str):
            return json.dumps(value, ensure_ascii=False)
        if value is None:
            return "None"
    return None


def _operand_slot(value: Json) -> Json:
    if (
        isinstance(value, dict)
        and isinstance(value.get("concept_name"), str)
        and isinstance(value.get("op_cid"), str)
        and isinstance(value.get("args"), list)
    ):
        return value
    if isinstance(value, dict) and (
        value.get("kind") in {"literal", "const"} or "value" in value
    ):
        return value
    return {}


def _bin_op(op: ast.operator) -> str | None:
    table: tuple[tuple[type[ast.operator], str], ...] = (
        (ast.Add, "concept:add"),
        (ast.Sub, "concept:sub"),
        (ast.Mult, "concept:mul"),
        (ast.Div, "concept:div"),
        (ast.Mod, "concept:mod"),
        (ast.LShift, "concept:shl"),
        (ast.RShift, "concept:shr"),
        (ast.BitAnd, "concept:bitand"),
        (ast.BitOr, "concept:bitor"),
        (ast.BitXor, "concept:bitxor"),
    )
    for cls, concept_name in table:
        if isinstance(op, cls):
            return concept_name
    return None


def _bool_op(op: ast.boolop) -> str | None:
    if isinstance(op, ast.And):
        return "concept:and"
    if isinstance(op, ast.Or):
        return "concept:or"
    return None


def _unary_op(op: ast.unaryop) -> str | None:
    if isinstance(op, ast.Not):
        return "concept:not"
    if isinstance(op, ast.USub):
        return "concept:neg"
    if isinstance(op, ast.Invert):
        return "concept:bitnot"
    return None


def _rel_op(op: ast.cmpop) -> str | None:
    table: tuple[tuple[type[ast.cmpop], str], ...] = (
        (ast.Eq, "concept:eq"),
        (ast.NotEq, "concept:ne"),
        (ast.Lt, "concept:lt"),
        (ast.LtE, "concept:le"),
        (ast.Gt, "concept:gt"),
        (ast.GtE, "concept:ge"),
    )
    for cls, concept_name in table:
        if isinstance(op, cls):
            return concept_name
    return None


def _contract_comment_witnesses(
    lines: list[str],
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    rel_path: str,
    diagnostics: list[Json],
) -> list[Json]:
    witnesses: list[Json] = []
    witnesses.extend(
        _contract_comment_witnesses_from_surface_lines(
            _leading_contract_comment_surface(lines, node.lineno),
            rel_path,
            diagnostics,
        )
    )
    docstring = ast.get_docstring(node, clean=True)
    if docstring:
        doc_lines = [
            (getattr(node.body[0], "lineno", node.lineno), line.strip())
            for line in docstring.splitlines()
        ]
        witnesses.extend(
            _contract_comment_witnesses_from_surface_lines(
                doc_lines,
                rel_path,
                diagnostics,
            )
        )
    return witnesses


def _leading_contract_comment_surface(
    lines: list[str],
    fn_line: int,
) -> list[tuple[int, str]]:
    start = fn_line - 2
    while start >= 0:
        stripped = lines[start].strip()
        if stripped == "" or stripped.startswith("#") or stripped.startswith("@"):
            start -= 1
            continue
        break
    surface: list[tuple[int, str]] = []
    for idx in range(start + 1, fn_line - 1):
        stripped = lines[idx].strip()
        if stripped.startswith("#"):
            surface.append((idx + 1, stripped[1:].strip()))
    return surface


def _contract_comment_witnesses_from_surface_lines(
    surface_lines: list[tuple[int, str]],
    rel_path: str,
    diagnostics: list[Json],
) -> list[Json]:
    witnesses: list[Json] = []
    idx = 0
    while idx < len(surface_lines):
        line_no, content = surface_lines[idx]
        if not content.startswith("provekit-contract:"):
            idx += 1
            continue
        raw_payload = content[len("provekit-contract:") :].strip()
        payload_cid: str | None = None
        if idx + 1 < len(surface_lines):
            _, next_content = surface_lines[idx + 1]
            if next_content.startswith("provekit-contract-payload-cid:"):
                payload_cid = next_content[
                    len("provekit-contract-payload-cid:") :
                ].strip()
                idx += 1
        witness = _contract_comment_witness(
            raw_payload,
            payload_cid,
            rel_path,
            line_no,
            diagnostics,
        )
        if witness is not None:
            witnesses.append(witness)
        idx += 1
    return witnesses


def _contract_comment_witness(
    raw_payload: str,
    emitted_payload_cid: str | None,
    rel_path: str,
    line_no: int,
    diagnostics: list[Json],
) -> Json | None:
    try:
        payload = json.loads(raw_payload)
    except json.JSONDecodeError as exc:
        _contract_comment_diag(
            diagnostics,
            rel_path,
            line_no,
            f"malformed JSON: {exc.msg}",
        )
        return None
    if not isinstance(payload, dict):
        _contract_comment_diag(diagnostics, rel_path, line_no, "payload is not an object")
        return None

    def require_str(key: str) -> str | None:
        value = payload.get(key)
        if isinstance(value, str) and value:
            return value
        _contract_comment_diag(diagnostics, rel_path, line_no, f"missing {key}")
        return None

    if payload.get("artifact_kind") != CONTRACT_COMMENT_KIND:
        _contract_comment_diag(diagnostics, rel_path, line_no, "wrong artifact_kind")
        return None
    if payload.get("schema_version") != "1":
        _contract_comment_diag(diagnostics, rel_path, line_no, "unknown schema_version")
        return None

    fol_text = require_str("fol_text")
    if fol_text is None:
        return None

    emitted_by = payload.get("emitted_by")
    if not _valid_emitted_by(emitted_by):
        _contract_comment_diag(diagnostics, rel_path, line_no, "malformed emitted_by")
        return None

    role = require_str("role")
    if role not in CONTRACT_COMMENT_ROLE_MAP:
        _contract_comment_diag(diagnostics, rel_path, line_no, "unknown role")
        return None

    cid_fields = [
        "concept_site_cid",
        "contract_cid",
        "ir_formula_jcs_cid",
        "loss_record_cid",
        "policy_cid",
        "sugar_dict_cid",
    ]
    for key in cid_fields:
        value = require_str(key)
        if value is None or not CID_RE.fullmatch(value):
            _contract_comment_diag(diagnostics, rel_path, line_no, f"malformed {key}")
            return None
    local_contract_cid = payload.get("local_contract_cid")
    if local_contract_cid is not None and (
        not isinstance(local_contract_cid, str) or not CID_RE.fullmatch(local_contract_cid)
    ):
        _contract_comment_diag(diagnostics, rel_path, line_no, "malformed local_contract_cid")
        return None

    predicate = payload.get("ir_formula_jcs")
    if not isinstance(predicate, dict):
        _contract_comment_diag(diagnostics, rel_path, line_no, "missing ir_formula_jcs")
        return None
    if not _valid_formula_shape(predicate):
        _contract_comment_diag(diagnostics, rel_path, line_no, "invalid formula shape")
        return None
    if cid_of_json(predicate) != payload["ir_formula_jcs_cid"]:
        _contract_comment_diag(diagnostics, rel_path, line_no, "formula CID mismatch")
        return None

    payload_cid = cid_of_json(payload)
    if emitted_payload_cid is not None and not CID_RE.fullmatch(emitted_payload_cid):
        _contract_comment_diag(diagnostics, rel_path, line_no, "malformed payload CID")
        return None
    if emitted_payload_cid is not None and emitted_payload_cid != payload_cid:
        _contract_comment_diag(diagnostics, rel_path, line_no, "payload CID mismatch")
        return None

    extension_fields = {
        "concept_site_cid": payload["concept_site_cid"],
        "contract_cid": payload["contract_cid"],
        "ir_formula_jcs_cid": payload["ir_formula_jcs_cid"],
        "loss_record_cid": payload["loss_record_cid"],
        "payload_cid": payload_cid,
        "policy_cid": payload["policy_cid"],
        "sugar_dict_cid": payload["sugar_dict_cid"],
        "surface": "contract-comment-sugar",
    }
    if isinstance(local_contract_cid, str):
        extension_fields["local_contract_cid"] = local_contract_cid
    return {
        "confidence_basis_points": 10000,
        "extension_fields": extension_fields,
        "predicate": predicate,
        "predicate_text": fol_text,
        "role": CONTRACT_COMMENT_ROLE_MAP[role],
        "source_kind": "native-surface",
    }


def _concept_citation_comments(
    lines: list[str],
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    rel_path: str,
    diagnostics: list[Json],
) -> list[Json]:
    """Return concept-citation artifacts separately from contract witnesses."""
    citations: list[Json] = []
    citations.extend(
        _concept_citation_comments_from_surface_lines(
            _leading_contract_comment_surface(lines, node.lineno),
            rel_path,
            diagnostics,
        )
    )
    citations.extend(
        _concept_citation_comments_from_surface_lines(
            _function_body_comment_surface(lines, node),
            rel_path,
            diagnostics,
        )
    )
    return citations


def _function_body_comment_surface(
    lines: list[str],
    node: ast.FunctionDef | ast.AsyncFunctionDef,
) -> list[tuple[int, str]]:
    end_lineno = getattr(node, "end_lineno", node.lineno)
    surface: list[tuple[int, str]] = []
    for idx in range(node.lineno, min(end_lineno, len(lines))):
        stripped = lines[idx].strip()
        if stripped.startswith("#"):
            surface.append((idx + 1, stripped[1:].strip()))
    return surface


def _trivia_comment_occurrences(
    lines: list[str],
    node: ast.FunctionDef | ast.AsyncFunctionDef,
) -> list[_CommentOccurrence]:
    end_lineno = getattr(node, "end_lineno", node.lineno)
    occurrences: list[_CommentOccurrence] = []
    for idx in range(node.lineno, min(end_lineno, len(lines))):
        stripped = lines[idx].strip()
        if not stripped.startswith("#"):
            continue
        surface = stripped[1:].strip()
        if _is_provekit_comment_carrier(surface):
            continue
        occurrences.append(_CommentOccurrence(line_no=idx + 1, surface=surface))
    return occurrences


def _is_provekit_comment_carrier(surface: str) -> bool:
    normalized = surface.strip()
    carrier_prefixes = (
        "provekit:concept:",
        "provekit:concept-payload-cid:",
        "provekit-concept:",
        "provekit-concept-payload-cid:",
        "provekit-contract:",
        "provekit-contract-payload-cid:",
    )
    return any(normalized.startswith(prefix) for prefix in carrier_prefixes)


def _concept_citation_comments_from_surface_lines(
    surface_lines: list[tuple[int, str]],
    rel_path: str,
    diagnostics: list[Json],
) -> list[Json]:
    citations: list[Json] = []
    idx = 0
    while idx < len(surface_lines):
        line_no, content = surface_lines[idx]
        if content.startswith("provekit-concept-payload-cid:"):
            _concept_citation_diag(
                diagnostics,
                rel_path,
                line_no,
                "concept-citation:orphan-cid-line",
                "payload CID line has no preceding payload",
            )
            idx += 1
            continue
        if not content.startswith("provekit-concept: "):
            idx += 1
            continue
        raw_payload = content[len("provekit-concept: ") :].strip()
        payload_cid: str | None = None
        if idx + 1 < len(surface_lines):
            next_line_no, next_content = surface_lines[idx + 1]
            if (
                next_line_no == line_no + 1
                and next_content.startswith("provekit-concept-payload-cid: ")
            ):
                payload_cid = next_content[
                    len("provekit-concept-payload-cid: ") :
                ].strip()
                idx += 1
        citation = _concept_citation_witness(
            raw_payload,
            payload_cid,
            rel_path,
            line_no,
            diagnostics,
        )
        if citation is not None:
            citations.append(citation)
        idx += 1
    return citations


def _concept_citation_witness(
    raw_payload: str,
    emitted_payload_cid: str | None,
    rel_path: str,
    line_no: int,
    diagnostics: list[Json],
) -> Json | None:
    try:
        payload = json.loads(raw_payload)
    except json.JSONDecodeError as exc:
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:malformed-json",
            f"malformed JSON: {exc.msg}",
        )
        return None
    if not isinstance(payload, dict):
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:malformed-json",
            "payload is not an object",
        )
        return None

    if payload.get("artifact_kind") != CONCEPT_CITATION_COMMENT_KIND:
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:unknown-schema-version",
            "wrong artifact_kind",
        )
        return None
    if payload.get("schema_version") != "1":
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:unknown-schema-version",
            "unknown schema_version",
        )
        return None
    if not _valid_concept_emitted_by(payload.get("emitted_by")):
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:malformed-cid",
            "malformed emitted_by",
        )
        return None

    operation_kind = payload.get("operation_kind")
    if not isinstance(operation_kind, str) or not operation_kind:
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:malformed-json",
            "missing operation_kind",
        )
        return None
    term_position = payload.get("term_position")
    if not _valid_term_position(term_position):
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:malformed-json",
            "malformed term_position",
        )
        return None

    cid_fields = [
        "args_jcs_cid",
        "concept_cid",
        "concept_site_cid",
        "loss_record_cid",
        "shape_cid",
        "sugar_dict_cid",
    ]
    for key in cid_fields:
        value = payload.get(key)
        if not isinstance(value, str) or CID_RE.fullmatch(value) is None:
            _concept_citation_diag(
                diagnostics,
                rel_path,
                line_no,
                "concept-citation:malformed-cid",
                f"malformed {key}",
            )
            return None
    for key in ("callsite_cid", "policy_cid"):
        value = payload.get(key)
        if value is not None and (
            not isinstance(value, str) or CID_RE.fullmatch(value) is None
        ):
            _concept_citation_diag(
                diagnostics,
                rel_path,
                line_no,
                "concept-citation:malformed-cid",
                f"malformed {key}",
            )
            return None
    if emitted_payload_cid is None:
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:payload-cid-mismatch",
            "missing payload CID",
        )
        return None
    if CID_RE.fullmatch(emitted_payload_cid) is None:
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:malformed-cid",
            "malformed payload CID",
        )
        return None

    payload_cid = cid_of_json(payload)
    if emitted_payload_cid != payload_cid:
        _concept_citation_diag(
            diagnostics,
            rel_path,
            line_no,
            "concept-citation:payload-cid-mismatch",
            "payload CID mismatch",
        )
        return None

    args_jcs = payload.get("args_jcs")
    if args_jcs is not None:
        if not isinstance(args_jcs, list):
            _concept_citation_diag(
                diagnostics,
                rel_path,
                line_no,
                "concept-citation:malformed-json",
                "malformed args_jcs",
            )
            return None
        if cid_of_json(args_jcs) != payload["args_jcs_cid"]:
            _concept_citation_diag(
                diagnostics,
                rel_path,
                line_no,
                "concept-citation:args-cid-mismatch",
                "args CID mismatch",
            )
            return None

    catalog = _concept_shape_catalog()
    if catalog is not None:
        catalog_entry = catalog.get(payload["concept_cid"])
        if catalog_entry is None:
            _concept_citation_diag(
                diagnostics,
                rel_path,
                line_no,
                "concept-citation:unknown-concept",
                "concept not in local catalog",
            )
            return None
        expected_shape_cid, expected_operation_kind = catalog_entry
        if expected_shape_cid != payload["shape_cid"]:
            raise _ConceptCitationRefusal(
                "concept-citation:shape-mismatch",
                rel_path,
                line_no,
                "shape CID mismatch",
            )
        if expected_operation_kind != operation_kind:
            raise _ConceptCitationRefusal(
                "concept-citation:operation-kind-mismatch",
                rel_path,
                line_no,
                "operation_kind mismatch",
            )

    extension_fields = {
        "args_jcs_cid": payload["args_jcs_cid"],
        "concept_site_cid": payload["concept_site_cid"],
        "loss_record_cid": payload["loss_record_cid"],
        "payload_cid": payload_cid,
        "shape_cid": payload["shape_cid"],
        "sugar_dict_cid": payload["sugar_dict_cid"],
        "surface": "concept-citation-comment-sugar",
    }
    for key in ("callsite_cid", "policy_cid"):
        if isinstance(payload.get(key), str):
            extension_fields[key] = payload[key]
    if args_jcs is not None:
        extension_fields["args_jcs"] = args_jcs
    return {
        "args_jcs_cid": payload["args_jcs_cid"],
        "artifact_kind": CONCEPT_CITATION_COMMENT_KIND,
        "col": 0,
        "confidence_basis_points": 10000,
        "concept_cid": payload["concept_cid"],
        "extension_fields": extension_fields,
        "line": line_no,
        "operation_kind": operation_kind,
        "shape_cid": payload["shape_cid"],
        "source_kind": "native-surface",
        "term_position": term_position,
    }


def _valid_concept_emitted_by(value: Json) -> bool:
    if not isinstance(value, dict):
        return False
    kit_cid = value.get("kit_cid")
    kit_id = value.get("kit_id")
    kit_kind = value.get("kit_kind")
    target_language = value.get("target_language")
    target_library_tag = value.get("target_library_tag")
    return (
        isinstance(kit_cid, str)
        and CID_RE.fullmatch(kit_cid) is not None
        and isinstance(kit_id, str)
        and bool(kit_id)
        and isinstance(kit_kind, str)
        and bool(kit_kind)
        and isinstance(target_language, str)
        and bool(target_language)
        and (target_library_tag is None or isinstance(target_library_tag, str))
    )


def _valid_term_position(value: Json) -> bool:
    if not isinstance(value, list):
        return False
    return all(
        isinstance(item, int) and not isinstance(item, bool) and item >= 0
        for item in value
    )


def _concept_citation_diag(
    diagnostics: list[Json],
    rel_path: str,
    line_no: int,
    category: str,
    message: str,
) -> None:
    diagnostics.append(
        {
            "kind": category,
            "message": message,
            "path": rel_path,
            "line": line_no,
        }
    )


def _concept_op_cid(name: str) -> str | None:
    return _concept_op_cids_by_name().get(name)


@lru_cache(maxsize=1)
def _concept_op_cids_by_name() -> dict[str, str]:
    root = _repo_root()
    if root is None:
        return {}
    index_path = root / "menagerie/concept-shapes/catalog/index.json"
    try:
        index = json.loads(index_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    entries = index.get("entries")
    if not isinstance(entries, dict):
        return {}
    cids: dict[str, str] = {}
    for cid, meta in entries.items():
        if not isinstance(cid, str) or CID_RE.fullmatch(cid) is None:
            continue
        if not isinstance(meta, dict) or meta.get("kind") != "algorithm":
            continue
        name = meta.get("name")
        if isinstance(name, str) and name.startswith("concept:"):
            meta_cid = meta.get("cid")
            cids[name] = (
                meta_cid
                if isinstance(meta_cid, str) and CID_RE.fullmatch(meta_cid) is not None
                else cid
            )
    return cids


@lru_cache(maxsize=1)
def _concept_shape_catalog() -> dict[str, tuple[str, str]] | None:
    root = _repo_root()
    if root is None:
        return None
    index_path = root / "menagerie/concept-shapes/catalog/index.json"
    try:
        index = json.loads(index_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    entries = index.get("entries")
    if not isinstance(entries, dict):
        return None
    catalog: dict[str, tuple[str, str]] = {}
    catalog_root = index_path.parent
    for cid, meta in entries.items():
        if not isinstance(cid, str) or CID_RE.fullmatch(cid) is None:
            continue
        if not isinstance(meta, dict) or meta.get("kind") != "algorithm":
            continue
        name = meta.get("name")
        rel_path = meta.get("path")
        if not isinstance(name, str) or not name.startswith("concept:"):
            continue
        if not isinstance(rel_path, str):
            continue
        try:
            document = json.loads((catalog_root / rel_path).read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        memento = document.get("memento")
        if not isinstance(memento, dict):
            continue
        operation_kind = _catalog_operation_kind(name, memento)
        shape_cid = document.get("cid")
        if operation_kind and isinstance(shape_cid, str) and CID_RE.fullmatch(shape_cid):
            catalog[cid] = (shape_cid, operation_kind)
    return catalog


def _catalog_operation_kind(name: str, memento: dict[str, Json]) -> str | None:
    post = memento.get("post")
    if isinstance(post, dict):
        operator = post.get("operator")
        if isinstance(operator, str) and operator:
            return operator
    if name.startswith("concept:"):
        return name.removeprefix("concept:")
    return None


def _repo_root() -> Path | None:
    for candidate in Path(__file__).resolve().parents:
        if (candidate / "menagerie/concept-shapes/catalog/index.json").exists():
            return candidate
    return None


def _valid_emitted_by(value: Json) -> bool:
    if not isinstance(value, dict):
        return False
    kit_cid = value.get("kit_cid")
    kit_kind = value.get("kit_kind")
    target_language = value.get("target_language")
    return (
        isinstance(kit_cid, str)
        and CID_RE.fullmatch(kit_cid) is not None
        and isinstance(kit_kind, str)
        and bool(kit_kind)
        and isinstance(target_language, str)
        and bool(target_language)
    )


def _valid_formula_shape(formula: Json) -> bool:
    if not isinstance(formula, dict):
        return False
    kind = formula.get("kind")
    if kind == "atomic":
        return isinstance(formula.get("name"), str) and isinstance(
            formula.get("args"),
            list,
        )
    if kind in {"and", "or", "not", "implies"}:
        operands = formula.get("operands")
        return isinstance(operands, list) and all(
            _valid_formula_shape(operand) for operand in operands
        )
    if kind in {"forall", "exists"}:
        return (
            isinstance(formula.get("name"), str)
            and isinstance(formula.get("sort"), dict)
            and _valid_formula_shape(formula.get("body"))
        )
    return False


def _contract_comment_diag(
    diagnostics: list[Json],
    rel_path: str,
    line_no: int,
    message: str,
) -> None:
    diagnostics.append(
        {
            "kind": "contract-comment-invalid",
            "message": message,
            "path": rel_path,
            "line": line_no,
        }
    )


def _decorator_contract_witnesses(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    param_names: list[str],
    rel_path: str,
    diagnostics: list[Json],
) -> list[Json]:
    witnesses: list[Json] = []
    for decorator in node.decorator_list:
        if not isinstance(decorator, ast.Call):
            continue
        if _decorator_name(decorator.func) not in {"contract", "provekit_contract"}:
            continue
        for keyword in decorator.keywords:
            role = {"pre": "pre", "post": "post", "inv": "inv"}.get(keyword.arg or "")
            if role is None or not isinstance(keyword.value, ast.Constant):
                continue
            if not isinstance(keyword.value.value, str):
                continue
            text = keyword.value.value
            try:
                names = [*param_names, "out"] if role == "post" else param_names
                formula = _parse_expr_string(text, names)
                predicate = json.loads(encode_jcs(formula_to_value(formula)))
            except Exception as exc:
                diagnostics.append(
                    {
                        "kind": "decorator-contract-invalid",
                        "message": str(exc),
                        "path": rel_path,
                        "line": getattr(decorator, "lineno", node.lineno),
                    }
                )
                continue
            witnesses.append(
                {
                    "confidence_basis_points": 10000,
                    "extension_fields": {
                        "decorator": _decorator_name(decorator.func),
                        "surface": "python-decorator-contract",
                    },
                    "predicate": predicate,
                    "predicate_text": text,
                    "role": role,
                    "source_kind": "native-surface",
                }
            )
    return witnesses


def _decorator_name(node: ast.expr) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return node.attr
    return ""


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
