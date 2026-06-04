# Attribute-access lift fixture: CONSISTENT assertions about distinct dotted paths.
#
# Two equality assertions about DIFFERENT attribute paths (``a.x`` vs ``b.x``)
# lift to independent free vars in Pattern 3.  Each var can equal 1 independently
# -> mutually satisfiable -> PROVEN-consistent.
#
# CLAIM: "test assertions mutually consistent about callsite" -- NOT
# code-correctness.  Dotted path = opaque Var; same path = same Var.


def test_attr_different_paths_consistent(a, b):
    assert a.x == 1
    assert b.x == 1
