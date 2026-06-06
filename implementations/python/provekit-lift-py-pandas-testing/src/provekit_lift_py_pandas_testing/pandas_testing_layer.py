# SPDX-License-Identifier: Apache-2.0
"""pandas ``pandas.testing`` assertion-vocabulary lift surface.

The vocabulary goes through ``derive_vocab`` like the numpy and sklearn seats, but
pandas is the STRUCTURAL HOLD-OUT, so the table is override-dominated rather than
mostly-lifted (revisit):

  - pandas's frame/series/index equality assertions are APPROXIMATE BY DEFAULT
    (they carry ``rtol``/``atol``), so derive_vocab correctly classifies them as
    ``approx`` from the signature. The seat reclassifies them to ``equality`` via
    an override, lifting as ``=`` ONLY when ``check_exact=True`` is pinned (a
    ``require_true`` kwarg) and no relation-altering keyword is present.
  - the rest of the recognised vocabulary is the ``pandas._testing`` (``tm``)
    names, which live in a DIFFERENT module than ``pandas.testing``; derive_vocab
    reads one module, so they arrive as an explicit ``other`` override.

Revisit: teach derive_vocab the approximate-by-default -> conditional-equality
pattern (and optionally multi-module reads) so pandas becomes mostly-lifted.

SOUNDNESS: a frame/series equality is opaque-EUF on both sides, so the scalar
assertions carry the consistency teeth; this seat's load-bearing job is sound
refusal-of-approximate plus witness keying.
"""

from __future__ import annotations

from provekit_lift_py_tests.assertion_layer import lift_file_assertions
from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab

PANDAS_TESTING = derive_vocab(
    "pandas.testing",
    "pandas-testing",
    overrides={
        # approximate-by-default -> conditional exact equality (see check_exact).
        "equality": frozenset({
            "assert_frame_equal",
            "assert_series_equal",
            "assert_index_equal",
            "assert_extension_array_equal",
        }),
        # the pandas._testing (tm) vocabulary: recognised so nothing real is
        # silently skipped, claimed + refused. assert_equal is EXCLUDED -- it is
        # the cross-library generic owned by the numpy seat.
        "other": frozenset({
            "assert_almost_equal",
            "assert_attr_equal",
            "assert_categorical_equal",
            "assert_class_equal",
            "assert_contains_all",
            "assert_copy",
            "assert_datetime_array_equal",
            "assert_dict_equal",
            "assert_indexing_slices_equivalent",
            "assert_interval_array_equal",
            "assert_is_sorted",
            "assert_metadata_equivalent",
            "assert_numpy_array_equal",
            "assert_period_array_equal",
            "assert_produces_warning",
            "assert_sp_array_equal",
            "assert_timedelta_array_equal",
        }),
    },
    # Only these two don't change the relation; anything else -> refuse.
    harmless_kwargs=frozenset({"obj", "check_exact"}),
    # Approximate by default: lift as `=` only when check_exact=True is pinned.
    require_true_kwargs=frozenset({"check_exact"}),
)


def lift_file_pandas_testing(source: str, source_path: str):
    """Lift pandas.testing assertions from a test file."""
    return lift_file_assertions(source, source_path, PANDAS_TESTING)
