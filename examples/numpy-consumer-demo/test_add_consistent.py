# The consistent companion: a numpy user whose contracts agree.
#
# Same `numpy.add` under contract via the .proof, but the two assertions about
# the SSA-bound `np.add(2, 3)` result agree (both 5). The conjoined inv
#
#   and( =(r, 5), =(r, 5) )
#
# is SAT -> PROVEN-consistent. This is the non-degenerate baseline: the spec is
# internally consistent, so the framework discharges it.

import numpy as np


def test_add_consistent():
    r = np.add(2, 3)
    assert r == 5
    assert r == 5
