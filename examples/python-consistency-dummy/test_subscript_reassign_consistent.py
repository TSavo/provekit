# Subscript-index lift fixture: SSA reassignment — NOT a false refusal.
#
# Pattern 6 (mixed-body): the base var ``parsed`` is reassigned between the
# two asserts.  SSA bumps the version, so the two subscript Ctors have
# DIFFERENT base terms:
#
#   parsed = f()            # parsed -> parsed$0
#   assert parsed['k'] == 1 # subscript(parsed$0, strlit_k) == 1
#   parsed = g()            # parsed -> parsed$1
#   assert parsed['k'] == 2 # subscript(parsed$1, strlit_k) == 2
#
# The two Ctors are DISTINCT (different base SSA vars) -> independent -> SAT.
# Without SSA they would share the same Ctor term -> UNSAT (false refusal).
#
# CORRECT verdict: PROVEN-consistent (not false-refused).


def test_subscript_reassign_base_consistent():
    parsed = f()
    assert parsed['k'] == 1
    parsed = g()
    assert parsed['k'] == 2
