# Membership lift fixture: CONSISTENT assertions (same membership, no contradiction).
#
# Two ``x in coll`` assertions lift to ``member(x, coll) ∧ member(x, coll)``.
# A propositional formula ``P ∧ P`` is satisfiable -> PROVEN-consistent.
#
# CLAIM: "test assertions mutually consistent about callsite" -- NOT set-membership
# correctness.  ``member`` is an uninterpreted Bool function; no set theory needed.


def test_membership_consistent(x, coll):
    assert x in coll
    assert x in coll
