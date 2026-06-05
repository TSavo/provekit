# Regression fixture: CONSISTENT call-binding test.
# y = make_value(x) is a call result; the two asserts about y are consistent.
# CORRECT verdict: PROVEN-consistent (discharged). This fixture ensures the
# call-binding conjoin fix does NOT swallow legitimate consistent binding tests.
#
#   assert y is not None  -> ≠(y, None)
#   assert y == 3         -> =(y, 3)
#
# Coalesced inv: and(≠(y,None), =(y,3)) -> z3 SAT -> PROVEN-consistent.
def test_y_consistent_binding(x):
    y = make_value(x)
    assert y is not None
    assert y == 3
