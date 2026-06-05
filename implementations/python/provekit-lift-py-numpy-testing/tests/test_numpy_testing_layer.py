# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import textwrap

from provekit_lift_py_numpy_testing import lift_file_numpy_testing
from provekit_lift_py_tests.ir import _Atomic, _Connective, _Var, _ConstInt


def _lift(src: str):
    return lift_file_numpy_testing(textwrap.dedent(src), "t.py")


def _only_decl(out):
    assert len(out.decls) == 1, [d.name for d in out.decls]
    return out.decls[0].inv


# --- EXACT vocabulary lifts ---------------------------------------------------


def test_assert_equal_lifts_as_equality():
    out = _lift("def test_a():\n    assert_equal(x, 1)\n")
    inv = _only_decl(out)
    assert isinstance(inv, _Atomic) and inv.name == "="
    assert "test_a" in out.claimed_tests
    assert out.lifted == 1


def test_assert_array_equal_lifts_as_equality():
    out = _lift("def test_a():\n    assert_array_equal(x, y)\n")
    inv = _only_decl(out)
    assert isinstance(inv, _Atomic) and inv.name == "="


def test_module_prefixed_call_lifts():
    # np.testing.assert_equal(...) — attribute callee, same as bare name.
    out = _lift("def test_a():\n    np.testing.assert_equal(x, 1)\n")
    inv = _only_decl(out)
    assert isinstance(inv, _Atomic) and inv.name == "="


def test_assert_truthiness_lifts():
    out = _lift("def test_a():\n    assert_(flag)\n")
    assert out.lifted == 1, out.warnings


def test_err_msg_keyword_is_permitted():
    out = _lift('def test_a():\n    assert_equal(x, 1, err_msg="boom")\n')
    assert out.lifted == 1, out.warnings


# --- Contradiction detection (teeth) + within-test conjunction ----------------


def test_two_exact_about_same_var_conjoin():
    # assert_equal(x,1); assert_equal(x,2) -> =(x,1) ^ =(x,2): UNSAT at prove.
    out = _lift("def test_a():\n    assert_equal(x, 1)\n    assert_equal(x, 2)\n")
    inv = _only_decl(out)
    assert isinstance(inv, _Connective) and inv.kind == "and"
    assert len(inv.operands) == 2
    # both atoms are equalities about the SAME var x.
    for op in inv.operands:
        assert isinstance(op, _Atomic) and op.name == "="
        assert isinstance(op.args[0], _Var) and op.args[0].name == "x"


# --- SSA binding handling (the prize shape) -----------------------------------


def test_binding_then_npt_lifts():
    out = _lift("def test_a():\n    a = make()\n    assert_equal(a, 5)\n")
    inv = _only_decl(out)
    assert isinstance(inv, _Atomic) and inv.name == "="
    assert isinstance(inv.args[0], _Var) and inv.args[0].name == "a$0"


def test_reassignment_ssa_independence_no_false_refuse():
    # DISCRIMINATION (false-refuse gate): a is rebound between the two asserts,
    # so =(a$0,1) ^ =(a$1,2) is SAT (distinct SSA vars) — NOT a contradiction.
    out = _lift(
        "def test_a():\n"
        "    a = make()\n"
        "    assert_equal(a, 1)\n"
        "    a = make2()\n"
        "    assert_equal(a, 2)\n"
    )
    inv = _only_decl(out)
    assert isinstance(inv, _Connective) and inv.kind == "and"
    vars_seen = {op.args[0].name for op in inv.operands if isinstance(op.args[0], _Var)}
    assert vars_seen == {"a$0", "a$1"}, vars_seen


# --- APPROXIMATE -> LOUD REFUSE (the soundness crux) --------------------------


def test_assert_allclose_refused_not_lifted_as_equality():
    out = _lift("def test_a():\n    assert_allclose(x, 1.0)\n")
    assert out.decls == [], "approx must NOT lift as exact equality"
    assert "test_a" in out.claimed_tests  # claimed (loud), not silent
    assert any("approx" in w.reason or "tolerance" in w.reason for w in out.warnings), out.warnings


def test_assert_almost_equal_refused():
    out = _lift("def test_a():\n    assert_almost_equal(x, 1.0)\n")
    assert out.decls == []
    assert "test_a" in out.claimed_tests


def test_partial_exact_plus_approx_lifts_exact_warns_approx():
    # exact lifts, approx skipped+warned (partial) — never the approx as =.
    out = _lift(
        "def test_a():\n"
        "    assert_equal(x, 1)\n"
        "    assert_allclose(x, 1.0)\n"
    )
    inv = _only_decl(out)
    assert isinstance(inv, _Atomic) and inv.name == "="  # only the exact one
    assert any("skipped" in w.reason for w in out.warnings), out.warnings


# --- Impure repeated call must NOT false-unify (the numpy false-violation) ----


def test_impure_repeated_call_not_false_unified():
    # Real numpy shape (TestPrintOptions::test_basic): repr(x) is RECOMPUTED,
    # and an intervening call (set_printoptions) changes what it returns.  The
    # two repr(x) terms must be DISTINCT (location-keyed), so the conjunction is
    # NOT a spurious contradiction.  RED against structural-ctor unification.
    out = _lift(
        "def test_a():\n"
        "    x = make()\n"
        '    assert_equal(repr(x), "a")\n'
        "    ret = mutate()\n"
        '    assert_equal(repr(x), "b")\n'
    )
    inv = _only_decl(out)
    assert isinstance(inv, _Connective) and inv.kind == "and"
    lefts = [op.args[0] for op in inv.operands if isinstance(op, _Atomic) and op.name == "="]
    assert len(lefts) == 2
    assert lefts[0] != lefts[1], "repr(x) must not unify across statements (impure call)"


def test_same_bound_var_still_contradicts():
    # The teeth that MUST survive the fix: a bound variable is stable, so two
    # equalities about it with distinct constants are a genuine contradiction.
    out = _lift("def test_a():\n    x = make()\n    assert_equal(x, 1)\n    assert_equal(x, 2)\n")
    inv = _only_decl(out)
    assert isinstance(inv, _Connective) and inv.kind == "and"
    lefts = {op.args[0].name for op in inv.operands if isinstance(op, _Atomic) and isinstance(op.args[0], _Var)}
    assert lefts == {"x$0"}, lefts  # SAME var both times -> z3 sees contradiction


# --- Mutation / control-flow / side-effect -> LOUD REFUSE ---------------------


def test_subscript_mutation_refused():
    out = _lift("def test_a():\n    a = make()\n    a[0] = 5\n    assert_equal(a, 1)\n")
    assert out.decls == []
    assert any("mutation" in w.reason for w in out.warnings), out.warnings


def test_side_effect_call_refused():
    # a.sort() could mutate `a`; refuse rather than ignore.
    out = _lift("def test_a():\n    a = make()\n    a.sort()\n    assert_equal(a, 1)\n")
    assert out.decls == []
    assert any("side-effecting" in w.reason for w in out.warnings), out.warnings


def test_for_loop_refused():
    out = _lift(
        "def test_a():\n"
        "    assert_equal(x, 1)\n"
        "    for i in r:\n"
        "        assert_equal(i, 0)\n"
    )
    assert out.decls == []
    assert any("unsupported statement kind `For`" in w.reason for w in out.warnings), out.warnings


def test_assert_raises_recognized_but_refused():
    out = _lift("def test_a():\n    assert_raises(ValueError, f, 1)\n")
    assert out.decls == []
    assert "test_a" in out.claimed_tests  # recognized + claimed, not silent


# --- Seat boundary: bare-assert-only is NOT this lifter's test -----------------


def test_bare_assert_only_not_claimed_by_this_seat():
    out = _lift("def test_a():\n    assert x == 1\n")
    assert out.claimed_tests == set()
    assert out.decls == []
    assert out.warnings == []
