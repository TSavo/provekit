# SPDX-License-Identifier: Apache-2.0
"""Tests for the sklearn.utils._testing assertion lift seat.

Mirrors the numpy/pandas seats' discrimination discipline: every positive lift is
paired with the refusal that proves the lifter is sound. The headline for sklearn
is that it is the most approximate-dominated of the three -- only
``assert_array_equal`` is exact; the whole ``assert_allclose`` family is refused.
"""

import provekit_lift_py_tests.ir as ir  # noqa: F401
from provekit_lift_py_sklearn_testing import lift_file_sklearn_testing


def _lift(src: str):
    return lift_file_sklearn_testing(src, "<test>")


def _decl_lefts(out):
    lefts = set()
    for d in out.decls:
        for atom in (getattr(d.inv, "operands", None) or [d.inv]):
            args = getattr(atom, "args", None)
            if args:
                nm = getattr(args[0], "name", None)
                if nm is not None:
                    lefts.add(nm)
    return lefts


# --- positive: assert_array_equal is the one exact assertion ------------------


def test_assert_array_equal_lifts_as_equality():
    out = _lift("def test_a():\n    assert_array_equal(x, y)\n")
    assert out.lifted == 1, out.warnings
    assert len(out.decls) == 1


def test_attribute_callee():
    out = _lift("def test_a():\n    skt.assert_array_equal(x, y)\n")
    assert out.lifted == 1, out.warnings


def test_same_bound_var_contradicts():
    # =(r$0, 1) ^ =(r$0, 2) -> z3 UNSAT at prove (the consistency teeth).
    out = _lift(
        "def test_a():\n"
        "    r = make()\n"
        "    assert_array_equal(r, 1)\n"
        "    assert_array_equal(r, 2)\n"
    )
    assert out.lifted == 1, out.warnings
    assert _decl_lefts(out) == {"r$0"}, _decl_lefts(out)


def test_reassignment_ssa_independence_no_false_refuse():
    out = _lift(
        "def test_a():\n"
        "    r = a()\n"
        "    assert_array_equal(r, 1)\n"
        "    r = b()\n"
        "    assert_array_equal(r, 2)\n"
    )
    assert out.lifted == 1, out.warnings
    assert _decl_lefts(out) == {"r$0", "r$1"}, _decl_lefts(out)


# --- THE headline refusals: the approximate family ----------------------------


def test_assert_allclose_refused():
    out = _lift("def test_a():\n    assert_allclose(x, y)\n")
    assert out.lifted == 0
    assert out.seen == 1
    assert not out.decls
    assert any("approximate assertion" in w.reason for w in out.warnings), out.warnings


def test_assert_array_almost_equal_refused():
    out = _lift("def test_a():\n    assert_array_almost_equal(x, y)\n")
    assert out.lifted == 0
    assert any("approximate assertion" in w.reason for w in out.warnings), out.warnings


def test_assert_almost_equal_refused():
    out = _lift("def test_a():\n    assert_almost_equal(x, y)\n")
    assert out.lifted == 0
    assert any("approximate assertion" in w.reason for w in out.warnings), out.warnings


def test_assert_allclose_dense_sparse_refused():
    out = _lift("def test_a():\n    assert_allclose_dense_sparse(x, y)\n")
    assert out.lifted == 0
    assert any("approximate assertion" in w.reason for w in out.warnings), out.warnings


# --- other recognized-but-refused ---------------------------------------------


def test_assert_array_less_recognized_but_refused():
    out = _lift("def test_a():\n    assert_array_less(x, y)\n")
    assert out.seen == 1 and out.lifted == 0 and not out.decls


# --- mutation / control flow / not-our-test -----------------------------------


def test_subscript_mutation_refused():
    out = _lift("def test_a():\n    a = make()\n    a[0] = 5\n    assert_array_equal(a, 1)\n")
    assert out.lifted == 0
    assert any("not soundly liftable" in w.reason for w in out.warnings), out.warnings


def test_for_loop_refused():
    out = _lift(
        "def test_a():\n"
        "    assert_array_equal(x, 1)\n"
        "    for i in range(2):\n"
        "        assert i == 0\n"
    )
    assert out.lifted == 0


def test_no_sklearn_testing_assertion_not_claimed():
    out = _lift("def test_a():\n    assert 1 == 1\n")
    assert out.seen == 0
    assert not out.decls
