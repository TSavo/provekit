# SPDX-License-Identifier: Apache-2.0
"""NumPy ``numpy.testing`` assertion-vocabulary lift surface.

This is now the generic assertion fold (``provekit_lift_py_tests.assertion_layer``)
plus a vocabulary table. The fold is shared with the pandas and sklearn seats; the
ONLY thing that varies per library is this table.

SOUNDNESS -- the EXACT/APPROXIMATE split:
  EXACT (lifted as ``=``):  assert_equal / assert_array_equal / assert_equals
  TRUTH (bool-expr lift):   assert_
  APPROXIMATE (LOUD REFUSE -- ``a ~= b`` within tolerance is NOT ``a = b``):
    assert_allclose, assert_almost_equal, assert_array_almost_equal,
    assert_approx_equal, assert_array_almost_equal_nulp, assert_array_max_ulp
  OTHER (claim + refuse, so nothing is silently skipped):
    assert_raises, assert_warns, assert_string_equal, assert_array_less, ...
"""

from __future__ import annotations

from provekit_lift_py_tests.assertion_layer import AssertionVocab, lift_file_assertions

NUMPY_TESTING = AssertionVocab(
    label="numpy-testing",
    equality=frozenset({"assert_equal", "assert_array_equal", "assert_equals"}),
    truth=frozenset({"assert_"}),
    approx=frozenset({
        "assert_allclose",
        "assert_almost_equal",
        "assert_array_almost_equal",
        "assert_approx_equal",
        "assert_array_almost_equal_nulp",
        "assert_array_max_ulp",
    }),
    other=frozenset({
        "assert_raises",
        "assert_raises_regex",
        "assert_warns",
        "assert_no_warnings",
        "assert_string_equal",
        "assert_array_less",
        "assert_warns_message",
        "assert_array_equal_nan",
    }),
)


def lift_file_numpy_testing(source: str, source_path: str):
    """Lift numpy.testing assertions from a test file."""
    return lift_file_assertions(source, source_path, NUMPY_TESTING)
