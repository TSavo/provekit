# SPDX-License-Identifier: Apache-2.0
#
# provekit.decorators: direct contract authoring for Python.
#
# Usage:
#   @provekit.contract(pre=lambda x: x >= 0, post=lambda out: out >= 0)
#   def abs(x: int) -> int:
#       return x if x >= 0 else -x
#
# The decorator captures the contract metadata and registers it with the
# kit collector. When the module is loaded, the contracts are available
# for lifting via provekit.lift or for direct verification.

from __future__ import annotations

import ast
import functools
import inspect
import textwrap
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Union

from .ir import (
    ContractDecl,
    Formula,
    Int,
    String,
    Bool,
    Sort,
    atomic,
    eq,
    ne,
    gt,
    gte,
    lt,
    lte,
    make_var,
    num,
    str_const,
    bool_const,
    ctor,
    and_,
    or_,
    not_,
    implies,
)


# ---------------------------------------------------------------------------
# Contract decorator
# ---------------------------------------------------------------------------


def contract(
    *,
    pre: Optional[Union[Callable[..., bool], str]] = None,
    post: Optional[Union[Callable[..., bool], str]] = None,
    inv: Optional[Union[Callable[..., bool], str]] = None,
    out_binding: str = "out",
) -> Callable[[Callable], Callable]:
    """Decorate a function with a ProvekIt contract.

    The ``pre``, ``post``, and ``inv`` arguments accept either:
      - A Python callable (lambda or function) that the decorator introspects
        to build an IR formula.
      - A string containing a Python boolean expression (e.g. ``"x >= 0"``).

    Example:
        @provekit.contract(pre="x >= 0", post="out >= 0")
        def sqrt(x: float) -> float:
            return x ** 0.5
    """

    def decorator(fn: Callable) -> Callable:
        sig = inspect.signature(fn)
        param_names = list(sig.parameters.keys())

        pre_ir = _parse_contract_expr(pre, param_names, "pre") if pre else None
        post_ir = (
            _parse_contract_expr(post, param_names + [out_binding], "post")
            if post
            else None
        )
        inv_ir = _parse_contract_expr(inv, param_names, "inv") if inv else None

        # Store metadata on the function object for later collection.
        fn._provekit_contract = ContractDecl(  # type: ignore
            name=fn.__qualname__,
            pre=pre_ir,
            post=post_ir,
            inv=inv_ir,
            out_binding=out_binding,
        )

        @functools.wraps(fn)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            # Runtime contract checking (optional, lightweight).
            bound = sig.bind(*args, **kwargs)
            bound.apply_defaults()
            if pre and callable(pre):
                _check_runtime(pre, bound.arguments, "precondition")
            result = fn(*args, **kwargs)
            if post and callable(post):
                post_args = dict(bound.arguments)
                post_args[out_binding] = result
                _check_runtime(post, post_args, "postcondition")
            return result

        wrapper._provekit_contract = fn._provekit_contract  # type: ignore
        return wrapper

    return decorator


def _check_runtime(
    predicate: Callable[..., bool],
    args: Dict[str, Any],
    kind: str,
) -> None:
    """Invoke a runtime predicate and raise ContractViolation on failure."""
    sig = inspect.signature(predicate)
    call_args = {name: args[name] for name in sig.parameters if name in args}
    try:
        ok = predicate(**call_args)
    except Exception as e:
        raise ContractViolation(f"{kind} predicate raised {type(e).__name__}: {e}")
    if not ok:
        raise ContractViolation(f"{kind} violated")


class ContractViolation(Exception):
    """Raised when a runtime contract check fails."""

    pass


# ---------------------------------------------------------------------------
# Expression parser: Python source -> IR Formula
# ---------------------------------------------------------------------------


def _parse_contract_expr(
    expr: Union[Callable[..., bool], str],
    available_names: List[str],
    kind: str,
) -> Optional[Formula]:
    """Parse a Python expression into a canonical IR Formula."""
    if isinstance(expr, str):
        return _parse_expr_string(expr, available_names)
    if callable(expr):
        return _parse_callable(expr, available_names)
    return None


def _parse_expr_string(expr: str, available_names: List[str]) -> Formula:
    """Parse a string expression like ``x >= 0 && y != null``."""
    source = textwrap.dedent(expr).strip()
    tree = ast.parse(source, mode="eval")
    return _translate_expr(tree.body, available_names)


def _parse_callable(fn: Callable[..., bool], available_names: List[str]) -> Formula:
    """Introspect a lambda/function AST to extract its body expression."""
    try:
        source = inspect.getsource(fn)
    except OSError:
        # Fallback: if source unavailable, treat as opaque.
        return atomic("py_predicate", [str_const(fn.__name__)])
    source = textwrap.dedent(source).strip()
    tree = ast.parse(source)
    # Search for Lambda first (separate pass so we don't accidentally
    # match a nested FunctionDef from enclosing scope).
    for node in ast.walk(tree):
        if isinstance(node, ast.Lambda):
            return _translate_expr(node.body, available_names)
    # Then search for FunctionDef.
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.body and isinstance(node.body[-1], ast.Return):
                return _translate_expr(node.body[-1].value, available_names)
            if len(node.body) == 1 and isinstance(node.body[0], ast.Expr):
                return _translate_expr(node.body[0].value, available_names)
    # Fallback: opaque predicate
    return atomic("py_predicate", [str_const(fn.__name__)])


_COMPARE_OPS = {
    ast.Eq: "=",
    ast.NotEq: "≠",
    ast.Lt: "<",
    ast.LtE: "≤",
    ast.Gt: ">",
    ast.GtE: "≥",
    ast.Is: "=",
    ast.IsNot: "≠",
}

_BOOL_OPS = {
    ast.And: "and",
    ast.Or: "or",
}


def _translate_expr(node: ast.expr, available_names: List[str]) -> Formula:
    """Translate a Python AST expression node into an IR Formula."""
    if isinstance(node, ast.BoolOp):
        operands = [_translate_expr(v, available_names) for v in node.values]
        kind = _BOOL_OPS.get(type(node.op))
        if kind == "and":
            return and_(operands)
        if kind == "or":
            return or_(operands)
        raise ValueError(f"unsupported bool op: {type(node.op).__name__}")

    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not):
        inner = _translate_expr(node.operand, available_names)
        return not_(inner)

    if isinstance(node, ast.Compare):
        if len(node.ops) != 1 or len(node.comparators) != 1:
            raise ValueError("chained comparisons are not supported")
        op = node.ops[0]
        sym = _COMPARE_OPS.get(type(op))
        if sym is None:
            raise ValueError(f"unsupported comparison: {type(op).__name__}")
        l = _translate_term(node.left, available_names)
        r = _translate_term(node.comparators[0], available_names)
        return atomic(sym, [l, r])

    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Add):
        l = _translate_term(node.left, available_names)
        r = _translate_term(node.right, available_names)
        return eq(ctor("+", [l, r]), ctor("+", [l, r]))  # placeholder

    # Single term treated as truthiness assertion.
    t = _translate_term(node, available_names)
    return eq(t, bool_const(True))


def _translate_term(node: ast.expr, available_names: List[str]):
    """Translate a Python AST expression node into an IR Term."""
    from .ir import Term

    if isinstance(node, ast.Name):
        if node.id == "None":
            return ctor("None", [])
        if node.id == "True":
            return bool_const(True)
        if node.id == "False":
            return bool_const(False)
        return make_var(node.id)

    if isinstance(node, ast.Constant):
        v = node.value
        if isinstance(v, bool):
            return bool_const(v)
        if isinstance(v, int):
            return num(v)
        if isinstance(v, str):
            return str_const(v)
        if v is None:
            return ctor("None", [])
        raise ValueError(f"unsupported constant: {type(v).__name__}")

    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        if isinstance(node.operand, ast.Constant) and isinstance(
            node.operand.value, int
        ):
            return num(-node.operand.value)
        raise ValueError("unary minus only supported on integer literals")

    if isinstance(node, ast.Call):
        if not isinstance(node.func, ast.Name):
            raise ValueError("only simple-name calls are supported")
        args = [_translate_term(a, available_names) for a in node.args]
        return ctor(node.func.id, args)

    if isinstance(node, ast.BinOp):
        if isinstance(node.op, ast.Add):
            return ctor(
                "+",
                [
                    _translate_term(node.left, available_names),
                    _translate_term(node.right, available_names),
                ],
            )
        if isinstance(node.op, ast.Sub):
            return ctor(
                "-",
                [
                    _translate_term(node.left, available_names),
                    _translate_term(node.right, available_names),
                ],
            )
        if isinstance(node.op, ast.Mult):
            return ctor(
                "*",
                [
                    _translate_term(node.left, available_names),
                    _translate_term(node.right, available_names),
                ],
            )
        if isinstance(node.op, ast.Div):
            return ctor(
                "/",
                [
                    _translate_term(node.left, available_names),
                    _translate_term(node.right, available_names),
                ],
            )
        raise ValueError(f"unsupported binary op: {type(node.op).__name__}")

    raise ValueError(f"unsupported expression: {type(node).__name__}")


# ---------------------------------------------------------------------------
# Collector: gather all decorated functions in a module
# ---------------------------------------------------------------------------


def collect_module(module) -> List[ContractDecl]:
    """Collect all @provekit.contract declarations from a loaded module."""
    decls: List[ContractDecl] = []
    for name in dir(module):
        obj = getattr(module, name)
        if hasattr(obj, "_provekit_contract"):
            decls.append(obj._provekit_contract)
    return decls
