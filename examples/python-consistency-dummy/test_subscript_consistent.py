# Subscript-index lift fixture: CONSISTENT assertions.
#
# Three distinct consistent cases:
#
#   (a) Different keys from the same base:
#       ``parsed['a'] == 1; parsed['b'] == 1`` — the two subscript Ctors have
#       different key args -> independent free-var-like terms -> satisfiable.
#
#   (b) Same key on different bases:
#       ``a['k'] == 1; b['k'] == 1`` — different base vars -> different Ctors
#       -> independent -> satisfiable.
#
#   (c) Same subscript, compatible values:
#       ``parsed['k'] == 1; parsed['k'] != 2`` — same Ctor, values 1 and 2
#       are distinct ints, so 1 satisfies both -> SAT -> PROVEN.
#
# CLAIM: test assertions mutually consistent. NOT code-correctness.
# Discrimination: verified by test_subscript_contradictory.py (same base + key,
# contradictory values -> REFUSED).


def test_subscript_different_keys_consistent(parsed):
    assert parsed['a'] == 1
    assert parsed['b'] == 1


def test_subscript_different_bases_consistent(a, b):
    assert a['k'] == 1
    assert b['k'] == 1


def test_subscript_same_key_compatible_consistent(parsed):
    assert parsed['k'] == 1
    assert parsed['k'] != 2
