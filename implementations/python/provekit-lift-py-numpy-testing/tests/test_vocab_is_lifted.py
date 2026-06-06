# SPDX-License-Identifier: Apache-2.0
"""The numpy.testing vocabulary is LIFT OUTPUT, not a hand-authored table.

``derive_vocab`` reads numpy.testing's own source and classifies each assert
function. This test proves the hand table is exactly that derivation plus a small,
labeled, structurally-opaque remainder -- so the table could be regenerated from
the library, and a numpy version bump that changed the assert signatures would
change the lifted vocabulary (which is correct: the vocab is the library's
contract, pinned to its version).
"""
import pytest

np = pytest.importorskip("numpy")
from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab
from provekit_lift_py_numpy_testing.numpy_testing_layer import NUMPY_TESTING as HAND


def test_approx_split_lifts_itself_from_source():
    # The SOUNDNESS-CRITICAL split -- which assertions are approximate (and must
    # never be lifted as exact ``=``) -- is derived from the signatures alone.
    derived = derive_vocab("numpy.testing", "numpy-testing")
    assert derived.approx == HAND.approx, (
        f"derived approx {sorted(derived.approx)} != hand {sorted(HAND.approx)}"
    )
    # the exact-equality assertion is recovered from its operator.__eq__ delegation
    assert "assert_array_equal" in derived.equality


def test_hand_table_is_derivation_plus_a_labeled_remainder():
    # The hand table = the derived classification + an explicit override for the
    # structurally-opaque names. Nothing in the hand table is unaccounted for.
    derived = derive_vocab("numpy.testing", "numpy-testing")
    override_equality = HAND.equality - derived.equality   # recursive dispatch, no single op
    override_truth = HAND.truth                            # assert_ truthiness
    assert override_equality == {"assert_equal", "assert_equals"}, sorted(override_equality)
    assert override_truth == {"assert_"}, sorted(override_truth)
    # and rebuilding from the derivation + that exact override reproduces the table
    rebuilt = derive_vocab(
        "numpy.testing", "numpy-testing",
        overrides={"equality": frozenset(override_equality), "truth": override_truth},
    )
    assert rebuilt.equality == HAND.equality
    assert rebuilt.truth == HAND.truth
    assert rebuilt.approx == HAND.approx
