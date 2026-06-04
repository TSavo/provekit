# pytest.approx discrimination receipt — REFUSED (CONTRADICTORY).
#
# SAME x, SAME target (1.5), CONTRADICTORY directions:
#   assert x == pytest.approx(1.5)  -> approx_eq(x, strlit_1_5)
#   assert x != pytest.approx(1.5)  -> not_(approx_eq(x, strlit_1_5))
#
# Conjoined inv: approx_eq(x,t) ^ not_(approx_eq(x,t)) -> UNSAT -> REFUSED.
#
# This is the "obvious case" for approx: a test that asserts the same value
# is BOTH within tolerance AND outside tolerance of the same target. The
# framework solves it UNSAT and refuses it loudly.
#
# SOUND MODEL: conservative uninterpreted predicate "approx_eq".
# z3 sees P ^ not-P which is propositionally UNSAT without any arithmetic.
# NOTE: this does NOT model the tolerance interval — it only catches the
# P ^ not-P shape. Different targets are treated as independent (under-refusal).


def test_approx_contradiction(x):
    assert x == pytest.approx(1.5)
    assert x != pytest.approx(1.5)
