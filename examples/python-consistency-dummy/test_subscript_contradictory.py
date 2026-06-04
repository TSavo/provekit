# Subscript-index lift fixture: CONTRADICTORY — same base, same key, distinct values.
#
# ``parsed['kind'] == 'contract'; parsed['kind'] == 'bridge'`` lifts to:
#   and(=(subscript(parsed, strlit_<hash('kind')>), strlit_<hash('contract')>),
#       =(subscript(parsed, strlit_<hash('kind')>), strlit_<hash('bridge')>))
#
# Both string literals are opaque Int constants (strlit_<hash>) made distinct
# from each other by the cross-type distinctness axiom.  Two equalities
# constraining the SAME Ctor term to two DISTINCT constants -> UNSAT.
#
# Discrimination guard for the subscript lift:
#   - consistent (different keys / different bases) -> PROVEN above
#   - contradictory (same base + key, distinct values) -> REFUSED here
# If this case is DISCHARGED (falsePass), the subscript is not being treated
# as a stable term sharing the same Ctor.
#
# CORRECT verdict: REFUSED-contradictory.


def test_subscript_same_key_contradictory(parsed):
    assert parsed['kind'] == 'contract'
    assert parsed['kind'] == 'bridge'
