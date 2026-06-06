# SPDX-License-Identifier: Apache-2.0
"""scikit-learn ``sklearn.utils._testing`` assertion-vocabulary lift surface.

This is now the generic assertion fold (``provekit_lift_py_tests.assertion_layer``)
plus a vocabulary table -- the same fold the numpy and pandas seats use.

sklearn is the most APPROXIMATE-dominated of the three: of its assertion
vocabulary, ONLY ``assert_array_equal`` is exact (-> ``=``); the whole allclose
family is tolerance-based (-> LOUD REFUSE), and the order/docstring/script shapes
are claimed + refused. The witness axis carries the weight for sklearn.
"""

from __future__ import annotations

from provekit_lift_py_tests.assertion_layer import AssertionVocab, lift_file_assertions

SKLEARN_TESTING = AssertionVocab(
    label="sklearn-testing",
    equality=frozenset({"assert_array_equal"}),
    approx=frozenset({
        "assert_allclose",
        "assert_array_almost_equal",
        "assert_almost_equal",
        "assert_allclose_dense_sparse",
    }),
    other=frozenset({
        "assert_array_less",
        "assert_docstring_consistency",
        "assert_run_python_script_without_output",
    }),
)


def lift_file_sklearn_testing(source: str, source_path: str):
    """Lift sklearn.utils._testing assertions from a test file."""
    return lift_file_assertions(source, source_path, SKLEARN_TESTING)
