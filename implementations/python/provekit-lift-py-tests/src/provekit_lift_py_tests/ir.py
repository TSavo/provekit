# SPDX-License-Identifier: Apache-2.0
#
# Minimal Python IR shape mirroring provekit-ir-symbolic.
#
# Three formula kinds (atomic / connective / quantifier) and three term
# kinds (var / const / ctor). Sort is a primitive name. ContractDecl is
# an emit-time record carrying name, optional pre/post/inv, and an
# outBinding.
#
# Locked IR-JSON shape per protocol/specs/2026-04-30-ir-formal-grammar.md.
# Insertion-order serialization that the canonicalizer's JCS pass re-sorts
# before hashing. We emit canonical Value trees directly (skipping the
# kit's insertion-order JSON string), since downstream hashing is what
# matters.

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Optional, Tuple, Union

from .canonicalizer import Value, varr, vint, vobj, vstr


# Sort ----------------------------------------------------------------------


@dataclass(frozen=True)
class Sort:
    name: str  # "Int" / "Real" / "String" / "Bool"


def Int() -> Sort:
    return Sort("Int")


def Real() -> Sort:
    return Sort("Real")


def String() -> Sort:
    return Sort("String")


def Bool() -> Sort:
    return Sort("Bool")


# Term ----------------------------------------------------------------------


@dataclass(frozen=True)
class _Var:
    name: str


@dataclass(frozen=True)
class _ConstInt:
    value: int
    sort: Sort


@dataclass(frozen=True)
class _ConstStr:
    value: str
    sort: Sort


@dataclass(frozen=True)
class _ConstBool:
    value: bool
    sort: Sort


@dataclass(frozen=True)
class _Ctor:
    name: str
    args: Tuple["Term", ...]


Term = Union[_Var, _ConstInt, _ConstStr, _ConstBool, _Ctor]


def make_var(name: str) -> Term:
    return _Var(name)


def num(n: int) -> Term:
    return _ConstInt(int(n), Int())


def str_const(s: str) -> Term:
    return _ConstStr(s, String())


def bool_const(b: bool) -> Term:
    return _ConstBool(bool(b), Bool())


def ctor(name: str, args: List[Term]) -> Term:
    return _Ctor(name, tuple(args))


# Formula -------------------------------------------------------------------


@dataclass(frozen=True)
class _Atomic:
    name: str
    args: Tuple[Term, ...]


@dataclass(frozen=True)
class _Connective:
    kind: str  # and / or / not / implies
    operands: Tuple["Formula", ...]


@dataclass(frozen=True)
class _Quantifier:
    kind: str  # forall / exists
    name: str
    sort: Sort
    body: "Formula"


Formula = Union[_Atomic, _Connective, _Quantifier]


def atomic(name: str, args: List[Term]) -> Formula:
    return _Atomic(name, tuple(args))


# Atomic predicate names use the Unicode glyphs >=, <=, !=. Cross-language
# hash agreement depends on UTF-8 verbatim emission for U+0080+.
def gt(a: Term, b: Term) -> Formula:
    return atomic(">", [a, b])


def gte(a: Term, b: Term) -> Formula:
    return atomic("≥", [a, b])


def lt(a: Term, b: Term) -> Formula:
    return atomic("<", [a, b])


def lte(a: Term, b: Term) -> Formula:
    return atomic("≤", [a, b])


def eq(a: Term, b: Term) -> Formula:
    return atomic("=", [a, b])


def ne(a: Term, b: Term) -> Formula:
    return atomic("≠", [a, b])


def connective(kind: str, operands: List[Formula]) -> Formula:
    return _Connective(kind, tuple(operands))


def not_(a: Formula) -> Formula:
    return connective("not", [a])


def implies(a: Formula, b: Formula) -> Formula:
    return connective("implies", [a, b])


def and_(operands: List[Formula]) -> Formula:
    return connective("and", operands)


def or_(operands: List[Formula]) -> Formula:
    return connective("or", operands)


def forall(name: str, sort: Sort, body: Formula) -> Formula:
    return _Quantifier("forall", name, sort, body)


def exists(name: str, sort: Sort, body: Formula) -> Formula:
    return _Quantifier("exists", name, sort, body)


# ContractDecl --------------------------------------------------------------


@dataclass
class ContractDecl:
    name: str
    pre: Optional[Formula] = None
    post: Optional[Formula] = None
    inv: Optional[Formula] = None
    out_binding: str = "out"


# To-Value (canonicalizer Value tree) --------------------------------------


def sort_to_value(s: Sort) -> Value:
    return vobj([("kind", vstr("primitive")), ("name", vstr(s.name))])


def term_to_value(t: Term) -> Value:
    if isinstance(t, _Var):
        return vobj([("kind", vstr("var")), ("name", vstr(t.name))])
    if isinstance(t, _ConstInt):
        return vobj([
            ("kind", vstr("const")),
            ("value", vint(t.value)),
            ("sort", sort_to_value(t.sort)),
        ])
    if isinstance(t, _ConstStr):
        return vobj([
            ("kind", vstr("const")),
            ("value", vstr(t.value)),
            ("sort", sort_to_value(t.sort)),
        ])
    if isinstance(t, _ConstBool):
        from .canonicalizer import vbool
        return vobj([
            ("kind", vstr("const")),
            ("value", vbool(t.value)),
            ("sort", sort_to_value(t.sort)),
        ])
    if isinstance(t, _Ctor):
        return vobj([
            ("kind", vstr("ctor")),
            ("name", vstr(t.name)),
            ("args", varr([term_to_value(a) for a in t.args])),
        ])
    raise TypeError(f"unknown Term: {type(t)!r}")


def formula_to_value(f: Formula) -> Value:
    if isinstance(f, _Atomic):
        return vobj([
            ("kind", vstr("atomic")),
            ("name", vstr(f.name)),
            ("args", varr([term_to_value(a) for a in f.args])),
        ])
    if isinstance(f, _Connective):
        return vobj([
            ("kind", vstr(f.kind)),
            ("operands", varr([formula_to_value(o) for o in f.operands])),
        ])
    if isinstance(f, _Quantifier):
        return vobj([
            ("kind", vstr(f.kind)),
            ("name", vstr(f.name)),
            ("sort", sort_to_value(f.sort)),
            ("body", formula_to_value(f.body)),
        ])
    raise TypeError(f"unknown Formula: {type(f)!r}")


# Variable substitution (used by helper-inlining and parametrize patterns)


def subst_var_in_term(t: Term, formal: str, actual: Term) -> Term:
    if isinstance(t, _Var):
        return actual if t.name == formal else t
    if isinstance(t, _Ctor):
        return _Ctor(t.name, tuple(subst_var_in_term(a, formal, actual) for a in t.args))
    return t  # const variants are inert


def subst_var_in_formula(f: Formula, formal: str, actual: Term) -> Formula:
    if isinstance(f, _Atomic):
        return _Atomic(f.name, tuple(subst_var_in_term(a, formal, actual) for a in f.args))
    if isinstance(f, _Connective):
        return _Connective(f.kind, tuple(subst_var_in_formula(o, formal, actual) for o in f.operands))
    if isinstance(f, _Quantifier):
        # Don't substitute under a shadowing binder.
        if f.name == formal:
            return f
        return _Quantifier(f.kind, f.name, f.sort, subst_var_in_formula(f.body, formal, actual))
    raise TypeError(f"unknown Formula: {type(f)!r}")
