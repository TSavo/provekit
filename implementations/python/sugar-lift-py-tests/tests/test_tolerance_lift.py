# SPDX-License-Identifier: Apache-2.0
"""The approximate-tolerance lift: an ``approx`` assertion with a ToleranceSpec is
lifted as the real-arithmetic two-sided bound ``|a-b| < 1.5 * 10**(-decimal)``
instead of being loud-refused. Discrimination tests, one per behaviour.
"""
import json

from sugar_lift_py_tests.assertion_layer import (
    AssertionVocab,
    ToleranceSpec,
    _decimal_tol_strings,
    lift_file_assertions,
)
from sugar_lift_py_tests.ir import formula_to_value

# Synthetic vocab: one decimal-bounded approx (`close`) and one unbounded approx
# (`ulp`, standing in for the ULP family that has no algebraic bound).
VOCAB = AssertionVocab(
    label="fake-testing",
    approx=frozenset({"close", "ulp"}),
    tolerances=(ToleranceSpec("close", decimal_default=7),),
)


def _lift(src):
    return lift_file_assertions(f"def test_it():\n    {src}\n", "t.py", VOCAB)


def _bound_strings(out):
    assert len(out.decls) == 1, [w.reason for w in out.warnings]
    return json.dumps(formula_to_value(out.decls[0].inv), default=str)


def _formula(out):
    assert len(out.decls) == 1, [w.reason for w in out.warnings]
    return out.decls[0].inv


def test_decimal_bound_strings_are_exact_and_float_free():
    # exact decimals, content-addressable verbatim -- never a float repr
    assert _decimal_tol_strings(7) == ("0.00000015", "-0.00000015")
    assert _decimal_tol_strings(6) == ("0.0000015", "-0.0000015")
    assert _decimal_tol_strings(0) == ("1.5", "-1.5")


def test_positive_decimal_approx_lifts_to_two_sided_real_bound():
    f = _formula(_lift("close(a, b)"))  # default decimal=7
    # a conjunction of two strict comparisons (the two-sided bound)
    assert f.kind == "and"
    names = sorted(atom.name for atom in f.operands)
    assert names == ["<", ">"], names
    # each compares the difference (a-b) against a Real-sorted decimal bound
    for atom in f.operands:
        diff, bound = atom.args
        assert diff.name == "-" and [v.name for v in diff.args] == ["a", "b"]
        assert bound.sort.name == "Real"
        assert bound.value in ("0.00000015", "-0.00000015")


def test_decimal_kwarg_sets_the_bound():
    blob = _bound_strings(_lift("close(a, b, decimal=3)"))
    assert "0.0015" in blob and "-0.0015" in blob


def test_discrimination_unbounded_approx_is_still_refused():
    # `ulp` is approx but has NO ToleranceSpec -> loud refuse, zero contracts.
    out = _lift("ulp(a, b)")
    assert len(out.decls) == 0
    assert any("refused to avoid false-pass" in w.reason for w in out.warnings)


def test_soundness_non_literal_decimal_refuses():
    # tolerance not computable at lift time -> refuse, never guess a bound.
    out = _lift("close(a, b, decimal=n)")
    assert len(out.decls) == 0
    assert any("not computable" in w.reason for w in out.warnings)
