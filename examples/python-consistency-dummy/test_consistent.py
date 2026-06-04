# Receipt 1 fixture: CONSISTENT test assertions about the same bare var.
#
# Two bare assertions about the SAME parameter `x` (NO call binding, so this
# routes to Pattern 3 / characterization). The two facts are mutually
# satisfiable:
#
#   assert x is not None  -> ≠(x, None)
#   assert x == 3         -> =(x, 3)
#
# Coalesced inv: `and(≠(x, None), =(x, 3))` -> z3 SAT
# -> PROVEN-consistent.
#
# CLAIM: "test assertions mutually consistent about callsite X" -- NOT a
# code-correctness claim.


def test_x_consistent(x):
    assert x is not None
    assert x == 3
