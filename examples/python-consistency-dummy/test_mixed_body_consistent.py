# Mixed-body POSITIVE fixture: CONSISTENT.
#
# Pattern 6 fires because **kw blocks Pattern 5's call-result translation.
# Two asserts about the SAME opaque-binding subject, with COMPATIBLE claims.
# SSA-keyed inv: and(=(r$0, 1), ≠(r$0, 2))
#
#   r = some_factory(**kw)                  # opaque binding → r$0 (free var)
#   assert r == 1                           # =(r$0, 1)
#   assert r != 2                           # ≠(r$0, 2)
#
# Satisfiable (r can be 1, which is ≠ 2) → z3 SAT → PROVEN-consistent.
#
# CORRECT verdict: PROVEN-consistent.


def test_mixed_body_consistent():
    r = some_factory(**kw)
    assert r == 1
    assert r != 2
