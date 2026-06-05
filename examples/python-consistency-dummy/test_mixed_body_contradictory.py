# Mixed-body DISCRIMINATION fixture: CONTRADICTORY.
#
# Pattern 6 (mixed-body) lifts a test with opaque bindings + multiple asserts.
# The RHS uses **kw so Pattern 5 (which requires a translatable call-result)
# does not claim this test. Pattern 6 fires instead.
#
#   r = some_factory(**kw)                  # opaque binding → r$0 (free var)
#   assert r == 1                           # =(r$0, 1)
#   assert r == 2                           # =(r$0, 2)
#
# Conjoined inv: and(=(r$0,1), =(r$0,2))
# Same free var, two distinct Int constants → z3 UNSAT → REFUSED.
#
# CORRECT verdict: REFUSED-contradictory.
# This is the "teeth" shape: a test that contains a self-defeating claim.


def test_mixed_body_contradictory():
    r = some_factory(**kw)
    assert r == 1
    assert r == 2
