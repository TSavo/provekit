# SPDX-License-Identifier: Apache-2.0
"""Lift an AssertionVocab from the assertion library's OWN source.

Step 2 of collapsing the per-library seats: the vocabulary table is not data we
hand-author -- it is the *contract* of the assert functions, and that contract is
in the library's source. ``derive_vocab`` reads each ``assert_*`` function from the
installed module (the Source Oracle's act: resolve the function to its body from
the real package) and classifies it by structure:

  - SIGNATURE carries a tolerance parameter (rtol/atol/decimal/...) -> APPROX.
    This is the soundness-critical split -- exact vs ``a ~= b`` within tolerance --
    and it falls out of the signature with no human input. Lifting an approximate
    assertion as ``=`` is the false-pass we refuse; the signature TELLS us which
    ones those are.
  - BODY delegates a comparison with ``operator.__eq__`` as the first arg
    (numpy's ``assert_array_compare(operator.__eq__, ...)`` family) -> EQUALITY (``=``).
  - everything else -> OTHER (loud-refuse). The trichotomy applied to the
    vocabulary itself: lift what's structurally clear, refuse what isn't.

What the structure cannot confirm (e.g. numpy's ``assert_equal``, whose body is a
recursive dispatch with no single delegated operator) stays an explicit, labeled
``override`` -- a small, honest human remainder on top of the derived core, NOT a
hand-transcribed table.
"""

from __future__ import annotations

import ast
import importlib
import inspect
import textwrap
from typing import Dict, FrozenSet, Optional, Set

from provekit_lift_py_tests.assertion_layer import AssertionVocab

# A parameter named one of these means the assertion compares within a tolerance:
# it is APPROXIMATE, and must never be lifted as exact equality.
_TOLERANCE_PARAMS = frozenset({
    "rtol", "atol", "decimal", "significant", "nulp", "maxulp", "places", "delta",
})


def _params(fn: ast.AST) -> Set[str]:
    a = fn.args  # type: ignore[attr-defined]
    return {p.arg for p in (a.posonlyargs + a.args + a.kwonlyargs)}


def _delegates_eq(fn: ast.AST) -> bool:
    """True iff the body delegates a comparison whose first arg is
    ``operator.__eq__`` (numpy's assert_array_compare(operator.__eq__, ...))."""
    for node in ast.walk(fn):
        if isinstance(node, ast.Call) and node.args:
            a0 = node.args[0]
            if isinstance(a0, ast.Attribute) and a0.attr == "__eq__":
                return True
    return False


def _classify(fn: ast.AST) -> str:
    if _params(fn) & _TOLERANCE_PARAMS:
        return "approx"
    if _delegates_eq(fn):
        return "equality"
    return "other"


def derive_vocab(
    module_name: str,
    label: str,
    overrides: "Optional[Dict[str, FrozenSet[str]]]" = None,
    harmless_kwargs: "Optional[FrozenSet[str]]" = None,
    require_true_kwargs: FrozenSet[str] = frozenset(),
) -> AssertionVocab:
    """Derive an AssertionVocab by reading ``module_name``'s assert functions.

    ``overrides`` maps a category (``equality``/``truth``/``approx``/``other``) to
    names to FORCE into it -- the small structurally-opaque remainder. A name in
    an override is removed from every derived set first, so the override wins."""
    mod = importlib.import_module(module_name)
    cats: Dict[str, Set[str]] = {"equality": set(), "truth": set(), "approx": set(), "other": set()}
    for name in dir(mod):
        if not name.startswith("assert_"):
            continue
        obj = getattr(mod, name)
        if not callable(obj):
            continue
        try:
            src = textwrap.dedent(inspect.getsource(obj))
            fn = next(n for n in ast.parse(src).body if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)))
        except (OSError, TypeError, SyntaxError, StopIteration):
            cats["other"].add(name)
            continue
        cats[_classify(fn)].add(name)

    forced = set().union(*overrides.values()) if overrides else set()
    for cat in cats:
        cats[cat] -= forced
    if overrides:
        for cat, names in overrides.items():
            cats[cat] |= set(names)

    kw = harmless_kwargs if harmless_kwargs is not None else frozenset({"err_msg", "verbose"})
    return AssertionVocab(
        label=label,
        equality=frozenset(cats["equality"]),
        truth=frozenset(cats["truth"]),
        approx=frozenset(cats["approx"]),
        other=frozenset(cats["other"]),
        harmless_kwargs=kw,
        require_true_kwargs=require_true_kwargs,
    )
