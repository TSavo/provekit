# Membership lift fixture: CONTRADICTORY assertions (in AND not-in same terms).
#
# ``x in coll`` AND ``x not in coll`` lift to:
#   ``member(x, coll) ∧ not(member(x, coll))``
# which is propositionally UNSAT (P ∧ ¬P) -> REFUSED-contradictory.
#
# Discrimination guard for the membership-lift:
#   - consistent (same in) -> PROVEN above
#   - contradictory (in AND not-in) -> REFUSED here
# If the REFUSED case is DISCHARGED, ``in`` and ``not in`` are not using the
# same predicate symbol and the contradiction cannot unify (falsePass).


def test_membership_contradictory(x, coll):
    assert x in coll
    assert x not in coll
