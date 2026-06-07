# numpy's own testing vocabulary mints the contract on numpy.rot90 — lifted by
# the numpy.testing seat, no synthetic unit test needed. assert_equal is numpy's
# scalar-equality assertion; the rotated result is bound to `r` and its elements
# are asserted, so each fact lifts to a value-scope consistency obligation AND is
# re-run for the witness.
import numpy as np
from numpy.testing import assert_equal


def test_rot90_quarter_turn():
    # rot90 turns [[1, 2], [3, 4]] a quarter-turn counter-clockwise into
    # [[2, 4], [1, 3]]. The four element facts are mutually consistent.
    r = np.rot90([[1, 2], [3, 4]])
    assert_equal(r[0][0], 2)
    assert_equal(r[0][1], 4)
    assert_equal(r[1][0], 1)
    assert_equal(r[1][1], 3)


def test_rot90_contradiction():
    # The degenerate case: the SAME element asserted equal to two different
    # values. The spec contradicts itself -> z3 UNSAT -> refused; and when run,
    # `r[0][0] == 9` is False -> the test fails -> witness 'failed' -> refused.
    r = np.rot90([[1, 2], [3, 4]])
    assert_equal(r[0][0], 2)
    assert_equal(r[0][0], 9)
