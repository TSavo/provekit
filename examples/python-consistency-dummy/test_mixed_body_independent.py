# Mixed-body STRUCTURAL fixture: INDEPENDENT SUBJECTS.
#
# Pattern 6 fires because **kw blocks Pattern 5's call-result translation.
# Asserts about DIFFERENT SSA-keyed subjects must NOT cross-contaminate.
#
#   a = factory_a(**kw)                     # opaque → a$0 (free var)
#   b = factory_b(**kw)                     # opaque → b$0 (independent free var)
#   assert a == 1                           # =(a$0, 1)
#   assert b == 2                           # =(b$0, 2)
#
# Conjoined inv: and(=(a$0,1), =(b$0,2))
# Two DIFFERENT free vars → no constraint links them → z3 SAT → PROVEN.
#
# CORRECT verdict: PROVEN-consistent (NOT refused, NOT spurious contradiction).
# This guards against cross-contamination of independent subjects.


def test_mixed_body_independent():
    a = factory_a(**kw)
    b = factory_b(**kw)
    assert a == 1
    assert b == 2
