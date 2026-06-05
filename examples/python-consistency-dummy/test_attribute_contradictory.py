# Attribute-access lift fixture: CONTRADICTORY assertions about the SAME dotted path.
#
# Two equality assertions about the SAME attribute path (``out.val``) with distinct
# integer literals lift to the conjunction ``=(out.val, 1) ∧ =(out.val, 2)``.
# Same Var, two distinct Int constants -> z3 UNSAT -> REFUSED-contradictory.
#
# Discrimination guard for the attribute-lift:
#   - consistent (two different paths) -> PROVEN above
#   - contradictory (same path, distinct values) -> REFUSED here
# If the REFUSED case is DISCHARGED, the attribute path is not being treated as
# a single stable Var (falsePass).


def test_attr_same_path_contradictory(out):
    assert out.val == 1
    assert out.val == 2
