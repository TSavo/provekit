# Truthiness-call lift fixture: CONSISTENT assertions (different args, same callee).
#
# ``assert h.startswith(p)`` and ``assert h.startswith(q)`` lift to:
#   ``call_startswith_a2(h, p) ∧ call_startswith_a2(h, q)``
# These are INDEPENDENT predicate atoms over different argument terms.
# A propositional formula ``P(a) ∧ P(b)`` with distinct a,b is satisfiable
# (the uninterpreted predicate can be true for both) -> PROVEN-consistent.
#
# CLAIM: "test assertions mutually consistent" -- NOT that startswith is correct.
# ``call_startswith_a2`` is an uninterpreted Bool function; no string semantics.


def test_truthiness_consistent(h, p, q):
    assert h.startswith(p)
    assert h.startswith(q)
