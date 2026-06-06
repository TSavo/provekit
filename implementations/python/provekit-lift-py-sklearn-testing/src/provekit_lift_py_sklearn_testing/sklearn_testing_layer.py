# SPDX-License-Identifier: Apache-2.0
"""scikit-learn ``sklearn.utils._testing`` assertion-vocabulary lift surface.

A SEPARATE federation seat, the sibling of the numpy.testing and pandas.testing
seats. ``sklearn.utils._testing`` is sklearn's own numpy-style assertion
vocabulary (the alias the suite imports, often as ``from sklearn.utils._testing
import ...``); pytest knows nothing about it, so it gets its OWN seat, reusing
the pytest package's term/formula machinery so equivalent claims federate.

SOUNDNESS -- the EXACT/APPROXIMATE split is the whole game, and sklearn is the
most approximate-dominated of the three: of its assertion vocabulary, ONLY
``assert_array_equal`` is exact. Everything else compares within a tolerance:

  EXACT (lifted as ``=``; sound for the CONSISTENCY pass, which records opaque
  relations between terms and never evaluates them):
    assert_array_equal  -> ``=``

  APPROXIMATE (LOUD REFUSE -- ``a ~= b`` within tolerance is NOT ``a = b``;
  lifting it as ``=`` would FALSE-PASS, claiming an exactness sklearn never
  checked):
    assert_allclose, assert_array_almost_equal, assert_almost_equal,
    assert_allclose_dense_sparse

  OTHER shapes (order / docstring / script-runner) -> LOUD REFUSE for now:
    assert_array_less, assert_docstring_consistency,
    assert_run_python_script_without_output

This is where sklearn's correctness is witnessed, not consistency-proven: the
approximate numeric assertions dominate, so the consistency teeth come from the
exact ``assert_array_equal`` and from scalar bare asserts (the pytest seat),
while the WITNESS axis (pytest-witness re-running the suite) carries the weight.

Binding handling mirrors the numpy/pandas lifters EXACTLY: simple ``x = <expr>``
bindings are SSA-versioned opaque free vars; any mutation, control flow, or
non-assertion side-effecting expression statement is LOUDLY REFUSED, because
soundness depends on bound values being stable.
"""

from __future__ import annotations

import ast
from collections import OrderedDict
from typing import Dict, List, Optional, Sequence

# Reuse the pytest seat's shared term/formula machinery so equivalent claims
# federate (same imports the numpy/pandas testing seats use).
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
    _translate_bool_expr_scoped,
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

# --- sklearn.utils._testing vocabulary ----------------------------------------

# Exact equality -> ``=`` (the only exact assertion sklearn ships).
_SKT_EQUALITY = {"assert_array_equal"}
# Approximate -> LOUD REFUSE (must never become ``=``).
_SKT_APPROX = {
    "assert_allclose",
    "assert_array_almost_equal",
    "assert_almost_equal",
    "assert_allclose_dense_sparse",
}
# Other shapes recognised (claim+refuse loudly rather than silently ignore) but
# not lifted in v0.
_SKT_OTHER = {
    "assert_array_less",
    "assert_docstring_consistency",
    "assert_run_python_script_without_output",
}
_SKT_ALL = _SKT_EQUALITY | _SKT_APPROX | _SKT_OTHER


def _skt_call_name(func: ast.expr) -> Optional[str]:
    """Final callee name for ``assert_array_equal(...)`` (bare ``Name``) AND
    ``skt.assert_array_equal(...)`` / attribute callees. None if not a
    simple name/attribute."""
    if isinstance(func, ast.Name):
        return func.id
    if isinstance(func, ast.Attribute):
        return func.attr
    return None


def _skt_assertion_name(stmt: ast.stmt) -> Optional[str]:
    """The sklearn.utils._testing assertion name iff ``stmt`` is ``<skt-call>(...)``
    as a bare expression statement; else None."""
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        name = _skt_call_name(stmt.value.func)
        if name in _SKT_ALL:
            return name
    return None


def _lift_skt_assertion_scoped(call: ast.Call, scope: _ValueScope, call_vars) -> Formula:
    """Lift one sklearn.utils._testing assertion call under the current SSA scope.
    Raises ValueError (loud, recorded as a skip) for approximate / unsupported
    forms so they are NEVER silently lifted as exact equality."""
    name = _skt_call_name(call.func)
    if call.keywords:
        kw = {k.arg for k in call.keywords}
        if kw - {"err_msg", "verbose"}:
            raise ValueError(f"{name}: keyword arg(s) {sorted(kw)} not liftable")
    if name in _SKT_EQUALITY:
        if len(call.args) < 2:
            raise ValueError(f"{name} expects at least 2 positional args")
        l = _translate_term_scoped(call.args[0], scope, call_vars)
        r = _translate_term_scoped(call.args[1], scope, call_vars)
        return comparison_with_none_guard("=", l, r, emit_none_guard=False)
    if name in _SKT_APPROX:
        raise ValueError(
            f"approximate assertion `{name}` is not exact equality "
            "(a ~= b within tolerance); refused to avoid false-pass"
        )
    raise ValueError(f"sklearn.utils._testing assertion `{name}` not lifted in v0")


def _skt_subject_call(stmt: ast.stmt) -> Optional[ast.Call]:
    """The library call WHOSE RESULT is under test, for callsite-keying.
    ``assert_array_equal(model.predict(X), expected)`` -> the ``model.predict(X)``
    Call (first comparand). None when no call is the subject."""
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        if _skt_call_name(stmt.value.func) in _SKT_EQUALITY:
            for arg in stmt.value.args[:2]:
                if isinstance(arg, ast.Call):
                    return arg
    if isinstance(stmt, ast.Assert) and isinstance(stmt.test, ast.Compare):
        for operand in [stmt.test.left, *stmt.test.comparators]:
            if isinstance(operand, ast.Call):
                return operand
    return None


def _skt_callsite_base(call, scope, call_vars, source_path: str) -> Optional[str]:
    """The callsite contract base for ``call``, reusing the pytest lifter's
    argument-keyed naming so a sklearn.utils._testing assertion and a plain pytest
    assertion about the SAME call land on the SAME name and conjoin. None when
    ``call`` is not a concrete-arg module-function callsite."""
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


# --- statement gating (mirrors the numpy/pandas lifter's permitted set) --------


def _unsupported_stmt(stmt: ast.stmt) -> Optional[str]:
    """Reason iff ``stmt`` is unsupported for a sklearn.utils._testing body, else
    None. Permitted: simple-name ``Assign``/``AnnAssign`` (opaque SSA binding),
    bare ``Assert``, a recognised assertion call, ``Pass``. Everything else is
    refused -- soundness depends on bound values being stable."""
    if isinstance(stmt, ast.Assert) or isinstance(stmt, ast.Pass):
        return None
    if _skt_assertion_name(stmt) is not None:
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
    return f"unsupported statement kind `{type(stmt).__name__}` in sklearn.utils._testing test"


def _classify_sklearn_testing(
    body: Sequence[ast.stmt],
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> bool:
    """Claim+lift a test whose body uses sklearn.utils._testing assertions.

    Returns True iff this seat claimed the test (lifted OR loudly refused),
    False iff the body has no sklearn.utils._testing assertion (not our test)."""
    if not any(_skt_assertion_name(s) is not None for s in body):
        return False

    unsupported = [r for s in body if (r := _unsupported_stmt(s)) is not None]
    if unsupported:
        out.claimed_tests.add(test_name)
        out.seen += 1
        for reason in unsupported:
            out.warnings.append(
                LiftWarning(source_path, test_name,
                            f"layer2 sklearn-testing: LOUD REFUSAL -- {reason}")
            )
        return True

    ssa_current: Dict[str, Term] = {}
    ssa_versions: Dict[str, int] = {}
    skipped: List[str] = []

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
            if _skt_assertion_name(stmt) is not None:
                atom = _lift_skt_assertion_scoped(stmt.value, scope, call_vars)
            else:  # bare assert -- conjoin for a complete per-test claim
                atom = _lift_assertion_stmt_scoped(stmt, scope, call_vars)
        except ValueError as e:
            skipped.append(f"`{_unparse(stmt)[:60]}`: {e}")
            continue
        subject = _skt_subject_call(stmt)
        base = (
            _skt_callsite_base(subject, scope, call_vars, source_path)
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
                        f"layer2 sklearn-testing: 0 assertions liftable; "
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
                        f"layer2 sklearn-testing: {len(skipped)} assertion(s) "
                        f"skipped from conjunction: {'; '.join(skipped)}")
        )
    return True


def lift_file_sklearn_testing(source: str, source_path: str) -> Layer2Output:
    """Public entry: lift sklearn.utils._testing assertions from a test file."""
    out = Layer2Output()
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as e:
        out.warnings.append(
            LiftWarning(source_path, "<file>", f"layer2 sklearn-testing: failed to parse: {e}")
        )
        return out
    prev_aliases = _l2._CURRENT_MODULE_ALIASES
    _l2._CURRENT_MODULE_ALIASES = _l2._collect_module_aliases(tree)
    try:
        for fn, class_name in _iter_test_functions(tree):
            test_name = f"{class_name}::{fn.name}" if class_name else fn.name
            body = _strip_self(fn.body, fn)
            _classify_sklearn_testing(body, test_name, source_path, out)
    finally:
        _l2._CURRENT_MODULE_ALIASES = prev_aliases
    return out
