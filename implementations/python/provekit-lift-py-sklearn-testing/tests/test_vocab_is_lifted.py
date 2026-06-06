# SPDX-License-Identifier: Apache-2.0
"""The sklearn.utils._testing vocabulary IS lift output, with zero overrides:
``SKLEARN_TESTING`` is produced by ``derive_vocab`` reading sklearn's own source.
"""
import pytest

pytest.importorskip("sklearn")
from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab
from provekit_lift_py_sklearn_testing.sklearn_testing_layer import SKLEARN_TESTING


def test_seat_vocab_is_pure_derive_output_zero_overrides():
    # sklearn lifts ENTIRELY from source -- no overrides, no tolerances tweaks.
    assert SKLEARN_TESTING == derive_vocab("sklearn.utils._testing", "sklearn-testing")


def test_soundness_critical_split_from_source():
    # the approximate family the seat refuses, derived from the tolerance params
    for name in ("assert_allclose", "assert_array_almost_equal", "assert_almost_equal"):
        assert name in SKLEARN_TESTING.approx, name
    # exact equality recovered from operator.__eq__ delegation
    assert "assert_array_equal" in SKLEARN_TESTING.equality
    # nothing approximate leaked into equality
    assert not (SKLEARN_TESTING.approx & SKLEARN_TESTING.equality)
