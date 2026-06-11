"""Translate-table universe walk (python generalization layer, rung 2a).

A vendor body of the shape ``return <call>.translate(TABLE)`` where TABLE
is a stable module-level ``bytes.maketrans(b"...", b"...")`` binding swears
a complement universe: translate is total, so the output contains NONE of
the surviving from-side characters. Emitted as
``str.chars-not-in-set(subject, forbidden)`` — one more conjunct under the
callee's ``#euf#`` base. The encoder being delegated to (C or python) is
never walked; the claim rests entirely on the translate literals, which is
the whole point: the seam is provable even when the encoder is not.

Soundness gates (each refusal is named, never silent):
- the vendor body must be exactly docstring? + return <call>.translate(NAME)
  (any other statement could affect the result; refuse the walk);
- NAME must have exactly one module-level binding and no global-declaration
  puncture (the same teeth as sugar_lift_python_source.value_pins);
- the binding must be ``bytes.maketrans(<bytes literal>, <bytes literal>)``
  with equal lengths (CPython raises otherwise) and ASCII-only bytes;
- the forbidden set is from_set MINUS to_set: a translate that maps '+'->'/'
  REINTRODUCES '/', so swapped tables yield an empty set and NO universe.
"""

from __future__ import annotations

import ast
import functools
import importlib.util
from dataclasses import dataclass
from typing import Optional, Tuple

try:
    from sugar_lift_python_source.value_pins import (
        _admission_failure,
        _binding_events,
        _Candidate,
        _global_declarations,
    )
except ModuleNotFoundError:  # repo checkout without editable installs
    import sys
    from pathlib import Path

    _SIBLING_SRC = (
        Path(__file__).resolve().parents[3] / "sugar-lift-python-source" / "src"
    )
    if str(_SIBLING_SRC) not in sys.path:
        sys.path.insert(0, str(_SIBLING_SRC))
    # The binding-stability teeth live in ONE place (value_pins); a local
    # mirror here would be the Java lesson's latent-hole scan all over again.
    from sugar_lift_python_source.value_pins import (
        _admission_failure,
        _binding_events,
        _Candidate,
        _global_declarations,
    )


@dataclass(frozen=True)
class TranslateUniverse:
    """A walked complement universe: the callee's output contains none of
    ``forbidden``. Provenance pins the claim to the vendor source."""

    forbidden: str
    module: str
    qualname: str
    source_path: str
    lineno: int
    table_name: str


@dataclass(frozen=True)
class TranslateWalkRefusal:
    callee: str
    reason: str


@functools.lru_cache(maxsize=None)
def translate_universe_for_callee(
    callee: str,
) -> Tuple[Optional[TranslateUniverse], Optional[TranslateWalkRefusal]]:
    """Resolve a dotted callee against installed vendor source and walk the
    translate shape. Returns (universe, None) on success, (None, refusal)
    when the callee matched the family but a gate refused, and (None, None)
    when the callee is simply not translate-shaped (not a candidate; no
    refusal owed)."""
    if "." not in callee:
        return None, None
    module_name, fn_name = callee.rsplit(".", 1)
    try:
        spec = importlib.util.find_spec(module_name)
    except (ImportError, ValueError):
        return None, None
    if spec is None or spec.origin in (None, "built-in", "frozen"):
        return None, None
    if not spec.origin.endswith(".py"):
        # Compiled extension: nothing to walk. Not a candidate.
        return None, None
    try:
        source = open(spec.origin, encoding="utf-8").read()
    except OSError:
        return None, None
    try:
        tree = ast.parse(source, filename=spec.origin)
    except SyntaxError:
        return None, None

    fn = next(
        (
            stmt
            for stmt in tree.body
            if isinstance(stmt, ast.FunctionDef) and stmt.name == fn_name
        ),
        None,
    )
    if fn is None:
        return None, None

    body = [
        stmt
        for stmt in fn.body
        if not (
            isinstance(stmt, ast.Expr)
            and isinstance(stmt.value, ast.Constant)
            and isinstance(stmt.value.value, str)
        )
    ]
    shape = _translate_return_shape(body)
    if shape is None:
        # Not translate-shaped: not a candidate, no refusal owed.
        return None, None
    table_name = shape

    def refuse(reason: str) -> Tuple[None, TranslateWalkRefusal]:
        return None, TranslateWalkRefusal(callee=callee, reason=reason)

    table_call, table_line = _module_binding_call(tree, table_name)
    if table_call is None:
        return refuse(
            f"translate table '{table_name}' has no single module-level "
            "call binding"
        )
    candidate = _Candidate(
        name=table_name, value=table_call, line=table_line, confession=None
    )
    events = [e for e in _binding_events(tree) if e.name == table_name]
    failure = _admission_failure(
        candidate, events, _global_declarations(tree).get(table_name)
    )
    if failure is not None:
        return refuse(f"translate table '{table_name}' is not stable: {failure}")

    pair = _maketrans_byte_literals(table_call)
    if pair is None:
        return refuse(
            f"translate table '{table_name}' is not "
            "bytes.maketrans(<bytes literal>, <bytes literal>)"
        )
    frm, to = pair
    if len(frm) != len(to):
        return refuse("maketrans from/to lengths differ")
    try:
        frm_s = frm.decode("ascii")
        to_s = to.decode("ascii")
    except UnicodeDecodeError:
        return refuse("maketrans bytes are not ASCII; charset atom needs ASCII")

    # from MINUS to: a mapped-away char that some other mapping writes back
    # is NOT forbidden in the output. Swapped tables yield the empty set.
    forbidden = "".join(sorted(set(frm_s) - set(to_s)))
    if not forbidden:
        return refuse(
            "translate table reintroduces every mapped char (swap-shaped); "
            "no complement universe exists"
        )

    return (
        TranslateUniverse(
            forbidden=forbidden,
            module=module_name,
            qualname=f"{module_name}.{fn_name}",
            source_path=spec.origin,
            lineno=fn.lineno,
            table_name=table_name,
        ),
        None,
    )


def _translate_return_shape(body: list) -> Optional[str]:
    """Match exactly one statement: return <call>.translate(NAME).
    Returns NAME, or None when the body is not translate-shaped."""
    if len(body) != 1 or not isinstance(body[0], ast.Return):
        return None
    value = body[0].value
    if (
        isinstance(value, ast.Call)
        and isinstance(value.func, ast.Attribute)
        and value.func.attr == "translate"
        and isinstance(value.func.value, ast.Call)
        and len(value.args) == 1
        and not value.keywords
        and isinstance(value.args[0], ast.Name)
    ):
        return value.args[0].id
    return None


def _module_binding_call(
    tree: ast.Module, name: str
) -> Tuple[Optional[ast.Call], int]:
    binding: Optional[ast.Call] = None
    line = 0
    for stmt in tree.body:
        if (
            isinstance(stmt, ast.Assign)
            and len(stmt.targets) == 1
            and isinstance(stmt.targets[0], ast.Name)
            and stmt.targets[0].id == name
        ):
            if binding is not None:
                return None, 0
            if isinstance(stmt.value, ast.Call):
                binding = stmt.value
                line = stmt.lineno
            else:
                return None, 0
    return binding, line


def _maketrans_byte_literals(
    call: ast.Call,
) -> Optional[Tuple[bytes, bytes]]:
    func = call.func
    is_maketrans = (
        isinstance(func, ast.Attribute)
        and func.attr == "maketrans"
        and isinstance(func.value, ast.Name)
        and func.value.id in ("bytes", "bytearray")
    )
    if not is_maketrans or len(call.args) != 2 or call.keywords:
        return None
    frm, to = call.args
    if (
        isinstance(frm, ast.Constant)
        and isinstance(frm.value, bytes)
        and isinstance(to, ast.Constant)
        and isinstance(to.value, bytes)
    ):
        return frm.value, to.value
    return None
