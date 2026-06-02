from __future__ import annotations

import ast
from typing import Any

Json = dict[str, Any]


def compile_ir_document(ir: list[Json]) -> str:
    source = _source_unit_bytes(ir)
    if source is not None:
        return source

    functions = [_compile_contract(contract) for contract in ir if _is_function_contract(contract)]
    module = ast.Module(body=functions, type_ignores=[])
    ast.fix_missing_locations(module)
    text = ast.unparse(module)
    return text + ("\n" if text else "")


def compile_body_term(term: Json, *, fn_name: str = "f", formals: list[str] | None = None) -> str:
    contract = {
        "kind": "function-contract",
        "fnName": fn_name,
        "formals": list(formals or []),
        "post": {"args": [None, term]},
    }
    module = ast.Module(body=[_compile_contract(contract)], type_ignores=[])
    ast.fix_missing_locations(module)
    text = ast.unparse(module)
    return text + ("\n" if text else "")


def _source_unit_bytes(ir: list[Json]) -> str | None:
    for contract in ir:
        if not _is_function_contract(contract):
            continue
        term = _contract_term(contract)
        if _name(term) != "python:source-unit":
            continue
        args = term.get("args", [])
        if args and isinstance(args[0], dict) and args[0].get("kind") == "const":
            value = args[0].get("value")
            if isinstance(value, str):
                return value
    return None


def _compile_contract(contract: Json) -> ast.FunctionDef:
    fn_name = _source_function_name(str(contract["fnName"]))
    formals = [
        ast.arg(arg=str(name), annotation=None, type_comment=None)
        for name in contract.get("formals", [])
    ]
    body = _stmt_list(_contract_term(contract))
    if not body:
        body = [ast.Pass()]
    return ast.FunctionDef(
        name=fn_name,
        args=ast.arguments(
            posonlyargs=[],
            args=formals,
            vararg=None,
            kwonlyargs=[],
            kw_defaults=[],
            kwarg=None,
            defaults=[],
        ),
        body=body,
        decorator_list=[],
        returns=None,
        type_comment=None,
    )


def _stmt_list(term: Json) -> list[ast.stmt]:
    if _name(term) == "python:seq":
        args = term.get("args", [])
        return _stmt_list(args[0]) + _stmt_list(args[1])
    return [_stmt(term)]


def _stmt(term: Json) -> ast.stmt:
    name = _name(term)
    args = term.get("args", [])
    if name == "python:assign":
        return ast.Assign(targets=[_target(args[0])], value=_expr(args[1]))
    if name == "python:return":
        value = None if _is_none_const(args[0]) else _expr(args[0])
        return ast.Return(value=value)
    if name == "python:if":
        return ast.If(
            test=_expr(args[0]),
            body=_stmt_list(args[1]) or [ast.Pass()],
            orelse=[] if _name(args[2]) == "python:pass" else _stmt_list(args[2]),
        )
    if name == "cf_ite":
        then_branch = _unguarded(args[1])
        else_branch = _unguarded(args[2])
        return ast.If(
            test=_expr(args[0]),
            body=_stmt_list(then_branch) or [ast.Pass()],
            orelse=[] if _name(else_branch) == "python:pass" else _stmt_list(else_branch),
        )
    if name == "python:while":
        return ast.While(test=_expr(args[0]), body=_stmt_list(args[1]), orelse=[])
    if name == "python:for":
        return ast.For(
            target=_target(args[0]),
            iter=_expr(args[1]),
            body=_stmt_list(args[2]),
            orelse=[],
            type_comment=None,
        )
    if name == "python:expr":
        return ast.Expr(value=_expr(args[0]))
    if name == "python:pass":
        return ast.Pass()
    if name == "python:break":
        return ast.Break()
    if name == "python:continue":
        return ast.Continue()
    if name == "python:raise":
        return ast.Raise(exc=None if _is_none_const(args[0]) else _expr(args[0]), cause=None)
    return ast.Expr(value=_expr(term))


def _expr(term: Json) -> ast.expr:
    kind = term.get("kind")
    if kind == "const":
        return ast.Constant(value=term.get("value"))
    if kind == "var":
        return ast.Name(id=str(term.get("name", "x")), ctx=ast.Load())
    if kind != "ctor":
        raise ValueError(f"unsupported term kind: {kind}")

    name = _name(term)
    args = term.get("args", [])
    if name in _BINOPS:
        return ast.BinOp(left=_expr(args[0]), op=_BINOPS[name](), right=_expr(args[1]))
    if name in _UNARYOPS:
        return ast.UnaryOp(op=_UNARYOPS[name](), operand=_expr(args[0]))
    if name == "python:and" or name == "python:or":
        op = ast.And() if name == "python:and" else ast.Or()
        return ast.BoolOp(op=op, values=[_expr(args[0]), _expr(args[1])])
    if name == "python:compare":
        return ast.Compare(
            left=_expr(args[1]),
            ops=[_cmpop(_const_string(args[0]))],
            comparators=[_expr(args[2])],
        )
    if name == "python:call":
        return ast.Call(
            func=_dotted_expr(_const_string(args[0])),
            args=[_expr(arg) for arg in args[1:]],
            keywords=[],
        )
    if name == "python:attribute":
        return ast.Attribute(value=_expr(args[0]), attr=_const_string(args[1]), ctx=ast.Load())
    if name == "python:subscript":
        return ast.Subscript(value=_expr(args[0]), slice=_expr(args[1]), ctx=ast.Load())
    raise ValueError(f"unsupported python operation in expression position: {name}")


def _target(term: Json) -> ast.expr:
    expr = _expr(term)
    return _with_context(expr, ast.Store())


def _with_context(expr: ast.expr, ctx: ast.expr_context) -> ast.expr:
    if isinstance(expr, ast.Name):
        expr.ctx = ctx
    elif isinstance(expr, ast.Attribute):
        expr.ctx = ctx
    elif isinstance(expr, ast.Subscript):
        expr.ctx = ctx
    else:
        raise ValueError(f"term is not assignable: {ast.dump(expr)}")
    return expr


def _dotted_expr(name: str) -> ast.expr:
    parts = name.split(".")
    expr: ast.expr = ast.Name(id=parts[0], ctx=ast.Load())
    for part in parts[1:]:
        expr = ast.Attribute(value=expr, attr=part, ctx=ast.Load())
    return expr


def _cmpop(op: str) -> ast.cmpop:
    mapping: dict[str, type[ast.cmpop]] = {
        "==": ast.Eq,
        "!=": ast.NotEq,
        "<": ast.Lt,
        "<=": ast.LtE,
        ">": ast.Gt,
        ">=": ast.GtE,
        "is": ast.Is,
        "is not": ast.IsNot,
        "in": ast.In,
        "not in": ast.NotIn,
    }
    if op not in mapping:
        raise ValueError(f"unsupported comparison operator: {op}")
    return mapping[op]()


def _contract_term(contract: Json) -> Json:
    return contract["post"]["args"][1]


def _source_function_name(fn_name: str) -> str:
    parts = [part for part in fn_name.split(".") if part != "<locals>"]
    return parts[-1] if parts else "f"


def _is_function_contract(value: Json) -> bool:
    return value.get("kind") == "function-contract"


def _name(term: Any) -> str:
    return str(term.get("name", "")) if isinstance(term, dict) else ""


def _unguarded(term: Json) -> Json:
    if _name(term) == "cf_guarded":
        args = term.get("args", [])
        if len(args) == 2:
            return args[1]
    return term


def _const_string(term: Json) -> str:
    if term.get("kind") != "const" or not isinstance(term.get("value"), str):
        raise ValueError(f"expected string const: {term!r}")
    return term["value"]


def _is_none_const(term: Any) -> bool:
    return isinstance(term, dict) and term.get("kind") == "const" and term.get("value") is None


_BINOPS: dict[str, type[ast.operator]] = {
    "python:add": ast.Add,
    "python:sub": ast.Sub,
    "python:mul": ast.Mult,
    "python:div": ast.Div,
    "python:floordiv": ast.FloorDiv,
    "python:mod": ast.Mod,
    "python:pow": ast.Pow,
    "python:lshift": ast.LShift,
    "python:rshift": ast.RShift,
    "python:bitand": ast.BitAnd,
    "python:bitor": ast.BitOr,
    "python:bitxor": ast.BitXor,
}

_UNARYOPS: dict[str, type[ast.unaryop]] = {
    "python:neg": ast.USub,
    "python:pos": ast.UAdd,
    "python:not": ast.Not,
    "python:bitnot": ast.Invert,
}
