# SPDX-License-Identifier: Apache-2.0
"""The sklearn.utils._testing vocabulary is LIFT OUTPUT, not a hand-authored table.

sklearn's assertion vocabulary is numpy-style (it re-exports / mirrors
numpy.testing), so the structural signals carry: tolerance parameters mark the
approximate family, ``operator.__eq__`` delegation marks exact equality. The
soundness-critical split derives from the library's own source.
"""
import pytest

pytest.importorskip("sklearn")
from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab
from provekit_lift_py_sklearn_testing.sklearn_testing_layer import SKLEARN_TESTING as HAND


def test_approx_split_lifts_itself_from_source():
    derived = derive_vocab("sklearn.utils._testing", "sklearn-testing")
    # the approximate family the hand table refuses is exactly what the tolerance
    # parameters mark -- the soundness split, derived from source.
    assert HAND.approx <= derived.approx, (
        f"hand approx {sorted(HAND.approx)} not all derived {sorted(derived.approx)}"
    )
    assert "assert_array_equal" in derived.equality
