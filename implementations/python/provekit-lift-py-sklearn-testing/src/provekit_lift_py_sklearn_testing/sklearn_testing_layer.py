# SPDX-License-Identifier: Apache-2.0
"""scikit-learn ``sklearn.utils._testing`` assertion-vocabulary lift surface.

The vocabulary lifts ENTIRELY from sklearn.utils._testing's own source, with ZERO
overrides: sklearn's assertions are numpy-style, so ``derive_vocab`` classifies
every ``assert_*`` correctly from structure alone (a tolerance parameter marks
APPROX; an ``operator.__eq__`` delegation marks EQUALITY; everything else is
claimed + refused as OTHER). This is the cleanest seat -- the table is pure lift
output, not hand-authored.
"""

from __future__ import annotations

from provekit_lift_py_tests.assertion_layer import lift_file_assertions
from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab

SKLEARN_TESTING = derive_vocab("sklearn.utils._testing", "sklearn-testing")


def lift_file_sklearn_testing(source: str, source_path: str):
    """Lift sklearn.utils._testing assertions from a test file."""
    return lift_file_assertions(source, source_path, SKLEARN_TESTING)
