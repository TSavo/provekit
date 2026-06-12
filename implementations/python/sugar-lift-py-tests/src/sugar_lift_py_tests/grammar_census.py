"""Body-shape census classifier — the corpus's vote, brought in-repo.

This is the classification function that ran over the top-1000 PyPI
sdists (128,766 files, 1,519,521 functions, battleaxe ~/census.py,
2026-06-12): every FunctionDef body lands in exactly ONE primary shape
bucket, plus orthogonal flags. TOTAL by construction: every last
statement that is not a value-return falls into the parametric
``non-return:<Stmt>`` bucket and every return value that matches no
named shape falls into ``return-other:<Expr>`` — there is no silent
drop, so the bucket vocabulary is exactly (named shapes) ∪ (parametric
buckets over the interpreter's own grammar), which is what lets
grammar_ledger hold an import-time totality floor against it.

Kept byte-faithful to the corpus run's semantics; changing a bucket
boundary here re-buckets the census, so any edit must re-run the corpus.
"""

from __future__ import annotations

import ast
from typing import List, Tuple


def _strip_doc(body):
    return [
        s
        for s in body
        if not (
            isinstance(s, ast.Expr)
            and isinstance(s.value, ast.Constant)
            and isinstance(s.value.value, str)
        )
    ]


def _is_literalish(node) -> bool:
    return isinstance(node, ast.Constant)


def classify(fn: ast.FunctionDef) -> Tuple[str, List[str]]:
    """Primary bucket + orthogonal flags. TOTAL: always returns a bucket."""
    body = _strip_doc(fn.body)
    flags = []
    if not body:
        return "empty", flags

    # guard-then-raise prefix: one or more `if X: raise` leading statements
    guards = 0
    for s in body:
        if (
            isinstance(s, ast.If)
            and len(s.body) == 1
            and isinstance(s.body[0], ast.Raise)
            and not s.orelse
        ):
            guards += 1
        else:
            break
    if guards:
        flags.append("guard-then-raise-prefix")

    # table-loop: any for-loop whose body subscripts a Name and accumulates
    for s in ast.walk(fn):
        if isinstance(s, (ast.For, ast.AsyncFor)):
            has_sub = any(
                isinstance(n, ast.Subscript) and isinstance(n.value, ast.Name)
                for n in ast.walk(s)
            )
            has_acc = any(
                isinstance(n, (ast.AugAssign,))
                or (
                    isinstance(n, ast.Call)
                    and isinstance(n.func, ast.Attribute)
                    and n.func.attr in ("append", "extend", "write", "add")
                )
                for n in ast.walk(s)
            )
            if has_sub and has_acc:
                flags.append("table-loop")
                break

    last = body[-1]
    if not isinstance(last, ast.Return) or last.value is None:
        return f"non-return:{type(last).__name__}", flags
    v = last.value

    if isinstance(v, ast.Call):
        f = v.func
        if isinstance(f, ast.Attribute):
            a = f.attr
            args = v.args
            lit1 = len(args) == 1 and _is_literalish(args[0])
            if a == "translate":
                return "return-translate", flags
            if a in ("rstrip", "lstrip", "strip") and lit1:
                return "return-strip-literal", flags
            if a == "replace" and len(args) == 2 and all(map(_is_literalish, args)):
                return "return-replace-literals", flags
            if a == "join":
                return "return-join", flags
            if a in ("encode", "decode"):
                return "return-encode-decode", flags
            if a == "format":
                return "return-format", flags
            if a in ("upper", "lower", "casefold", "title"):
                return "return-case-method", flags
            return "return-method-call", flags
        if isinstance(f, ast.Name):
            if len(body) == 1:
                return "pure-delegation", flags
            return "return-fn-call", flags
        return "return-call-other", flags
    if isinstance(v, ast.Subscript) and isinstance(v.value, ast.Name):
        return "return-table-subscript", flags
    if isinstance(v, ast.Constant):
        return "return-constant", flags
    if isinstance(v, ast.Name):
        return "return-name", flags
    if isinstance(v, ast.BinOp):
        return "return-binop", flags
    if isinstance(v, (ast.Compare, ast.BoolOp)) or (
        isinstance(v, ast.UnaryOp) and isinstance(v.op, ast.Not)
    ):
        return "return-predicate", flags
    if isinstance(v, ast.IfExp):
        return "return-ifexp", flags
    if isinstance(v, ast.JoinedStr):
        return "return-fstring", flags
    if isinstance(v, (ast.Tuple, ast.List, ast.Dict, ast.Set)):
        return "return-collection", flags
    if isinstance(v, ast.Attribute):
        return "return-attribute", flags
    return f"return-other:{type(v).__name__}", flags
