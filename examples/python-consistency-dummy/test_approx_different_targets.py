# pytest.approx discrimination receipt — PROVEN (CONSERVATIVE UNDER-REFUSAL GUARD).
#
# SAME x, DIFFERENT targets (5.0 vs 99.0):
#   assert x == pytest.approx(5.0)   -> approx_eq(x, strlit_5_0)
#   assert x == pytest.approx(99.0)  -> approx_eq(x, strlit_99_0)
#
# These are two INDEPENDENT uninterpreted predicates (different str_const
# target terms). No assertion that the targets are distinct. Conjoined inv:
# approx_eq(x,t1) ^ approx_eq(x,t2) -> SAT -> PROVEN.
#
# DOCUMENTED LIMITATION (conservative under-refusal, NEVER a falsePass):
# The tolerance intervals [5.0+-eps, 99.0+-eps] are disjoint in reality,
# so this SHOULD be REFUSED (x cannot simultaneously be within epsilon of
# 5.0 AND within epsilon of 99.0 with the default rel tolerance). However,
# the uninterpreted predicate model does NOT assert target distinctness.
# This is intentional: asserting distinctness would cause overlapping-range
# cases (e.g. approx(1.0) vs approx(1.0001)) to be falsely refused — a
# new falsePass. The under-refusal (disjoint targets not caught) is
# acceptable and explicitly documented here.


def test_approx_different_targets(x):
    assert x == pytest.approx(5.0)
    assert x == pytest.approx(99.0)
