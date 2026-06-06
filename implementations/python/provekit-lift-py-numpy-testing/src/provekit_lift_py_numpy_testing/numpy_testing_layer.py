# SPDX-License-Identifier: Apache-2.0
"""NumPy ``numpy.testing`` assertion-vocabulary lift surface.

The vocabulary is NOT hand-authored: it is LIFTED from numpy.testing's own source
by ``derive_vocab``, which reads each ``assert_*`` function and classifies it by
structure -- a tolerance parameter in the signature (rtol/atol/decimal/...) ->
APPROX (the soundness-critical exact/approximate split); an ``operator.__eq__``
delegation in the body -> EQUALITY (``=``); everything else -> OTHER (claim +
refuse). The ``other`` set is therefore the TRUE set for the installed numpy, not
a curated list that drifts.

Only the structurally-opaque names are a hand ``override``: ``assert_equal`` /
``assert_equals`` (recursive dispatch, no single delegated operator) and
``assert_`` (truthiness). The decimal-bounded approx members carry ToleranceSpecs
so they lift as the real two-sided bound ``|a-b| < 1.5*10**(-decimal)`` (see
``assertion_layer``) instead of being refused.
"""

from __future__ import annotations

from provekit_lift_py_tests.assertion_layer import ToleranceSpec, lift_file_assertions
from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab

NUMPY_TESTING = derive_vocab(
    "numpy.testing",
    "numpy-testing",
    overrides={
        "equality": frozenset({"assert_equal", "assert_equals"}),
        "truth": frozenset({"assert_"}),
    },
    # numpy's defaults: assert_almost_equal -> decimal=7,
    # assert_array_almost_equal -> decimal=6 (both: 3rd positional is `decimal`).
    tolerances=(
        ToleranceSpec("assert_almost_equal", decimal_default=7),
        ToleranceSpec("assert_array_almost_equal", decimal_default=6),
    ),
)


def lift_file_numpy_testing(source: str, source_path: str):
    """Lift numpy.testing assertions from a test file."""
    return lift_file_assertions(source, source_path, NUMPY_TESTING)
