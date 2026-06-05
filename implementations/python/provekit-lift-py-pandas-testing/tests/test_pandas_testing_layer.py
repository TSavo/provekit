# SPDX-License-Identifier: Apache-2.0
"""Tests for the pandas.testing assertion lift seat.

Mirrors the numpy.testing seat's discrimination discipline: every positive lift
is paired with the refusal that proves the lifter is sound, not permissive. The
headline property is the APPROXIMATE-BY-DEFAULT refusal — pandas compares floats
with a tolerance unless ``check_exact=True`` is pinned, so an un-pinned
``assert_frame_equal`` must NOT be lifted as exact equality.
"""

import provekit_lift_py_tests.ir as ir
from provekit_lift_py_pandas_testing import lift_file_pandas_testing


def _lift(src: str):
    return lift_file_pandas_testing(src, "<test>")


def _decl_lefts(out):
    """The set of left-hand variable names across all lifted equality atoms."""
    lefts = set()
    for d in out.decls:
        for atom in _atoms(d.inv):
            lhs = _eq_lhs(atom)
            if lhs is not None:
                lefts.add(lhs)
    return lefts


def _atoms(formula):
    # and_ wraps operands; a single atom is returned bare.
    return getattr(formula, "operands", None) or [formula]


def _eq_lhs(atom):
    args = getattr(atom, "args", None)
    if args and len(args) >= 1:
        return getattr(args[0], "name", None)
    return None


# --- positive lift: check_exact=True is liftable as equality -----------------


def test_frame_equal_with_check_exact_lifts_as_equality():
    out = _lift("def test_a():\n    assert_frame_equal(x, y, check_exact=True)\n")
    assert out.lifted == 1, out.warnings
    assert len(out.decls) == 1


def test_series_equal_with_check_exact_lifts():
    out = _lift("def test_a():\n    assert_series_equal(x, y, check_exact=True)\n")
    assert out.lifted == 1, out.warnings


def test_index_equal_with_check_exact_lifts():
    out = _lift("def test_a():\n    assert_index_equal(x, y, check_exact=True)\n")
    assert out.lifted == 1, out.warnings


def test_attribute_callee_pd_testing():
    # pd.testing.assert_frame_equal(...) / tm.assert_frame_equal(...): attribute
    # callee, same as the bare name.
    out = _lift("def test_a():\n    pd.testing.assert_frame_equal(x, y, check_exact=True)\n")
    assert out.lifted == 1, out.warnings


# --- THE headline refusal: approximate by default ----------------------------


def test_frame_equal_without_check_exact_refused():
    # No check_exact=True -> pandas compares floats with tolerance -> REFUSE,
    # because lifting it as `=` would claim exact equality pandas never checked.
    out = _lift("def test_a():\n    assert_frame_equal(x, y)\n")
    assert out.lifted == 0
    assert out.seen == 1  # claimed (not silently ignored)
    assert not out.decls
    assert any("approximate by default" in w.reason for w in out.warnings), out.warnings


def test_check_exact_false_refused():
    # Explicit check_exact=False is still approximate -> REFUSE.
    out = _lift("def test_a():\n    assert_frame_equal(x, y, check_exact=False)\n")
    assert out.lifted == 0
    assert any("approximate by default" in w.reason for w in out.warnings), out.warnings


# --- relation-altering keywords are refused ----------------------------------


def test_check_like_refused():
    # check_like=True ignores row/column ORDER -> a different relation -> REFUSE
    # even though check_exact is pinned.
    out = _lift(
        "def test_a():\n    assert_frame_equal(x, y, check_exact=True, check_like=True)\n"
    )
    assert out.lifted == 0
    assert any("may change the relation" in w.reason for w in out.warnings), out.warnings


def test_rtol_refused():
    out = _lift(
        "def test_a():\n    assert_frame_equal(x, y, check_exact=True, rtol=1e-3)\n"
    )
    assert out.lifted == 0
    assert any("may change the relation" in w.reason for w in out.warnings), out.warnings


# --- consistency teeth: scalar bare-asserts riding a claimed pandas test ------
#
# pandas.testing has NO scalar-equality assertion (assert_frame_equal is
# frame-level, opaque-EUF on both sides), so this seat cannot manufacture
# scalar contradiction teeth on its OWN vocabulary. The teeth come from bare
# scalar asserts, which this seat lifts ONLY when they ride alongside a claimed
# pandas.testing assertion (mixed body). A pure-scalar body is the pytest seat's
# job (see test_no_pandas_testing_assertion_not_claimed).


def test_same_bound_var_contradicts_on_scalar():
    # assert_frame_equal claims the test (frame eq is opaque, no teeth); the
    # contradiction-capable atoms are the two scalar asserts about the SAME
    # bound var: =(n$0, 5) ^ =(n$0, 6) -> UNSAT at prove.
    out = _lift(
        "def test_a():\n"
        "    n = total.sum()\n"
        "    assert_frame_equal(x, y, check_exact=True)\n"
        "    assert n == 5\n"
        "    assert n == 6\n"
    )
    assert out.lifted == 1, out.warnings
    lefts = _decl_lefts(out)
    assert "n$0" in lefts, lefts  # same SSA var both times -> z3 contradiction


def test_reassignment_ssa_independence_no_false_refuse():
    # DISCRIMINATION (false-refuse gate): n is rebound between the two asserts,
    # so =(n$0,5) ^ =(n$1,6) is SAT (distinct SSA vars) — NOT a contradiction.
    out = _lift(
        "def test_a():\n"
        "    assert_frame_equal(x, y, check_exact=True)\n"
        "    n = a.sum()\n"
        "    assert n == 5\n"
        "    n = b.sum()\n"
        "    assert n == 6\n"
    )
    assert out.lifted == 1, out.warnings
    lefts = _decl_lefts(out)
    assert "n$0" in lefts and "n$1" in lefts, lefts


# --- statement gating: mutation / control flow / side effects refused ---------


def test_subscript_mutation_refused():
    out = _lift(
        "def test_a():\n"
        "    a = make()\n"
        "    a[0] = 5\n"
        "    assert_frame_equal(a, y, check_exact=True)\n"
    )
    assert out.lifted == 0
    assert any("not soundly liftable" in w.reason for w in out.warnings), out.warnings


def test_side_effect_call_refused():
    # a.sort_values(inplace=True) could mutate `a`; refuse rather than ignore.
    out = _lift(
        "def test_a():\n"
        "    a = make()\n"
        "    a.reset_index()\n"
        "    assert_frame_equal(a, y, check_exact=True)\n"
    )
    assert out.lifted == 0
    assert any("side-effecting call" in w.reason for w in out.warnings), out.warnings


def test_for_loop_refused():
    out = _lift(
        "def test_a():\n"
        "    assert_frame_equal(x, y, check_exact=True)\n"
        "    for i in range(2):\n"
        "        assert i == 0\n"
    )
    assert out.lifted == 0


# --- not our test ------------------------------------------------------------


def test_no_pandas_testing_assertion_not_claimed():
    # A body with no pandas.testing assertion is NOT this seat's test.
    out = _lift("def test_a():\n    assert 1 == 1\n")
    assert out.seen == 0
    assert not out.decls


def test_other_recognized_but_refused():
    # assert_categorical_equal is recognized (claimed) but not lifted in v0.
    out = _lift("def test_a():\n    assert_categorical_equal(x, y)\n")
    assert out.seen == 1
    assert out.lifted == 0
    assert not out.decls


def test_broad_tm_vocabulary_claimed_and_refused():
    # The whole pandas._testing (tm) family is recognised so nothing real is
    # SILENTLY skipped. Each non-lifted member is claimed + loudly refused.
    for name in (
        "assert_numpy_array_equal",
        "assert_almost_equal",
        "assert_dict_equal",
        "assert_is_sorted",
        "assert_period_array_equal",
        "assert_produces_warning",
    ):
        out = _lift(f"def test_a():\n    tm.{name}(x, y)\n")
        assert out.seen == 1, (name, out.warnings)
        assert out.lifted == 0, name
        assert not out.decls, name


def test_assert_equal_not_claimed_by_pandas_seat():
    # assert_equal is a cross-library generic name owned by the numpy/generic
    # seat; the pandas seat must NOT claim it (would double-claim).
    out = _lift("def test_a():\n    assert_equal(x, 1)\n")
    assert out.seen == 0
    assert not out.decls
