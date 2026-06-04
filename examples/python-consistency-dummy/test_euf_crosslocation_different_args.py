# ARGUMENT-CARRYING EUF receipt — PROVEN (DIFFERENT CONCRETE ARGS GUARD).
#
# FIX 1 UPDATE: EUF cross-location unification is ONLY sound for CONCRETE
# LITERAL arguments.  Two SEPARATE test functions assert about a bare call
# result ``make_value_da`` with DIFFERENT CONCRETE LITERAL arguments (3 vs 7):
#
#   def test_euf_diff_a():  assert make_value_da(3) == 1
#   def test_euf_diff_b():  assert make_value_da(7) == 2
#
# Under ARGUMENT-CARRYING EUF these lift to DIFFERENT ctor terms with
# DIFFERENT argument-keyed bases:
#
#   make_value_da(3) -> ctor("callresult_make_value_da_a1", [num(3)])  base: make_value_da#euf#...(i:3)
#   make_value_da(7) -> ctor("callresult_make_value_da_a1", [num(7)])  base: make_value_da#euf#...(i:7)
#
# Different concrete arg values (3 != 7) -> different ctor -> different base ->
# mint does NOT coalesce them -> two independent ::assertion invs:
#
#   =(callresult_make_value_da_a1(3), 1)   -> SAT
#   =(callresult_make_value_da_a1(7), 2)   -> SAT
#
# Neither inv is contradictory -> both PROVEN.
#
# This is the DISCRIMINATION GUARD for the EUF lift: a contradiction must fire
# ONLY when the concrete argument values match.  If this case came back REFUSED,
# the lifter would be over-unifying distinct concrete inputs = a false-refusal
# regression.  It must stay PROVEN.
#
# NOTE: The previous version used symbolic params (x vs y).  After Fix 1,
# symbolic args fall back to location-keyed free vars — which is also PROVEN
# (independent per-location vars, no contradiction).  Using concrete different
# literals here tests the correct EUF discrimination path (arg_sig differs).
#
# CORRECT verdict: PROVEN-consistent (both functions).


def test_euf_diff_a():
    assert make_value_da(3) == 1


def test_euf_diff_b():
    assert make_value_da(7) == 2
