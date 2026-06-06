# SPDX-License-Identifier: Apache-2.0
"""The numpy.testing vocabulary IS lift output: ``NUMPY_TESTING`` is produced by
``derive_vocab`` reading numpy.testing's own source, not a hand-authored table.

These tests assert the soundness-critical properties of the lifted vocabulary
directly (they would catch a regression in derive_vocab or a numpy change that
moved an assertion across the exact/approximate boundary).
"""
import pytest

pytest.importorskip("numpy")
from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab
from provekit_lift_py_numpy_testing.numpy_testing_layer import NUMPY_TESTING


def test_the_seat_vocab_is_derive_vocab_output():
    # The seat is literally the lifter's output (same overrides + tolerances),
    # not a frozen table that a test happens to match.
    rebuilt = derive_vocab(
        "numpy.testing",
        "numpy-testing",
        overrides={
            "equality": frozenset({"assert_equal", "assert_equals"}),
            "truth": frozenset({"assert_"}),
        },
        tolerances=NUMPY_TESTING.tolerances,
    )
    assert rebuilt.equality == NUMPY_TESTING.equality
    assert rebuilt.approx == NUMPY_TESTING.approx
    assert rebuilt.other == NUMPY_TESTING.other


def test_soundness_critical_approx_split_came_from_source():
    # The approximate family (tolerance params in the signature) -- lifting any of
    # these as exact `=` would be a false pass. Derived from source, no human input.
    for name in (
        "assert_allclose",
        "assert_almost_equal",
        "assert_array_almost_equal",
        "assert_approx_equal",
        "assert_array_almost_equal_nulp",
        "assert_array_max_ulp",
    ):
        assert name in NUMPY_TESTING.approx, name
    # exact equality recovered from the operator.__eq__ delegation
    assert "assert_array_equal" in NUMPY_TESTING.equality
    # nothing approximate leaked into equality
    assert not (NUMPY_TESTING.approx & NUMPY_TESTING.equality)


def test_override_remainder_is_exactly_the_structurally_opaque_names():
    # The only hand input: names whose structure can't be read (recursive dispatch,
    # truthiness). Everything else is derived.
    assert {"assert_equal", "assert_equals"} <= NUMPY_TESTING.equality
    assert NUMPY_TESTING.truth == frozenset({"assert_"})


def test_other_set_tracks_the_installed_library_not_a_drifting_list():
    # derive_vocab found real numpy.testing functions a hand list had missed,
    # proving the `other` set follows the installed numpy rather than drifting.
    assert "assert_array_compare" in NUMPY_TESTING.other
