# SPDX-License-Identifier: Apache-2.0
"""NumPy ``numpy.testing`` assertion-vocabulary lift surface.

A SEPARATE federation seat from the pytest ``assert`` lifter
(``provekit_lift_py_tests``).  pytest natively collects bare ``assert`` and
``unittest.TestCase``, so those are the pytest lifter's job.  ``numpy.testing``
(``assert_equal``, ``assert_array_equal``, ``assert_``, ...) is a THIRD-PARTY
assertion vocabulary that pytest knows nothing about, so it gets its OWN seat —
which emits the same ProofIR consistency facts (reusing the pytest package's
term/formula machinery) so that ``assert_equal(a, b)`` and a bare
``assert a == b`` describe the same relation.

SOUNDNESS — the EXACT/APPROXIMATE split is the whole game:

  EXACT (lifted; sound for the CONSISTENCY pass, which records opaque relations
  between terms and never evaluates them):
    assert_equal / assert_array_equal / assert_equals  -> ``=``
    assert_                                             -> truthiness

  APPROXIMATE (LOUD REFUSE — ``a ≈ b`` within tolerance is NOT ``a = b``;
  lifting it as ``=`` would FALSE-PASS on two asserts whose tolerance ranges
  overlap):
    assert_allclose, assert_almost_equal, assert_array_almost_equal,
    assert_approx_equal, assert_array_almost_equal_nulp, assert_array_max_ulp

  OTHER shapes (exception/warning/string/order assertions) -> LOUD REFUSE for
  now (a later phase can map e.g. assert_array_less -> ``<`` and assert_raises
  -> a raises obligation):
    assert_raises, assert_warns, assert_no_warnings, assert_string_equal,
    assert_array_less, assert_warns_message, ...

Binding handling mirrors the pytest lifter's mixed-body pattern EXACTLY: simple
``x = <expr>`` bindings are SSA-versioned opaque free vars (reassignment bumps
the version so two asserts about different SSA generations never falsely
conjoin), and any mutation (subscript/attribute-assign), control flow, or
non-assertion side-effecting expression statement is LOUDLY REFUSED — because
the lifter cannot prove the bound value is unchanged across it.
"""

from __future__ import annotations

import ast
from collections import OrderedDict
from typing import Dict, List, Optional, Sequence

# Reuse the pytest seat's shared term/formula machinery so equivalent claims
# federate.  (These live in provekit_lift_py_tests today; if a shared
# libprovekit-py term core is extracted later, re-point these imports.)
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

# --- numpy.testing vocabulary -------------------------------------------------

# Exact equality -> ``=`` (same constructor the pytest lifter uses for ==).
_NPT_EQUALITY = {"assert_equal", "assert_array_equal", "assert_equals"}
# Truthiness -> reuse the bool-expr lifter on the single argument.
_NPT_TRUTH = {"assert_"}
# Approximate -> LOUD REFUSE (must never become ``=``).
_NPT_APPROX = {
    "assert_allclose",
    "assert_almost_equal",
    "assert_array_almost_equal",
    "assert_approx_equal",
    "assert_array_almost_equal_nulp",
    "assert_array_max_ulp",
}
# Other numpy.testing shapes recognised (so we claim+refuse loudly rather than
# silently ignore) but not yet lifted.
_NPT_OTHER = {
    "assert_raises",
    "assert_raises_regex",
    "assert_warns",
    "assert_no_warnings",
    "assert_string_equal",
    "assert_array_less",
    "assert_warns_message",
    "assert_array_equal_nan",
}
_NPT_ALL = _NPT_EQUALITY | _NPT_TRUTH | _NPT_APPROX | _NPT_OTHER


def _npt_call_name(func: ast.expr) -> Optional[str]:
    """Final callee name for ``assert_equal(...)`` (bare ``Name``) AND
    ``np.testing.assert_equal(...)`` / ``npt.assert_equal(...)`` (``Attribute``).
    Returns None if the callee is not a simple name/attribute.
    """
    if isinstance(func, ast.Name):
        return func.id
    if isinstance(func, ast.Attribute):
        return func.attr
    return None


def _npt_assertion_name(stmt: ast.stmt) -> Optional[str]:
    """The numpy.testing assertion name iff ``stmt`` is ``<npt-call>(...)`` as a
    bare expression statement; else None."""
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        name = _npt_call_name(stmt.value.func)
        if name in _NPT_ALL:
            return name
    return None


def _lift_npt_assertion_scoped(call: ast.Call, scope: _ValueScope, call_vars) -> Formula:
    """Lift one numpy.testing assertion call under the current SSA scope.
    ``call_vars`` location-keys recomputed call results so they are NOT assumed
    stable across statements.  Raises ValueError (loud, recorded as a skip) for
    approximate / unsupported forms so they are NEVER silently lifted as exact
    equality."""
    name = _npt_call_name(call.func)
    if call.keywords:
        # err_msg=/verbose= are common and harmless, but a keyword in the
        # comparand position is not order-stably liftable; refuse loudly.
        kw = {k.arg for k in call.keywords}
        if kw - {"err_msg", "verbose"}:
            raise ValueError(
                f"{name}: keyword arg(s) {sorted(kw)} not liftable"
            )
    if name in _NPT_EQUALITY:
        if len(call.args) < 2:
            raise ValueError(f"{name} expects at least 2 positional args")
        l = _translate_term_scoped(call.args[0], scope, call_vars)
        r = _translate_term_scoped(call.args[1], scope, call_vars)
        # Same atom the pytest/unittest path builds for equality.
        return comparison_with_none_guard("=", l, r, emit_none_guard=False)
    if name in _NPT_TRUTH:
        if len(call.args) < 1:
            raise ValueError(f"{name} expects 1 positional arg")
        return _translate_bool_expr_scoped(call.args[0], scope, call_vars)
    if name in _NPT_APPROX:
        raise ValueError(
            f"approximate assertion `{name}` is not exact equality "
            "(a ≈ b within tolerance); refused to avoid false-pass on "
            "overlapping tolerances"
        )
    raise ValueError(f"numpy.testing assertion `{name}` not lifted in v0")


def _npt_subject_call(stmt: ast.stmt) -> Optional[ast.Call]:
    """The library call WHOSE RESULT is under test, for callsite-keying.
    ``assert_equal(np.add(2,3), 5)`` -> the ``np.add(2,3)`` Call (first comparand);
    ``assert np.add(2,3) == 5`` -> the first Call operand of the comparison. None
    when no call is the subject (keeps the test-name fallback)."""
    if isinstance(stmt, ast.Expr) and isinstance(stmt.value, ast.Call):
        if _npt_call_name(stmt.value.func) in _NPT_EQUALITY and stmt.value.args:
            arg0 = stmt.value.args[0]
            if isinstance(arg0, ast.Call):
                return arg0
    if isinstance(stmt, ast.Assert) and isinstance(stmt.test, ast.Compare):
        for operand in [stmt.test.left, *stmt.test.comparators]:
            if isinstance(operand, ast.Call):
                return operand
    return None


def _npt_callsite_base(call, scope, call_vars, source_path: str) -> Optional[str]:
    """The callsite contract base (``numpy.add#euf#...``) for ``call``, reusing
    the pytest lifter's argument-keyed naming so a numpy.testing assertion and a
    plain pytest assertion about the SAME numpy call land on the SAME name and
    conjoin. None when ``call`` is not a module-function callsite."""
    origin = _call_origin_from_expr(call)
    if origin is None:
        return None
    euf_term = _call_result_term(call, origin, scope, call_vars)
    # Only a CONCRETE-ARG callsite (``numpy.add(2,3)``) splits into its own
    # argument-keyed contract -- that is what conjoins across tests/files/proofs.
    # Symbolic-arg or non-liftable calls return None -> the assertion stays in the
    # test-name group (unchanged behaviour, no spurious cross-location unify).
    if not (
        euf_term is not None
        and isinstance(euf_term, _Ctor)
        and euf_term.args
        and _euf_args_all_concrete(euf_term)
    ):
        return None
    origin.arg_sig = _canonical_term_sig(euf_term)
    return _callsite_contract_base(origin, source_path)


# --- statement gating (mirrors the pytest lifter's mixed-body permitted set) --


def _unsupported_stmt(stmt: ast.stmt) -> Optional[str]:
    """Reason iff ``stmt`` is unsupported for a numpy.testing body, else None.

    Permitted: simple-name ``Assign``/``AnnAssign`` (opaque SSA binding), bare
    ``Assert``, a recognised numpy.testing assertion call, ``Pass``.  Everything
    else — mutation (subscript/attr assign), control flow, imports, and any
    OTHER expression-statement (a side-effecting call could mutate a bound
    value) — is refused, because soundness depends on bound values being stable.
    """
    if isinstance(stmt, ast.Assert) or isinstance(stmt, ast.Pass):
        return None
    if _npt_assertion_name(stmt) is not None:
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
    return f"unsupported statement kind `{type(stmt).__name__}` in numpy.testing test"


def _classify_numpy_testing(
    body: Sequence[ast.stmt],
    test_name: str,
    source_path: str,
    out: Layer2Output,
) -> bool:
    """Claim+lift a test whose body uses numpy.testing assertions.

    Returns True iff this seat claimed the test (lifted OR loudly refused),
    False iff the body has no numpy.testing assertion at all (not our test).
    """
    if not any(_npt_assertion_name(s) is not None for s in body):
        return False

    unsupported = [r for s in body if (r := _unsupported_stmt(s)) is not None]
    if unsupported:
        out.claimed_tests.add(test_name)
        out.seen += 1
        for reason in unsupported:
            out.warnings.append(
                LiftWarning(source_path, test_name,
                            f"layer2 numpy-testing: LOUD REFUSAL — {reason}")
            )
        return True

    # SSA scope build (identical model to the pytest lifter's mixed-body).
    ssa_current: Dict[str, Term] = {}
    ssa_versions: Dict[str, int] = {}
    skipped: List[str] = []

    # SOUNDNESS — impure repeated calls: a call RESULT (repr(x), len(x), ...) is
    # NOT assumed stable across statements, because an intervening call can
    # change global/observable state (e.g. np.set_printoptions changes repr(x)).
    # Pre-key every call occurrence to a LOCATION-unique opaque var so the same
    # call text at two locations yields DISTINCT terms (no false contradiction).
    # Bound variables (SSA vars) are unaffected — they remain stable and still
    # detect genuine same-variable contradictions.
    call_vars = {}
    for stmt in body:
        for c in ast.walk(stmt):
            if isinstance(c, ast.Call):
                k = _call_key(c)
                if k not in call_vars:
                    call_vars[k] = make_var(f"__call${c.lineno}${c.col_offset}")

    # (contract name, atom) per assertion. The name is the CALLSITE UNDER TEST
    # (``numpy.add#euf#...``) so the contract conjoins with any other assertion
    # about the same call -- in this file, or in a consumer's imported proof.
    # Falls back to the test name only when there is no module-call subject.
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
            if _npt_assertion_name(stmt) is not None:
                atom = _lift_npt_assertion_scoped(stmt.value, scope, call_vars)
            else:  # bare assert — conjoin for a complete per-test claim
                atom = _lift_assertion_stmt_scoped(stmt, scope, call_vars)
        except ValueError as e:
            skipped.append(f"`{_unparse(stmt)[:60]}`: {e}")
            continue
        subject = _npt_subject_call(stmt)
        base = (
            _npt_callsite_base(subject, scope, call_vars, source_path)
            if subject is not None
            else None
        )
        keyed.append((base or test_name, atom))

    if not keyed:
        out.claimed_tests.add(test_name)
        out.seen += 1
        out.warnings.append(
            LiftWarning(source_path, test_name,
                        f"layer2 numpy-testing: 0 assertions liftable; "
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
                        f"layer2 numpy-testing: {len(skipped)} assertion(s) "
                        f"skipped from conjunction: {'; '.join(skipped)}")
        )
    return True


def lift_file_numpy_testing(source: str, source_path: str) -> Layer2Output:
    """Public entry: lift numpy.testing assertions from a test file."""
    out = Layer2Output()
    try:
        tree = ast.parse(source, filename=source_path)
    except SyntaxError as e:
        out.warnings.append(
            LiftWarning(source_path, "<file>", f"layer2 numpy-testing: failed to parse: {e}")
        )
        return out
    # Resolve module aliases (``import numpy as np`` -> np->numpy) so attribute
    # calls key to the qualified callsite -- the same map the pytest lifter uses,
    # so both surfaces produce identical ``numpy.add#euf#...`` names.
    prev_aliases = _l2._CURRENT_MODULE_ALIASES
    _l2._CURRENT_MODULE_ALIASES = _l2._collect_module_aliases(tree)
    try:
        for fn, class_name in _iter_test_functions(tree):
            test_name = f"{class_name}::{fn.name}" if class_name else fn.name
            body = _strip_self(fn.body, fn)
            _classify_numpy_testing(body, test_name, source_path, out)
    finally:
        _l2._CURRENT_MODULE_ALIASES = prev_aliases
    return out
