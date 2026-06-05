# Truthiness-call lift fixture: CONTRADICTORY assertions (same call, true AND false).
#
# ``assert h.startswith(prefix)`` and ``assert not h.startswith(prefix)`` lift to:
#   ``call_startswith_a2(h, prefix) ∧ not(call_startswith_a2(h, prefix))``
# which is propositionally UNSAT (P ∧ ¬P) -> REFUSED-contradictory.
#
# Discrimination guard for the truthiness-call lift:
#   - consistent (different args) -> PROVEN (test_truthiness_consistent.py)
#   - contradictory (same call, both true and false) -> REFUSED here
# If the REFUSED case is DISCHARGED, ``assert f(x)`` and ``assert not f(x)``
# are not using the same predicate symbol and cannot unify (falsePass).


def test_truthiness_contradictory(h, prefix):
    assert h.startswith(prefix)
    assert not h.startswith(prefix)
