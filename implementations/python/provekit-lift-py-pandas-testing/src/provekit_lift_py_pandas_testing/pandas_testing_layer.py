# SPDX-License-Identifier: Apache-2.0
"""pandas ``pandas.testing`` assertion-vocabulary lift surface.

A SEPARATE federation seat, the exact sibling of the numpy.testing seat
(``provekit_lift_py_numpy_testing``).  pytest natively collects bare ``assert``;
``pandas.testing`` (``assert_frame_equal``, ``assert_series_equal``,
``assert_index_equal``, ...) is a THIRD-PARTY assertion vocabulary pytest knows
nothing about, so it gets its OWN seat — reusing the pytest package's
term/formula machinery so that ``assert_frame_equal(a, b)`` and a bare
``assert a.equals(b)`` describe the same relation.

SOUNDNESS — the EXACT/APPROXIMATE split is the whole game, and pandas makes it
SHARPER than numpy because the DEFAULT is approximate:

  ``pandas.testing.assert_frame_equal`` / ``assert_series_equal`` compare floats
  with a TOLERANCE by default (``check_exact`` is False for floats).  So a bare
  ``assert_frame_equal(op(...), expected)`` is ``op ≈ expected``, NOT
  ``op = expected``.  Lifting it as ``=`` would FALSE-PASS: it would claim exact
  equality that pandas never checked, letting a consumer rely on a value that is
  only tolerance-close.

  Therefore — version-independently, so we never depend on a default that has
  drifted across pandas 2.x — we lift a frame/series/index assertion as ``=``
  ONLY when ``check_exact=True`` is EXPLICITLY pinned AND no relation-altering
  keyword is present.  Everything else is a LOUD REFUSE:

  EXACT (lifted as ``=``):
    assert_frame_equal(a, b, check_exact=True)
    assert_series_equal(a, b, check_exact=True)
    assert_index_equal(a, b, check_exact=True)
    assert_extension_array_equal(a, b, check_exact=True)

  APPROXIMATE / RELATION-ALTERING (LOUD REFUSE):
    the same calls WITHOUT a pinned ``check_exact=True`` (approximate by
    default), or carrying ``check_like`` (ignores row/column ORDER — a
    different relation), ``rtol``/``atol`` (explicit tolerance), or any other
    keyword whose presence we cannot prove keeps the relation an exact ``=``.

Note (why the consistency axis is honest here): even when lifted, a
frame/series equality is opaque-EUF on BOTH sides (``op = DataFrame(X)``), so z3
cannot manufacture a contradiction between two opaque constructors — the same
as numpy array equality.  The CONSISTENCY teeth for pandas come from SCALAR
assertions (``df['a'].sum() == 6``), which the pytest bare-assert seat already
lifts.  THIS seat's load-bearing contribution is (1) sound loud-refusal of
approximate frame comparison (false-pass prevention) and (2) callsite keying of
the pandas op under test for the witness axis.

Binding handling mirrors the numpy/pytest lifters EXACTLY: simple ``x = <expr>``
bindings are SSA-versioned opaque free vars (reassignment bumps the version),
and any mutation, control flow, or non-assertion side-effecting expression
statement is LOUDLY REFUSED — soundness depends on bound values being stable.
"""

from __future__ import annotations

import ast
from collections import OrderedDict
from typing import Dict, List, Optional, Sequence

# Reuse the pytest seat's shared term/formula machinery so equivalent claims
# federate.  (Same imports the numpy.testing seat uses; if a shared
# libprovekit-py term core is extracted later, re-point these together.)
import provekit_lift_py_tests.layer2 as _l2
from provekit_lift_py_tests.layer2 import (
    Layer2Output,
    LiftWarning,
    _ValueScope,
    _call_key,
    _callsite_contract_base,
    _call_origin_from_expr,
    _call_result_term,
    _canonical_term_sig,
    _Ctor,
    _euf_args_all_concrete,
    _iter_test_functions,
    _lift_assertion_stmt_scoped,
    _strip_self,
    _translate_term_scoped,
    _unparse,
)
from provekit_lift_py_tests.ir import (
    ContractDecl,
    Term,
    Formula,
    and_,
    comparison_with_none_guard,
    make_var,
)

# --- pandas.testing vocabulary -----------------------------------------------

# Structural equality assertions.  EXACT only when ``check_exact=True`` is
# pinned (see module docstring); otherwise APPROXIMATE -> loud refuse.  These
# four are the public ``pandas.testing`` surface.
_PDT_EQUALITY = {
    "assert_frame_equal",
    "assert_series_equal",
    "assert_index_equal",
    "assert_extension_array_equal",
}
# The REST of the ``pandas._testing`` (``tm``) assertion vocabulary — the module
# pandas's own suite imports as ``tm``, a 22-name superset of the 4 public ones.
# We RECOGNISE every member so we CLAIM+REFUSE loudly rather than silently skip
# a real assertion (the trichotomy: express the exact, loudly refuse the rest).
# Lifting these soundly is future work (e.g. assert_is_sorted -> a sortedness
# obligation, assert_numpy_array_equal -> element-wise ``=``); v0 refuses.
#
# NOTE: ``assert_equal`` is DELIBERATELY excluded — it is a cross-library
# generic name (numpy.testing also exports it) owned by the numpy/generic
# equality seat; claiming it here would double-claim the same call.
_PDT_OTHER = {
    "assert_almost_equal",  # APPROXIMATE by name; never exact
    "assert_attr_equal",
    "assert_categorical_equal",
    "assert_class_equal",
    "assert_contains_all",
    "assert_copy",
    "assert_datetime_array_equal",
    "assert_dict_equal",
    "assert_indexing_slices_equivalent",
    "assert_interval_array_equal",
    "assert_is_sorted",
    "assert_metadata_equivalent",
    "assert_numpy_array_equal",
    "assert_period_array_equal",
    "assert_produces_warning",
    "assert_sp_array_equal",
    "assert_timedelta_array_equal",
}
_PDT_ALL = _PDT_EQUALITY | _PDT_OTHER

# Keywords that are presentational only (do not change the asserted relation).
# Anything outside this set on an equality call is refused, because we cannot
# prove it keeps the relation an exact ``=`` (e.g. check_like reorders,
# rtol/atol loosen, check_dtype=False relaxes).
_PDT_HARMLESS_KW = {"obj", "check_exact"}


def _pdt_call_name(func: ast.expr) -> Optional[str]:
    """Final callee name for ``assert_frame_equal(...)`` (bare ``Name``) AND
    ``pd.testing.assert_frame_equal(...)`` / ``tm.assert_frame_equal(...)``
    (``Attribute``).  None if the callee is not a simple name/attribute."""
    if isinstance(func, ast.Name):
        return func.id
    if isinstance(func, ast.Attribute):
        return func.attr
    return None


def _pdt_assertion_name(stmt: ast.stmt) -> Optional[str]:
    """The pandas.testing assertion name iff ``stmt`` is ``<pdt-call>(...)`` as a
    bare expression statement; else None."""
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        name = _pdt_call_name(stmt.value.func)
        if name in _PDT_ALL:
            return name
    return None


def _check_exact_pinned_true(call: ast.Call) -> bool:
    """True iff the call carries an explicit ``check_exact=True`` keyword."""
    for kw in call.keywords:
        if kw.arg == "check_exact" and isinstance(kw.value, ast.Constant) and kw.value.value is True:
            return True
    return False


def _lift_pdt_assertion_scoped(call: ast.Call, scope: _ValueScope, call_vars) -> Formula:
    """Lift one pandas.testing assertion call under the current SSA scope.
    Raises ValueError (loud, recorded as a skip) for approximate / relation-
    altering / unsupported forms so they are NEVER silently lifted as exact
    equality."""
    name = _pdt_call_name(call.func)
    if name in _PDT_EQUALITY:
        if len(call.args) < 2:
            raise ValueError(f"{name} expects at least 2 positional args")
        kw = {k.arg for k in call.keywords}
        extra = kw - _PDT_HARMLESS_KW
        if extra:
            # check_like reorders, rtol/atol loosen, check_dtype relaxes, ...:
            # any of these changes the relation away from an exact ``=``.
            raise ValueError(
                f"{name}: keyword arg(s) {sorted(extra)} may change the "
                "relation; not liftable as exact equality"
            )
        if not _check_exact_pinned_true(call):
            # pandas compares floats with tolerance by DEFAULT; lifting an
            # un-pinned frame/series assertion as ``=`` would false-pass.
            raise ValueError(
                f"`{name}` is approximate by default (float tolerance); "
                "refused as exact equality unless check_exact=True is pinned"
            )
        l = _translate_term_scoped(call.args[0], scope, call_vars)
        r = _translate_term_scoped(call.args[1], scope, call_vars)
        return comparison_with_none_guard("=", l, r, emit_none_guard=False)
    raise ValueError(f"pandas.testing assertion `{name}` not lifted in v0")


def _pdt_subject_call(stmt: ast.stmt) -> Optional[ast.Call]:
    """The library call WHOSE RESULT is under test, for callsite-keying.
    ``assert_frame_equal(df.merge(other), expected)`` -> the ``df.merge(...)``
    Call (first comparand).  None when no call is the subject (keeps the
    test-name fallback)."""
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        if _pdt_call_name(stmt.value.func) in _PDT_EQUALITY:
            # Either comparand can be the value under test.
            for arg in stmt.value.args[:2]:
                if isinstance(arg, ast.Call):
                    return arg
    if isinstance(stmt, ast.Assert) and isinstance(stmt.test, ast.Compare):
        for operand in [stmt.test.left, *stmt.test.comparators]:
            if isinstance(operand, ast.Call):
                return operand
    return None


def _pdt_callsite_base(call, scope, call_vars, source_path: str) -> Optional[str]:
    """The callsite contract base (``pandas.merge#euf#...``) for ``call``,
    reusing the pytest lifter's argument-keyed naming so a pandas.testing
    assertion and a plain pytest assertion about the SAME pandas call land on
    the SAME name and conjoin.  None when ``call`` is not a module-function
    callsite or its args are not all concrete."""
    origin = _call_origin_from_expr(call)
    if origin is None:
        return None
    euf_term = _call_result_term(call, origin, scope, call_vars)
    if not (
        euf_term is not None
        and isinstance(euf_term, _Ctor)
        and euf_term.args
        and _euf_args_all_concrete(euf_term)
    ):
        return None
    origin.arg_sig = _canonical_term_sig(euf_term)
    return _callsite_contract_base(origin, source_path)


# --- statement gating (mirrors the numpy/pytest lifter's permitted set) -------


def _unsupported_stmt(stmt: ast.stmt) -> Optional[str]:
    """Reason iff ``stmt`` is unsupported for a pandas.testing body, else None.

    Permitted: simple-name ``Assign``/``AnnAssign`` (opaque SSA binding), bare
    ``Assert``, a recognised pandas.testing assertion call, ``Pass``.  Mutation,
    control flow, imports, and any OTHER expression statement are refused —
    soundness depends on bound values being stable across statements."""
    if isinstance(stmt, ast.Assert) or isinstance(stmt, ast.Pass):
        return None
    if _pdt_assertion_name(stmt) is not None:
        return None
    if isinstance(stmt, ast.Assign):
        for tgt in stmt.targets:
            if not isinstance(tgt, ast.Name):
                return (
                    f"non-simple assignment target `{_unparse(tgt)[:60]}` "
                    "(subscript/attribute mutation is not soundly liftable)"
                )
        return None
    if isinstance(stmt, ast.AnnAssign):
        if not isinstance(stmt.target, ast.Name):
            return f"non-simple annotated-assignment target `{_unparse(stmt.target)[:60]}`"
        return None
    if isinstance(stmt, ast.Expr):
        return (
            f"non-assertion expression statement `{_unparse(stmt)[:60]}` "
            "(a side-effecting call could mutate a bound value)"
        )
    return f"unsupported statement kind `{type(stmt).__name__}` in pandas.testing test"


def _classify_pandas_testing(
    body: Sequence[ast.stmt],
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> bool:
    """Claim+lift a test whose body uses pandas.testing assertions.

    Returns True iff this seat claimed the test (lifted OR loudly refused),
    False iff the body has no pandas.testing assertion at all (not our test).
    """
    if not any(_pdt_assertion_name(s) is not None for s in body):
        return False

    unsupported = [r for s in body if (r := _unsupported_stmt(s)) is not None]
    if unsupported:
        out.claimed_tests.add(test_name)
        out.seen += 1
        for reason in unsupported:
            out.warnings.append(
                LiftWarning(source_path, test_name,
                            f"layer2 pandas-testing: LOUD REFUSAL — {reason}")
            )
        return True

    ssa_current: Dict[str, Term] = {}
    ssa_versions: Dict[str, int] = {}
    skipped: List[str] = []

    # Pre-key every call occurrence to a LOCATION-unique opaque var so the same
    # call text at two locations yields DISTINCT terms (no false contradiction).
    call_vars = {}
    for stmt in body:
        for c in ast.walk(stmt):
            if isinstance(c, ast.Call):
                k = _call_key(c)
                if k not in call_vars:
                    call_vars[k] = make_var(f"__call${c.lineno}${c.col_offset}")

    keyed: List = []
    for stmt in body:
        if isinstance(stmt, (ast.Assign, ast.AnnAssign)):
            target = stmt.targets[0].id if isinstance(stmt, ast.Assign) else stmt.target.id
            version = ssa_versions.get(target, 0)
            ssa_versions[target] = version + 1
            ssa_current[target] = make_var(f"{target}${version}")
            continue
        if isinstance(stmt, ast.Pass):
            continue
        scope = _ValueScope(current=dict(ssa_current))
        try:
            if _pdt_assertion_name(stmt) is not None:
                atom = _lift_pdt_assertion_scoped(stmt.value, scope, call_vars)
            else:  # bare assert — conjoin for a complete per-test claim
                atom = _lift_assertion_stmt_scoped(stmt, scope, call_vars)
        except ValueError as e:
            skipped.append(f"`{_unparse(stmt)[:60]}`: {e}")
            continue
        subject = _pdt_subject_call(stmt)
        base = (
            _pdt_callsite_base(subject, scope, call_vars, source_path)
            if subject is not None
            else None
        )
        name = f"{base}::assertion" if base is not None else test_name
        keyed.append((name, atom))

    if not keyed:
        out.claimed_tests.add(test_name)
        out.seen += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 pandas-testing: 0 assertions liftable; "
                        f"skipped: {'; '.join(skipped)}")
        )
        return True

    out.claimed_tests.add(test_name)
    out.seen += 1
    groups: "OrderedDict[str, List]" = OrderedDict()
    for name, atom in keyed:
        groups.setdefault(name, []).append(atom)
    for name, atoms_g in groups.items():
        inv = atoms_g[0] if len(atoms_g) == 1 else and_(atoms_g)
        out.decls.append(ContractDecl(name=name, inv=inv))
    out.lifted += 1
    if skipped:
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 pandas-testing: {len(skipped)} assertion(s) "
                        f"skipped from conjunction: {'; '.join(skipped)}")
        )
    return True


def lift_file_pandas_testing(source: str, source_path: str) -> Layer2Output:
    """Public entry: lift pandas.testing assertions from a test file."""
    out = Layer2Output()
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as e:
        out.warnings.append(
            LiftWarning(source_path, "<file>", f"layer2 pandas-testing: failed to parse: {e}")
        )
        return out
    # Resolve module aliases (``import pandas as pd`` -> pd->pandas) so attribute
    # calls key to the qualified callsite -- the same map the pytest/numpy
    # lifters use, so all three surfaces produce identical ``pandas.<op>#euf#...``
    # names.
    prev_aliases = _l2._CURRENT_MODULE_ALIASES
    _l2._CURRENT_MODULE_ALIASES = _l2._collect_module_aliases(tree)
    try:
        for fn, class_name in _iter_test_functions(tree):
            test_name = f"{class_name}::{fn.name}" if class_name else fn.name
            body = _strip_self(fn.body, fn)
            _classify_pandas_testing(body, test_name, source_path, out)
    finally:
        _l2._CURRENT_MODULE_ALIASES = prev_aliases
    return out
