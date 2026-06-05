# The DEGENERATE case: inconsistent contracts on a numpy operation.
#
# `numpy.add` is under contract here via the numpy sugar `.proof` in
# .provekit/imports/. This test binds ONE `np.add(2, 3)` result and asserts two
# disagreeing values about it. The pytest lifter conjoins the two assertions
# (same SSA-bound result) into one inv:
#
#   and( =(r, 5), =(r, 6) )
#
# z3 reports that UNSAT -> REFUSED-contradictory. numpy.add(2, 3) cannot be both
# 5 and 6; the framework catches the inconsistent contracts loudly. The
# contradiction is the point.

import numpy as np


def test_add_contradictory():
    r = np.add(2, 3)
    assert r == 5
    assert r == 6
