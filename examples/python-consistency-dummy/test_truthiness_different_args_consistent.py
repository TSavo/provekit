# Truthiness-call lift fixture: arg-carrying discrimination guard.
#
# ``assert h.startswith(p)`` and ``assert not h.startswith(q)`` lift to:
#   ``call_startswith_a2(h, p) ∧ not(call_startswith_a2(h, q))``
# These are INDEPENDENT predicate atoms (different arg[1] terms, p ≠ q).
# An uninterpreted predicate CAN be true for one arg and false for another
# (no function-consistency constraint), so the conjunction is satisfiable -> PROVEN.
#
# This is the THIRD discrimination receipt that distinguishes arg-carrying from
# arg-dropping.  An arg-dropping implementation would lift both to the SAME
# 0-ary predicate ``P ∧ ¬P`` -> UNSAT, falsely REFUSING a consistent test.
# This fixture catches that regression.
#
# Receipt map:
#   test_truthiness_consistent.py          P(h,p) ∧ P(h,q)    -> sat (PROVEN)
#   test_truthiness_contradictory.py       P(h,p) ∧ ¬P(h,p)   -> unsat (REFUSED)
#   test_truthiness_different_args_consistent.py   P(h,p) ∧ ¬P(h,q) -> sat (PROVEN)


def test_truthiness_different_args_consistent(h, p, q):
    assert h.startswith(p)
    assert not h.startswith(q)
