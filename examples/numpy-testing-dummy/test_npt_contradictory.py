# Contradictory numpy.testing assertions about the SAME bound value ->
# REFUSED (teeth): assert_equal(r,1) and assert_equal(r,2) cannot both hold.
from numpy.testing import assert_equal


def test_contradictory_value():
    r = compute()
    assert_equal(r, 1)
    assert_equal(r, 2)
