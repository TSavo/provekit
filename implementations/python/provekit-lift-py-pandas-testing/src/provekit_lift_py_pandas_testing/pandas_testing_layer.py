# SPDX-License-Identifier: Apache-2.0
"""pandas ``pandas.testing`` assertion-vocabulary lift surface.

This is now the generic assertion fold (``provekit_lift_py_tests.assertion_layer``)
plus a vocabulary table -- the same fold the numpy and sklearn seats use.

SOUNDNESS -- pandas makes the EXACT/APPROXIMATE split SHARPER because it is
approximate BY DEFAULT: assert_frame_equal / assert_series_equal compare floats
with a tolerance unless ``check_exact`` is pinned. So an equality assertion is
lifted as ``=`` ONLY when ``check_exact=True`` is explicitly present (a
``require_true`` kwarg) AND no relation-altering keyword is present (only
``obj``/``check_exact`` are harmless; ``check_like``/``rtol``/``check_dtype``/...
fall outside the whitelist and refuse). The whole ``pandas._testing`` (``tm``)
vocabulary is recognised so nothing real is silently skipped; ``assert_equal`` is
excluded (cross-library generic, owned by the numpy/generic seat).

Note: a frame/series equality is opaque-EUF on both sides, so z3 cannot contradict
two opaque DataFrame constructors -- the consistency teeth come from scalar
assertions; this seat's load-bearing job is sound refusal-of-approximate plus
witness keying.
"""

from __future__ import annotations

from provekit_lift_py_tests.assertion_layer import AssertionVocab, lift_file_assertions

PANDAS_TESTING = AssertionVocab(
    label="pandas-testing",
    equality=frozenset({
        "assert_frame_equal",
        "assert_series_equal",
        "assert_index_equal",
        "assert_extension_array_equal",
    }),
    # The rest of the pandas._testing (tm) vocabulary: recognised so nothing real
    # is silently skipped, claimed + refused (sound lifting is future work).
    # assert_equal is EXCLUDED -- cross-library generic owned by the numpy seat.
    other=frozenset({
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
    # Only these two don't change the relation; anything else -> refuse.
    harmless_kwargs=frozenset({"obj", "check_exact"}),
    # Approximate by default: lift as `=` only when check_exact=True is pinned.
    require_true_kwargs=frozenset({"check_exact"}),
)


def lift_file_pandas_testing(source: str, source_path: str):
    """Lift pandas.testing assertions from a test file."""
    return lift_file_assertions(source, source_path, PANDAS_TESTING)
