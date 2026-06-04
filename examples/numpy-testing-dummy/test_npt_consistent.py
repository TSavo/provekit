# Consistent numpy.testing assertions about a bound value -> PROVEN-consistent.
from numpy.testing import assert_equal, assert_array_equal


def test_consistent_value():
    result = compute()
    assert_equal(result, 3)
    assert_array_equal(result, 3)
