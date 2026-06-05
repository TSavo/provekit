# pytest.approx discrimination receipt — PROVEN (CONSISTENT).
#
# SAME x, SAME target (1.5), SAME direction (==):
#   assert x == pytest.approx(1.5)  -> approx_eq(x, strlit_1_5)
#   assert x == pytest.approx(1.5)  -> approx_eq(x, strlit_1_5)
#
# Conjoined inv: approx_eq(x,t) ^ approx_eq(x,t) -> SAT -> PROVEN.
#
# CLAIM: test assertions mutually consistent (same approx_eq predicate twice).
#
# SOUND MODEL: conservative uninterpreted predicate "approx_eq".
# z3 sees P ^ P which is trivially SAT.


def test_approx_consistent(x):
    assert x == pytest.approx(1.5)
    assert x == pytest.approx(1.5)
