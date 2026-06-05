# Mixed-body REASSIGNMENT fixture: CONSISTENT (not false-refused).
#
# Pattern 6 fires because **kw blocks Pattern 5's call-result translation.
# SSA ensures that re-binding the same name gives a FRESH SSA generation.
#
#   r = factory_a(**kw)                     # → r$0 (free var, first gen)
#   assert r == 1                           # =(r$0, 1)
#   r = factory_b(**kw)                     # → r$1 (fresh free var, second gen)
#   assert r == 2                           # =(r$1, 2)
#
# Conjoined inv: and(=(r$0,1), =(r$1,2))
# DIFFERENT free vars (r$0 ≠ r$1 by SSA) → no link → SAT → PROVEN.
#
# CRITICAL guard: without SSA, both assertions would collapse to =(r,1)
# ∧ =(r,2) → UNSAT → FALSE REFUSAL.  This fixture ensures SSA is correct.
#
# CORRECT verdict: PROVEN-consistent.


def test_mixed_body_reassign_consistent():
    r = factory_a(**kw)
    assert r == 1
    r = factory_b(**kw)
    assert r == 2
