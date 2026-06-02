# SPDX-License-Identifier: Apache-2.0
#
# Production-side WP walker for Python.
#
# This mirrors the Rust `provekit-walk` MVP shape:
#   1. lift a callee's implicit precondition from defensive source patterns;
#   2. substitute actual arguments at each production callsite;
#   3. walk backward through in-scope assignments using wp(x := e, P) = P[e/x];
#   4. emit each arrival as a pre/post edge plus an implication declaration.

from __future__ import annotations

import ast
import os
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Sequence, Set, Tuple

from .ir import (
    ContractDecl,
    Formula,
    Term,
    and_,
    atomic,
    bool_const,
    comparison_with_none_guard,
    connective,
    ctor,
    eq,
    gt,
    gte,
    lt,
    lte,
    make_var,
    ne,
    not_,
    num,
    or_,
    str_const,
    subst_var_in_formula,
)
from .layer2 import ImplicationDecl, LiftWarning


@dataclass
class ProductionWalkOutput:
    decls: List[ContractDecl] = field(default_factory=list)
    implications: List[ImplicationDecl] = field(default_factory=list)
    warnings: List[LiftWarning] = field(default_factory=list)


@dataclass(frozen=True)
class _FunctionPrecondition:
    name: str
    formals: List[str]
    precondition: Formula


@dataclass(frozen=True)
class _Binding:
    name: str
    term: Term
    lineno: int
    col: int


@dataclass(frozen=True)
class _CallsiteHit:
    callee: str
    args: List[ast.expr]
    lineno: int
    col: int
    stmt_index: int
    conditions: List[Formula]
    preceding_inner_stmts: List[ast.stmt]


_COMPARE_OP_MAP = {
    ast.Eq: "=",
    ast.NotEq: "≠",
    ast.Lt: "<",
    ast.LtE: "≤",
    ast.Gt: ">",
    ast.GtE: "≥",
    ast.Is: "=",
    ast.IsNot: "≠",
}

_BINOP_TERM_NAMES = {
    ast.Add: "+",
    ast.Sub: "-",
    ast.Mult: "*",
    ast.Div: "/",
    ast.FloorDiv: "//",
    ast.Mod: "%",
    ast.Pow: "**",
}


def lift_production_walk(source: str, source_path: str) -> ProductionWalkOutput:
    out = ProductionWalkOutput()
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as e:
        out.warnings.append(
            LiftWarning(source_path, "<file>", f"python-wp-walk: failed to parse source: {e}")
        )
        return out

    functions = _top_level_functions(tree)
    callees = {
        name: pre
        for name, fn in functions.items()
        if (pre := _lift_function_precondition(fn)) is not None
    }
    used_names: Set[str] = set()

    for caller in functions.values():
        if _is_test_function(caller.name):
            continue
        for callee in callees.values():
            if callee.name == caller.name:
                continue
            _emit_walks_for_callee(caller, callee, source_path, out, used_names)

    return out


def _top_level_functions(tree: ast.Module) -> Dict[str, ast.FunctionDef]:
    out: Dict[str, ast.FunctionDef] = {}
    for item in tree.body:
        if isinstance(item, ast.FunctionDef):
            out[item.name] = item
    return out


def _is_test_function(name: str) -> bool:
    return name.startswith("test_") or name.endswith("_test")


def _lift_function_precondition(fn: ast.FunctionDef) -> Optional[_FunctionPrecondition]:
    atoms: List[Formula] = []
    for stmt in fn.body:
        contribution = _stmt_precondition_contribution(stmt)
        if contribution is not None:
            atoms.append(contribution)
    if not atoms:
        return None
    precondition = atoms[0] if len(atoms) == 1 else and_(atoms)
    return _FunctionPrecondition(
        name=fn.name,
        formals=[arg.arg for arg in fn.args.args],
        precondition=precondition,
    )


def _stmt_precondition_contribution(stmt: ast.stmt) -> Optional[Formula]:
    if isinstance(stmt, ast.Assert):
        try:
            return _lift_predicate(stmt.test)
        except ValueError:
            return None
    if isinstance(stmt, ast.If) and not stmt.orelse and _block_only_raises(stmt.body):
        try:
            return _negate_formula(_lift_predicate(stmt.test))
        except ValueError:
            return None
    return None


def _block_only_raises(stmts: Sequence[ast.stmt]) -> bool:
    return len(stmts) == 1 and isinstance(stmts[0], ast.Raise)


def _emit_walks_for_callee(
    caller: ast.FunctionDef,
    callee: _FunctionPrecondition,
    source_path: str,
    out: ProductionWalkOutput,
    used_names: Set[str],
) -> None:
    for hit in _find_callsites(caller, callee.name):
        if len(callee.formals) != len(hit.args):
            out.warnings.append(
                LiftWarning(
                    source_path,
                    caller.name,
                    f"python-wp-walk: arity mismatch at {caller.name}->{callee.name} "
                    f"(formals={len(callee.formals)}, actuals={len(hit.args)})",
                )
            )
            continue

        try:
            wp = callee.precondition
            for formal, actual in zip(callee.formals, hit.args):
                wp = subst_var_in_formula(wp, formal, _term_from_expr(actual))
        except ValueError as e:
            out.warnings.append(
                LiftWarning(
                    source_path,
                    caller.name,
                    f"python-wp-walk: callsite actual not liftable for {callee.name}: {e}",
                )
            )
            continue

        if hit.conditions:
            premise = hit.conditions[0] if len(hit.conditions) == 1 else and_(hit.conditions)
            wp = connective("implies", [premise, wp])

        base = _callsite_base(callee.name, source_path, hit.lineno, hit.col)
        _append_edge(out, used_names, f"{base}::callsite", wp, wp, caller.name, callee.name)
        previous_wp = wp

        for stmt in hit.preceding_inner_stmts:
            for binding in _bindings_from_stmt(stmt):
                next_wp = subst_var_in_formula(previous_wp, binding.name, binding.term)
                _append_edge(
                    out,
                    used_names,
                    f"{base}::let:{binding.name}",
                    next_wp,
                    previous_wp,
                    caller.name,
                    callee.name,
                )
                previous_wp = next_wp

        for stmt in reversed(caller.body[: hit.stmt_index]):
            for binding in _bindings_from_stmt(stmt):
                next_wp = subst_var_in_formula(previous_wp, binding.name, binding.term)
                _append_edge(
                    out,
                    used_names,
                    f"{base}::let:{binding.name}",
                    next_wp,
                    previous_wp,
                    caller.name,
                    callee.name,
                )
                previous_wp = next_wp

        _append_edge(out, used_names, f"{base}::entry", previous_wp, previous_wp, caller.name, callee.name)


def _append_edge(
    out: ProductionWalkOutput,
    used_names: Set[str],
    raw_name: str,
    pre: Formula,
    post: Formula,
    caller_name: str,
    callee_name: str,
) -> None:
    name = _unique_name(raw_name, used_names)
    out.decls.append(ContractDecl(name=name, pre=pre, post=post, out_binding="result"))
    out.implications.append(
        ImplicationDecl(
            name=f"{name}::pre-implies-post",
            antecedent=name,
            consequent=name,
            antecedent_slot="pre",
            consequent_slot="post",
            prover="python-wp-walk",
            proof_witness=f"{caller_name}->{callee_name}",
        )
    )


def _find_callsites(caller: ast.FunctionDef, callee_name: str) -> List[_CallsiteHit]:
    hits: List[_CallsiteHit] = []
    for idx, stmt in enumerate(caller.body):
        _walk_stmt_for_callsites(
            stmt,
            idx,
            callee_name,
            conditions=[],
            inner_stmts=[],
            hits=hits,
        )
    return hits


def _walk_stmt_for_callsites(
    stmt: ast.stmt,
    stmt_index: int,
    callee_name: str,
    conditions: List[Formula],
    inner_stmts: List[ast.stmt],
    hits: List[_CallsiteHit],
) -> None:
    if isinstance(stmt, (ast.Assign, ast.AnnAssign, ast.Expr, ast.Return)):
        expr = _stmt_expr(stmt)
        if expr is not None:
            _walk_expr_for_callsites(expr, stmt_index, callee_name, conditions, inner_stmts, hits)
        return

    if isinstance(stmt, ast.If):
        lifted = _try_lift_predicate(stmt.test)
        if lifted is not None:
            conditions.append(lifted)
        _walk_block_for_callsites(stmt.body, stmt_index, callee_name, conditions, inner_stmts, hits)
        if lifted is not None:
            conditions.pop()

        if lifted is not None:
            conditions.append(_negate_formula(lifted))
        _walk_block_for_callsites(stmt.orelse, stmt_index, callee_name, conditions, inner_stmts, hits)
        if lifted is not None:
            conditions.pop()
        return

    for child in ast.iter_child_nodes(stmt):
        if isinstance(child, ast.expr):
            _walk_expr_for_callsites(child, stmt_index, callee_name, conditions, inner_stmts, hits)


def _walk_block_for_callsites(
    stmts: Sequence[ast.stmt],
    stmt_index: int,
    callee_name: str,
    conditions: List[Formula],
    inner_stmts: List[ast.stmt],
    hits: List[_CallsiteHit],
) -> None:
    for branch_idx, stmt in enumerate(stmts):
        branch_preceding = list(reversed(stmts[:branch_idx])) + list(inner_stmts)
        _walk_stmt_for_callsites(
            stmt,
            stmt_index,
            callee_name,
            conditions,
            branch_preceding,
            hits,
        )


def _walk_expr_for_callsites(
    expr: ast.expr,
    stmt_index: int,
    callee_name: str,
    conditions: List[Formula],
    inner_stmts: List[ast.stmt],
    hits: List[_CallsiteHit],
) -> None:
    if isinstance(expr, ast.Call):
        if isinstance(expr.func, ast.Name) and expr.func.id == callee_name and not expr.keywords:
            hits.append(
                _CallsiteHit(
                    callee=callee_name,
                    args=list(expr.args),
                    lineno=getattr(expr, "lineno", 0),
                    col=getattr(expr, "col_offset", 0),
                    stmt_index=stmt_index,
                    conditions=list(conditions),
                    preceding_inner_stmts=list(inner_stmts),
                )
            )
        for arg in expr.args:
            _walk_expr_for_callsites(arg, stmt_index, callee_name, conditions, inner_stmts, hits)
        return

    for child in ast.iter_child_nodes(expr):
        if isinstance(child, ast.expr):
            _walk_expr_for_callsites(child, stmt_index, callee_name, conditions, inner_stmts, hits)


def _stmt_expr(stmt: ast.stmt) -> Optional[ast.expr]:
    if isinstance(stmt, ast.Assign):
        return stmt.value
    if isinstance(stmt, ast.AnnAssign):
        return stmt.value
    if isinstance(stmt, ast.Expr):
        return stmt.value
    if isinstance(stmt, ast.Return):
        return stmt.value
    return None


def _bindings_from_stmt(stmt: ast.stmt) -> List[_Binding]:
    if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1 and isinstance(stmt.targets[0], ast.Name):
        return [
            _Binding(
                stmt.targets[0].id,
                _term_from_expr(stmt.value),
                getattr(stmt, "lineno", 0),
                getattr(stmt, "col_offset", 0),
            )
        ]
    if isinstance(stmt, ast.AnnAssign) and isinstance(stmt.target, ast.Name) and stmt.value is not None:
        return [
            _Binding(
                stmt.target.id,
                _term_from_expr(stmt.value),
                getattr(stmt, "lineno", 0),
                getattr(stmt, "col_offset", 0),
            )
        ]
    return []


def _try_lift_predicate(node: ast.expr) -> Optional[Formula]:
    try:
        return _lift_predicate(node)
    except ValueError:
        return None


def _lift_predicate(node: ast.expr) -> Formula:
    if isinstance(node, ast.Compare):
        if len(node.ops) != 1 or len(node.comparators) != 1:
            raise ValueError("only single comparisons are liftable")
        sym = _COMPARE_OP_MAP.get(type(node.ops[0]))
        if sym is None:
            raise ValueError(f"unsupported comparison op: {type(node.ops[0]).__name__}")
        return comparison_with_none_guard(
            sym, _term_from_expr(node.left), _term_from_expr(node.comparators[0])
        )
    if isinstance(node, ast.BoolOp):
        operands = [_lift_predicate(value) for value in node.values]
        if isinstance(node.op, ast.And):
            return and_(operands)
        if isinstance(node.op, ast.Or):
            return or_(operands)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not):
        return _negate_formula(_lift_predicate(node.operand))
    return eq(_term_from_expr(node), bool_const(True))


def _term_from_expr(node: ast.expr) -> Term:
    if isinstance(node, ast.Name):
        if node.id == "True":
            return bool_const(True)
        if node.id == "False":
            return bool_const(False)
        if node.id == "None":
            return ctor("None", [])
        return make_var(node.id)
    if isinstance(node, ast.Constant):
        value = node.value
        if isinstance(value, bool):
            return bool_const(value)
        if isinstance(value, int):
            return num(value)
        if isinstance(value, str):
            return str_const(value)
        if value is None:
            return ctor("None", [])
        return _placeholder_term(node)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        if isinstance(node.operand, ast.Constant) and isinstance(node.operand.value, int):
            return num(-node.operand.value)
        return ctor("neg", [_term_from_expr(node.operand)])
    if isinstance(node, ast.BinOp):
        name = _BINOP_TERM_NAMES.get(type(node.op))
        if name is None:
            return _placeholder_term(node)
        return ctor(name, [_term_from_expr(node.left), _term_from_expr(node.right)])
    if isinstance(node, ast.Call):
        if not isinstance(node.func, ast.Name) or node.keywords:
            return _placeholder_term(node)
        return ctor(node.func.id, [_term_from_expr(arg) for arg in node.args])
    if isinstance(node, ast.Attribute):
        return ctor("field", [_term_from_expr(node.value), str_const(node.attr)])
    if isinstance(node, ast.Subscript):
        return ctor("index", [_term_from_expr(node.value), _term_from_expr(node.slice)])
    return _placeholder_term(node)


def _negate_formula(formula: Formula) -> Formula:
    if _is_atomic(formula, "<"):
        return gte(formula.args[0], formula.args[1])  # type: ignore[attr-defined]
    if _is_atomic(formula, "≤"):
        return gt(formula.args[0], formula.args[1])  # type: ignore[attr-defined]
    if _is_atomic(formula, ">"):
        return lte(formula.args[0], formula.args[1])  # type: ignore[attr-defined]
    if _is_atomic(formula, "≥"):
        return lt(formula.args[0], formula.args[1])  # type: ignore[attr-defined]
    if _is_atomic(formula, "="):
        return ne(formula.args[0], formula.args[1])  # type: ignore[attr-defined]
    if _is_atomic(formula, "≠"):
        return eq(formula.args[0], formula.args[1])  # type: ignore[attr-defined]
    return not_(formula)


def _is_atomic(formula: Formula, name: str) -> bool:
    return getattr(formula, "name", None) == name and hasattr(formula, "args")


def _callsite_base(callee: str, source_path: str, lineno: int, col: int) -> str:
    file_name = os.path.basename(source_path) or source_path or "<unknown>"
    return f"{callee}@{file_name}:{lineno}:{col}"


def _placeholder_term(node: ast.AST) -> Term:
    return make_var(f"<expr:{_unparse(node)}>")


def _unique_name(name: str, used_names: Set[str]) -> str:
    if name not in used_names:
        used_names.add(name)
        return name
    i = 1
    while f"{name}::{i}" in used_names:
        i += 1
    unique = f"{name}::{i}"
    used_names.add(unique)
    return unique


def _unparse(node: ast.AST) -> str:
    try:
        return ast.unparse(node)  # type: ignore[attr-defined]
    except Exception:
        return node.__class__.__name__
