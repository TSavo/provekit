# Receipt 1 fixture: CONTRADICTORY test assertions about the same bare var.
#
# Two bare assertions about the SAME parameter `x` (NO call binding, so this
# routes to Pattern 3 / characterization, which preserves BOTH assertions as
# a conjunction rather than the Pattern 5 callsite null-guard shape):
#
#   assert x is None      -> =(x, None)
#   assert x is not None  -> ≠(x, None)
#
# Coalesced inv: `and(=(x, None), ≠(x, None))` -> z3 UNSAT
# -> REFUSED-contradictory.
#
# This is T's obvious case: write a contradictory unit test, the framework
# solves it UNSAT and refuses it loudly. The contradiction is the point.


def test_x_contradictory(x):
    assert x is None
    assert x is not None
