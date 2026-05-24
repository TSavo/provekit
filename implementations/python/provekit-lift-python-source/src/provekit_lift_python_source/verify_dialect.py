"""Verify-facing dialect transform for the Python source lifter.

This is the Python analog of Go's ``LiftSourceCore`` / ``NormalizeCoreArith``
(PR #1445). The round-trip source lifter (``lifter.py``) emits a
``function-contract`` whose ``post`` is::

    (= (var return_value) (python:return (python:mul (var x) (const 2 Int))))

with namespaced ops, the ``return_value`` result variable, a ``python:return``
wrapper, and ``Value`` sorts. That form is byte-identical to what the
round-trip / realize pipeline depends on and MUST NOT change on disk.

The verifier's ``body_discharge::CatalogResolver`` instead needs a
z3-dischargeable ``function-contract`` whose ``post`` is::

    (= (var result) (* (var x) (const 2 Int)))

i.e. the SMT result var ``result``, no ``python:return`` wrapper, SMT-core op
symbols (``*``, ``+``, ``<`` ...), and ``Int`` formal/return sorts. This module
performs exactly that transform and REFUSES (returns ``None`` with a reason)
rather than emit an unsound contract.

Supra omnia, rectum: the cardinal rule mirrored from the Go division lesson is
that ``python:div`` / ``python:floordiv`` / ``python:mod`` are LEFT
NAMESPACED (uninterpreted). SMT-LIB ``div``/``mod`` floor toward -inf and
diverge from Python truncation / float semantics on negatives, so mapping them
would let a Python-false assertion discharge with a signed witness. Leaving
them uninterpreted makes the obligation Undecidable instead -- the honest
refusal.
"""

from __future__ import annotations

import ast
from dataclasses import dataclass
from typing import Any

Json = dict[str, Any]

# SMT result variable the verifier's body-discharge seam equates the call's
# result with (matches libprovekit::wp::DEFAULT_RESULT_VAR == "result").
RESULT_VAR = "result"

# KEPT (faithful core): the SMT-LIB symbol for each namespaced arithmetic /
# comparison / boolean op whose Int (or Bool) semantics coincide with the
# SMT-LIB core theory for the value domain we model.
#
# EXCLUDED (deliberately absent -> stay namespaced -> Undecidable):
#   python:div       true division `/`  (float result; not integer SMT div)
#   python:floordiv  floor division `//`
#   python:mod       modulo `%`
#   python:pow, shifts, bitwise ops, is/in comparisons
# All of these have no faithful core mapping for signed ints, so a contract
# that uses them refuses to z3 rather than risk a false discharge.
_CORE_ARITH_OP: dict[str, str] = {
    "python:add": "+",
    "python:sub": "-",
    "python:mul": "*",
    "python:neg": "-",
    "python:not": "not",
    "python:and": "and",
    "python:or": "or",
}

# Comparison operators carried as the string head of a `python:compare` ctor.
_CORE_CMP_OP: dict[str, str] = {
    "==": "=",
    "!=": "≠",
    "<": "<",
    "<=": "≤",
    ">": ">",
    ">=": "≥",
}

# Annotation surfaces that faithfully map to the SMT `Int` sort. Anything else
# (no annotation, `float`, `str`, custom types) refuses for an arithmetic body.
_INT_ANNOTATIONS = {"int"}
_BOOL_ANNOTATIONS = {"bool"}


class VerifyDialectRefusal(Exception):
    """Raised when a lifted function-contract cannot be faithfully lowered to
    the z3-dischargeable verify-facing dialect. The caller turns this into a
    diagnostic and SKIPS emitting a contract -- never an unsound one."""

    def __init__(self, fn_name: str, reason: str):
        self.fn_name = fn_name
        self.reason = reason
        super().__init__(f"{fn_name}: {reason}")


@dataclass(frozen=True)
class _Sorts:
    """SMT sorts for a function's formals + return, derived from the source
    `: int` / `-> int` annotations (the round-trip lifter erases these to
    `Value`)."""

    formal_sorts: dict[str, str]  # formal name -> "Int" | "Bool"
    return_sort: str | None  # "Int" | "Bool" | None


def prim_sort(name: str) -> Json:
    return {"kind": "primitive", "name": name}


def collect_int_signatures(source: str) -> dict[str, _Sorts]:
    """Parse `source` and return, per bare function name, the SMT sorts of its
    annotated formals + return. Only `int` / `bool` annotations are recorded;
    others are omitted (the transform refuses when a needed sort is missing)."""
    out: dict[str, _Sorts] = {}
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return out
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        formal_sorts: dict[str, str] = {}
        for arg in node.args.args:
            sort = _sort_for_annotation(arg.annotation)
            if sort is not None:
                formal_sorts[arg.arg] = sort
        return_sort = _sort_for_annotation(node.returns)
        out[node.name] = _Sorts(formal_sorts=formal_sorts, return_sort=return_sort)
    return out


def _sort_for_annotation(annotation: ast.expr | None) -> str | None:
    if annotation is None:
        return None
    if isinstance(annotation, ast.Name):
        if annotation.id in _INT_ANNOTATIONS:
            return "Int"
        if annotation.id in _BOOL_ANNOTATIONS:
            return "Bool"
    return None


def to_verify_dialect(contract: Json, sorts: _Sorts) -> Json:
    """Lower a round-trip `function-contract` to the z3-dischargeable
    verify-facing dialect, or raise `VerifyDialectRefusal`.

    Steps (mirroring Go LiftSourceCore + the result-var/return-unwrap that Go
    did not need):
      1. Strip the `python:return` wrapper from the post's value expression.
      2. Rename the result var `return_value` -> `result`.
      3. Normalize `python:*` ops to SMT-core symbols; refuse on any op with
         no faithful core mapping (div/mod/floordiv/pow/shifts/...).
      4. Replace the `Value` formal/return sorts with `Int`/`Bool` from the
         source annotations; refuse if a formal touched by arithmetic lacks an
         int/bool annotation.
    """
    fn_name = str(contract.get("fnName", "<unknown>"))
    post = contract.get("post")
    if not isinstance(post, dict) or post.get("name") != "=":
        raise VerifyDialectRefusal(fn_name, "post is not an `=` atomic")
    args = post.get("args")
    if not isinstance(args, list) or len(args) != 2:
        raise VerifyDialectRefusal(fn_name, "post `=` does not have exactly two args")
    lhs, rhs = args[0], args[1]
    if not (isinstance(lhs, dict) and lhs.get("kind") == "var" and lhs.get("name") == "return_value"):
        raise VerifyDialectRefusal(fn_name, "post LHS is not the `return_value` result var")

    # 1. Unwrap the single `python:return(<value>)` the body folds to. A body
    #    that is anything other than exactly one bare return (e.g. a sequence,
    #    a conditional, an assignment) does NOT produce a `result == value`
    #    shape and is refused here -- the honest posture, not a mangle.
    value_expr = _unwrap_return(rhs, fn_name)

    # 2 + 3. Normalize the value expression's ops + literals to core SMT.
    formal_sorts_map = sorts.formal_sorts
    core_value = _normalize_term(value_expr, fn_name, formal_sorts_map)

    # 4. Sorts: every formal must carry an Int/Bool annotation (the lifter
    #    erased them to `Value`, which z3 cannot reason about for arithmetic).
    formals = contract.get("formals")
    if not isinstance(formals, list):
        raise VerifyDialectRefusal(fn_name, "contract has no formals array")
    new_formal_sorts: list[Json] = []
    for formal in formals:
        sort = formal_sorts_map.get(str(formal))
        if sort is None:
            raise VerifyDialectRefusal(
                fn_name,
                f"formal `{formal}` lacks an `int`/`bool` annotation; refusing "
                f"rather than emit a `Value`-sorted obligation z3 cannot discharge",
            )
        new_formal_sorts.append(prim_sort(sort))
    return_sort = sorts.return_sort
    if return_sort is None:
        raise VerifyDialectRefusal(
            fn_name, "return lacks an `int`/`bool` annotation; refusing"
        )

    out = dict(contract)
    out["formalSorts"] = new_formal_sorts
    out["returnSort"] = prim_sort(return_sort)
    out["post"] = {
        "kind": "atomic",
        "name": "=",
        "args": [{"kind": "var", "name": RESULT_VAR}, core_value],
    }
    # The bridge-writer (#1443) keys the auto-bridge on `bridgeSourceSymbol`;
    # the harvested callsite ctor uses the bare function name. Set it so
    # `enumerate_callsites` matches `double(3)`.
    out["bridgeSourceSymbol"] = _bare_symbol(fn_name)
    return out


def _unwrap_return(term: Json, fn_name: str) -> Json:
    if isinstance(term, dict) and term.get("kind") == "ctor" and term.get("name") == "python:return":
        inner = term.get("args")
        if isinstance(inner, list) and len(inner) == 1:
            return inner[0]
        raise VerifyDialectRefusal(fn_name, "python:return wrapper is malformed")
    # A body that did not fold to exactly one return is not a value-op.
    raise VerifyDialectRefusal(
        fn_name,
        "body does not reduce to a single `return <expr>`; the verify-facing "
        "dialect only discharges single-return value functions",
    )


def _normalize_term(term: Json, fn_name: str, formal_sorts: dict[str, str]) -> Json:
    """Recursively rewrite a value-expression term into SMT-core form, or
    refuse. Vars / int / bool consts pass through; `python:compare` becomes the
    core comparison atomic-as-term; other `python:*` ctors map via
    `_CORE_ARITH_OP` or refuse."""
    if not isinstance(term, dict):
        raise VerifyDialectRefusal(fn_name, "non-object term in value expression")
    kind = term.get("kind")
    if kind == "var":
        return term
    if kind == "const":
        sort = term.get("sort")
        sort_name = sort.get("name") if isinstance(sort, dict) else None
        if sort_name in {"Int", "Bool"}:
            return term
        raise VerifyDialectRefusal(
            fn_name, f"constant of sort `{sort_name}` is not Int/Bool; refusing"
        )
    if kind != "ctor":
        raise VerifyDialectRefusal(fn_name, f"unexpected term kind `{kind}`")

    name = term.get("name", "")
    raw_args = term.get("args")
    args = raw_args if isinstance(raw_args, list) else []

    if name == "python:compare":
        # python:compare(str_const(op), lhs, rhs) -> core comparison ctor.
        if len(args) != 3:
            raise VerifyDialectRefusal(fn_name, "python:compare arity != 3")
        op_const = args[0]
        op_str = op_const.get("value") if isinstance(op_const, dict) else None
        core_op = _CORE_CMP_OP.get(str(op_str))
        if core_op is None:
            raise VerifyDialectRefusal(
                fn_name, f"comparison op `{op_str}` has no faithful SMT-core mapping"
            )
        return {
            "kind": "ctor",
            "name": core_op,
            "args": [
                _normalize_term(args[1], fn_name, formal_sorts),
                _normalize_term(args[2], fn_name, formal_sorts),
            ],
        }

    core = _CORE_ARITH_OP.get(name)
    if core is None:
        # Includes python:div / python:floordiv / python:mod / python:pow /
        # shifts / bitwise -- all deliberately UNINTERPRETED. We do NOT refuse
        # the whole contract: instead the op stays NAMESPACED so the bridge is
        # still written and wp hits an opaque symbol it cannot reduce, yielding
        # `WpError::Refused` -> the verifier reports Undecidable (exit 3, no
        # witness) via the proven-safe body-discharge refusal path. This
        # mirrors Go's `coreArithOp` returning (_, false) for go:div and is the
        # cardinal-sin guard: a division claim becomes Undecidable, NEVER a
        # false discharge. (Refusing the contract entirely would drop the
        # bridge and fall through to the refinement path, whose honesty for an
        # unbound-ctor callsite is not guaranteed -- see body_discharge.rs.)
        return {
            "kind": "ctor",
            "name": name,
            "args": [_normalize_term_uninterpreted(a, fn_name, formal_sorts) for a in args],
        }
    return {
        "kind": "ctor",
        "name": core,
        "args": [_normalize_term(a, fn_name, formal_sorts) for a in args],
    }


def _normalize_term_uninterpreted(term: Json, fn_name: str, formal_sorts: dict[str, str]) -> Json:
    """Normalize the operands of an uninterpreted op. Core arith inside an
    uninterpreted op is still normalized (so `(x + 1) // 2` keeps the `+`),
    but the surrounding uninterpreted op is preserved by `_normalize_term`.
    This is just `_normalize_term`; named separately for intent clarity."""
    return _normalize_term(term, fn_name, formal_sorts)


def _bare_symbol(fn_name: str) -> str:
    """`double.double` / `pkg.mod.double` -> `double` (the bare ident the
    harvested call ctor and the bridge sourceSymbol use)."""
    name = fn_name
    if "(" in name:
        name = name[: name.index("(")]
    if "." in name:
        name = name[name.rindex(".") + 1 :]
    return name
