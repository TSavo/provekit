# ARGUMENT-CARRYING EUF receipt — REFUSED (CROSS-LOCATION CONTRADICTION).
#
# Two SEPARATE test functions, at DIFFERENT source lines, each assert a value
# about a bare call result ``make_value_xc(x)`` with the SAME parameter ``x``:
#
#   def test_euf_a(x):  assert make_value_xc(x) == 1
#   def test_euf_b(x):  assert make_value_xc(x) == 2
#
# Under ARGUMENT-CARRYING EUF the bare call result lifts to an
# argument-keyed ctor:
#
#   make_value_xc(x)  ->  ctor("callresult_make_value_xc_a1", [x])
#
# keyed on (callee, SSA-resolved arg-terms) rather than on source LOCATION.
# Because both calls have the SAME callee and the SAME arg term ``x``, they
# produce the SAME ctor term AND the SAME argument-keyed contract base
# (``make_value_xc#euf#...``), so mint's coalesce-by-name conjoins the two
# ``::assertion`` decls into ONE inv:
#
#   and( =(callresult_make_value_xc_a1(x), 1),
#        =(callresult_make_value_xc_a1(x), 2) )
#
# Same ctor term equated to two distinct Int constants -> z3 UNSAT -> REFUSED.
#
# This is the cross-location same-input contradiction that the OLD
# location-keyed lifter could NOT see: with ``make_value_xc$call$<line>$<col>``
# the two calls produced DIFFERENT free vars (different lines) and z3 saw no
# contradiction (the argument-blind gap). EUF closes it.
#
# PURITY ASSUMPTION (loud, same as the Form-3 callval tradeoff): same callee +
# same args -> same value assumes ``make_value_xc`` is DETERMINISTIC / pure. A
# stateful callee called twice with the same args could return different values;
# unifying them here can only ever CONSERVATIVELY OVER-REFUSE (never falsePass).
#
# CORRECT verdict: REFUSED-contradictory.


def test_euf_a(x):
    assert make_value_xc(x) == 1


def test_euf_b(x):
    assert make_value_xc(x) == 2
