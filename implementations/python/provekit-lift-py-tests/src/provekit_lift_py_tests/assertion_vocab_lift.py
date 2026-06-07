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
import json
import os
import textwrap
from collections import defaultdict
from functools import lru_cache
from typing import Dict, FrozenSet, List, Optional, Sequence, Set, Tuple

from provekit_lift_py_tests.assertion_layer import (
    AssertionVocab,
    ToleranceSpec,
    lift_file_assertions,
)
from provekit_lift_py_tests.layer2 import Layer2Output, LiftWarning

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
    tolerances: "Tuple[ToleranceSpec, ...]" = (),
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
        tolerances=tolerances,
    )


# --- learn at lift time: derive live, apply an EXTERNALIZED exception ----------
#
# The vocabulary is LEARNED when you point the lifter at a test: each imported
# testing module is derived live. The only human input is the per-module
# EXCEPTION -- the structurally-opaque remainder -- and it is EXTERNAL DATA, not
# code: a `<module>.json` resolved from a workspace's `.provekit/vocab-exceptions/`
# directory. A library gets (or changes) its exceptions by dropping/editing that
# file; the lifter never changes. A module with no declaration is pure derivation.


def _load_exception(module_name: str, exception_dirs: Tuple[str, ...]) -> Optional[dict]:
    """Resolve a module's externalized exception declaration (`<module>.json`),
    searched across ``exception_dirs`` in order. None -> pure derivation."""
    for d in exception_dirs:
        path = os.path.join(d, f"{module_name}.json")
        if os.path.isfile(path):
            with open(path, encoding="utf-8") as fh:
                return json.load(fh)
    return None


@lru_cache(maxsize=None)
def learn_vocab(module_name: str, exception_dirs: Tuple[str, ...] = ()) -> AssertionVocab:
    """Derive ``module_name``'s vocabulary live, then apply its externalized
    exception (data) from ``exception_dirs``. Cached per (module, dirs)."""
    exc = _load_exception(module_name, exception_dirs) or {}
    overrides = {cat: frozenset(names) for cat, names in exc.get("overrides", {}).items()} or None
    tolerances = tuple(ToleranceSpec(**t) for t in exc.get("tolerances", []))
    harmless = frozenset(exc["harmless_kwargs"]) if "harmless_kwargs" in exc else None
    require_true = frozenset(exc.get("require_true_kwargs", []))
    return derive_vocab(
        module_name,
        exc.get("label", module_name),
        overrides=overrides,
        harmless_kwargs=harmless,
        require_true_kwargs=require_true,
        tolerances=tolerances,
    )


_TESTING_HINT = "testing"


def _module_has_assertions(module_name: str) -> bool:
    try:
        mod = importlib.import_module(module_name)
    except Exception:
        return False
    return any(n.startswith("assert_") and callable(getattr(mod, n, None)) for n in dir(mod))


def _scan_testing_modules(tree: ast.AST) -> List[str]:
    """Find the assertion-vocabulary modules a test file uses: any module it
    imports (``from M import ...`` / ``import M``) whose name contains ``testing``,
    plus ``<pkg>.testing.assert_*`` attribute usage via an aliased package import.
    Filtered to modules that actually expose ``assert_*`` callables."""
    candidates: Set[str] = set()
    pkg_aliases: Dict[str, str] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom) and node.module:
            if _TESTING_HINT in node.module:
                candidates.add(node.module)
        elif isinstance(node, ast.Import):
            for a in node.names:
                if _TESTING_HINT in a.name:
                    candidates.add(a.name)
                top = a.name.split(".")[0]
                pkg_aliases[a.asname or top] = top
    # `np.testing.assert_*` -> `numpy.testing`
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Attribute)
            and node.attr.startswith("assert")
            and isinstance(node.value, ast.Attribute)
            and node.value.attr == "testing"
            and isinstance(node.value.value, ast.Name)
            and node.value.value.id in pkg_aliases
        ):
            candidates.add(f"{pkg_aliases[node.value.value.id]}.testing")
    found = sorted(m for m in candidates if _module_has_assertions(m))
    # Drop a detected module that is a SUBMODULE of another detected module: it is
    # the private implementation of the public one (e.g. numpy.testing._private.utils
    # under numpy.testing), and only the public module carries the exception. The
    # public module re-exports the same assert_* names, so nothing is lost. This does
    # NOT drop a public underscore module with no detected ancestor (sklearn.utils._testing).
    return [m for m in found if not any(o != m and m.startswith(o + ".") for o in found)]


def _merge_vocabs(vocabs: Sequence[AssertionVocab], label: str) -> AssertionVocab:
    """Combine per-module vocabularies. SOUND rule: a name lifts as exact equality
    only if every source that has it agrees it is equality; any source classifying
    it approx/other (or a mixed equality/truth) keeps it from exact-lifting. On the
    real corpus each file imports one library, so no collision occurs -- this is the
    safe general rule, not an unsound union."""
    seen: Dict[str, Set[str]] = defaultdict(set)
    for v in vocabs:
        for cat in ("equality", "truth", "approx", "other"):
            for n in getattr(v, cat):
                seen[n].add(cat)
    equality, truth, approx, other = set(), set(), set(), set()
    for name, cats in seen.items():
        if "approx" in cats:
            approx.add(name)
        elif "other" in cats:
            other.add(name)
        elif cats == {"equality"}:
            equality.add(name)
        elif cats == {"truth"}:
            truth.add(name)
        else:
            other.add(name)
    tolerances = tuple(t for v in vocabs for t in v.tolerances)
    harmless = frozenset().union(*(v.harmless_kwargs for v in vocabs)) if vocabs else frozenset()
    require_true = frozenset().union(*(v.require_true_kwargs for v in vocabs)) if vocabs else frozenset()
    return AssertionVocab(
        label=label,
        equality=frozenset(equality),
        truth=frozenset(truth),
        approx=frozenset(approx),
        other=frozenset(other),
        harmless_kwargs=harmless,
        require_true_kwargs=require_true,
        tolerances=tolerances,
    )


def lift_test_file(
    source: str,
    source_path: str,
    workspace_root: Optional[str] = None,
    exception_dirs: Tuple[str, ...] = (),
) -> Layer2Output:
    """The one lifter. Point it at a test file: it LEARNS the assertion vocabulary
    from the file's imports (deriving each imported testing module live, applying
    any externalized `.provekit/vocab-exceptions/<module>.json` declaration), then
    lifts. No per-library seat, no --library flag."""
    dirs = tuple(exception_dirs)
    if workspace_root:
        dirs = dirs + (os.path.join(workspace_root, ".provekit", "vocab-exceptions"),)
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as e:
        out = Layer2Output()
        out.warnings.append(LiftWarning(source_path, "<file>", f"auto-lift: failed to parse: {e}"))
        return out
    vocabs = [learn_vocab(m, dirs) for m in _scan_testing_modules(tree)]
    vocabs = [v for v in vocabs if v.all]
    if not vocabs:
        return Layer2Output()
    vocab = vocabs[0] if len(vocabs) == 1 else _merge_vocabs(vocabs, "auto")
    return lift_file_assertions(source, source_path, vocab)
