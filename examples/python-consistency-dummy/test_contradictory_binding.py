# PROBE: contradictory test with a CALL BINDING (Pattern 5 path).
# y = make_value(x) is a call result; the two asserts about y are contradictory.
# CORRECT verdict: REFUSED (unsat). If this comes back DISCHARGED, the Pattern-5
# path dropped a polarity = FALSEPASS = the cardinal sin.
def test_y_contradictory_binding(x):
    y = make_value(x)
    assert y is None
    assert y is not None
