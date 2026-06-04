# ARGUMENT-CARRYING EUF receipt — PROVEN (DIFFERENT-ARGS GUARD).
#
# Two SEPARATE test functions assert about a bare call result ``make_value``
# with DIFFERENT arguments (``x`` vs ``y``):
#
#   def test_euf_diff_a(x, y):  assert make_value_da(x) == 1
#   def test_euf_diff_b(x, y):  assert make_value_da(y) == 2
#
# Under ARGUMENT-CARRYING EUF these lift to DIFFERENT ctor terms with
# DIFFERENT argument-keyed bases:
#
#   make_value_da(x) -> ctor("callresult_make_value_da_a1", [x])  base: make_value_da#euf#...(v:x)
#   make_value_da(y) -> ctor("callresult_make_value_da_a1", [y])  base: make_value_da#euf#...(v:y)
#
# Different arg terms (x != y) -> different ctor -> different base -> mint does
# NOT coalesce them -> two independent ::assertion invs:
#
#   =(callresult_make_value_da_a1(x), 1)   -> SAT
#   =(callresult_make_value_da_a1(y), 2)   -> SAT
#
# Neither inv is contradictory -> both PROVEN.
#
# This is the DISCRIMINATION GUARD for the EUF lift: a contradiction must fire
# ONLY when the arguments match. If this case came back REFUSED, the lifter
# would be argument-BLIND in the other direction (over-unifying distinct calls)
# = a false-refusal regression. It must stay PROVEN.
#
# CORRECT verdict: PROVEN-consistent (both functions).


def test_euf_diff_a(x, y):
    assert make_value_da(x) == 1


def test_euf_diff_b(x, y):
    assert make_value_da(y) == 2
