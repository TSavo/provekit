# Fixture: Layer 2 lift patterns for Python (pytest + unittest).
# Used by the integration test in tests/test_integration.py. NOT executed
# as test code; the lift adapter parses it as text via the ``ast`` module.
#
# Layer 2 catches structural patterns Layer 0 cannot:
#   - Pattern 1: bounded loop as universal quantifier (range OR literal list)
#   - Pattern 2: inlined helper functions
#   - Pattern 3: multi-assertion characterization conjunction
#   - Pattern 4: @pytest.mark.parametrize over a literal arg list
#
# One test deliberately exercises a SKIP path (nested loop) so the
# integration test can assert the structured warning surface.

import pytest
import unittest


# ---- Pattern 1: bounded loops -------------------------------------------

def test_squares_are_nonneg():
    for x in range(0, 100):
        assert x >= 0


def test_divmod_in_range():
    for x in range(1, 50):
        assert x > 0


def test_small_window():
    for x in range(16):
        assert x < 100


def test_literal_list_iter():
    for v in [1, 2, 3]:
        assert v >= 0


# ---- Pattern 2: helper inlining ----------------------------------------

def assert_is_42(x: int):
    assert x == 42


def assert_in_range(y: int):
    assert y >= 0


def test_many_42s():
    assert_is_42(42)
    assert_is_42(42)
    assert_is_42(42)


def test_ranges_ok():
    assert_in_range(0)
    assert_in_range(1)


# ---- Pattern 3: multi-assertion characterization -----------------------

def test_parse_int_characterization():
    assert parse_int("0") == 0
    assert parse_int("42") == 42
    assert parse_int("99") != 0


class TestUnittestStyle(unittest.TestCase):
    def test_three_facts(self):
        self.assertEqual(f(1), 1)
        self.assertEqual(f(2), 2)
        self.assertNotEqual(f(3), 0)


# ---- Pattern 4: parametrize -------------------------------------------

@pytest.mark.parametrize("v", [1, 2, 3, 4])
def test_v_nonneg(v):
    assert v >= 0


# ---- Pattern 1 deliberately-skipped: nested loop (Layer 2.5) ----------

def test_nested_loop_skipped():
    for x in range(10):
        for y in range(10):
            assert x >= 0
