# ARGUMENT-CARRYING EUF receipt — REFUSED (CROSS-LOCATION CONTRADICTION).
#
# FIX 1 UPDATE: EUF cross-location unification is ONLY sound for CONCRETE
# LITERAL arguments.  Two SEPARATE test functions, at DIFFERENT source lines,
# each assert a value about a bare call result ``make_value_xc(5)`` with the
# SAME CONCRETE argument ``5``:
#
#   def test_euf_a():  assert make_value_xc(5) == 1
#   def test_euf_b():  assert make_value_xc(5) == 2
#
# Concrete literal 5 is the same value regardless of call-site location, so
# cross-location unification is SOUND.  Under ARGUMENT-CARRYING EUF the bare
# call result lifts to an argument-keyed ctor:
#
#   make_value_xc(5)  ->  ctor("callresult_make_value_xc_a1", [num(5)])
#
# keyed on (callee, CONCRETE-arg-terms) rather than on source LOCATION.
# Because both calls have the SAME callee and the SAME concrete arg term 5,
# they produce the SAME ctor term AND the SAME argument-keyed contract base
# (``make_value_xc#euf#...``), so mint's coalesce-by-name conjoins the two
# ``::assertion`` decls into ONE inv:
#
#   and( =(callresult_make_value_xc_a1(5), 1),
#        =(callresult_make_value_xc_a1(5), 2) )
#
# Same ctor term equated to two distinct Int constants -> z3 UNSAT -> REFUSED.
#
# NOTE: The previous version used a function parameter ``x`` (symbolic arg).
# After Fix 1, symbolic args fall back to location-keyed free vars (no EUF
# unification) because the params in the two functions may hold DIFFERENT
# runtime values — unifying them would cause a false-refusal.  To preserve
# the cross-location REFUSED receipt, we use the concrete literal 5 here.
#
# PURITY ASSUMPTION (loud, same as the Form-3 callval tradeoff): same callee +
# same args -> same value assumes ``make_value_xc`` is DETERMINISTIC / pure. A
# stateful callee called twice with the same args could return different values;
# unifying them here can only ever CONSERVATIVELY OVER-REFUSE (never falsePass).
#
# CORRECT verdict: REFUSED-contradictory.


def test_euf_a():
    assert make_value_xc(5) == 1


def test_euf_b():
    assert make_value_xc(5) == 2
