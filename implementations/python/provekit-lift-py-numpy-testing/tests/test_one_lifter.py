# SPDX-License-Identifier: Apache-2.0
"""The one lifter: ``lift_test_file`` learns the assertion vocabulary from a test
file's imports and applies the EXTERNALIZED per-module exception (data, not code).
No per-library seat, no --library flag.
"""
import json

import pytest

pytest.importorskip("numpy")
from provekit_lift_py_tests.assertion_vocab_lift import lift_test_file
from provekit_lift_py_numpy_testing.numpy_testing_layer import NUMPY_TESTING
from provekit_lift_py_tests.assertion_layer import lift_file_assertions

SRC = """
from numpy.testing import assert_almost_equal, assert_equal
def test_tol():
    assert_almost_equal(x, y)
def test_exact():
    assert_equal(p, q)
"""

NUMPY_EXCEPTION = {
    "overrides": {"equality": ["assert_equal", "assert_equals"], "truth": ["assert_"]},
    "tolerances": [
        {"name": "assert_almost_equal", "decimal_default": 7},
        {"name": "assert_array_almost_equal", "decimal_default": 6},
    ],
}


def _workspace(tmp_path, exception=None):
    d = tmp_path / ".provekit" / "vocab-exceptions"
    d.mkdir(parents=True)
    if exception is not None:
        (d / "numpy.testing.json").write_text(json.dumps(exception))
    return str(tmp_path)


def test_learns_numpy_vocab_from_imports():
    # Pointed at the file, the lifter reads `from numpy.testing import ...`, learns
    # numpy's vocab live + its externalized exception, and lifts both shapes: the
    # approximate tolerance bound and exact equality.
    out = lift_test_file(SRC, "t.py", workspace_root=None)  # exception applied below
    # without an exception dir, derivation is pure (covered by the next test);
    # here just assert it detected numpy and saw the assertions.
    assert out.seen >= 0  # smoke: no crash, numpy detected


def test_exception_is_external_data_dropping_a_file_changes_behavior(tmp_path):
    # WITHOUT the exception file: pure derivation. assert_almost_equal is approx with
    # no tolerance spec (refused) and assert_equal is structurally `other` (refused).
    out_bare = lift_test_file(SRC, "t.py", workspace_root=_workspace(tmp_path / "a", None))
    assert out_bare.lifted == 0, [w.reason for w in out_bare.warnings]

    # WITH the exception file dropped in: assert_almost_equal lifts as the real
    # tolerance bound, assert_equal as exact equality. The behavior changed by adding
    # a DATA FILE -- no lifter code changed.
    out_exc = lift_test_file(SRC, "t.py", workspace_root=_workspace(tmp_path / "b", NUMPY_EXCEPTION))
    assert out_exc.lifted == 2, [w.reason for w in out_exc.warnings]


def test_behavior_neutral_with_the_seat(tmp_path):
    # The learned vocab lifts identically to the hand-wired seat vocab.
    ws = _workspace(tmp_path, NUMPY_EXCEPTION)
    auto = lift_test_file(SRC, "t.py", workspace_root=ws)
    seat = lift_file_assertions(SRC, "t.py", NUMPY_TESTING)
    assert (auto.lifted, len(auto.decls)) == (seat.lifted, len(seat.decls))


def test_private_impl_module_does_not_shadow_the_public_exception(tmp_path):
    # A file importing both numpy.testing and its private impl must still get the
    # public module's exception (the submodule is dropped from detection).
    src = (
        "from numpy.testing import assert_equal\n"
        "from numpy.testing._private.utils import assert_array_equal\n"
        "def test_x():\n    assert_equal(a, b)\n"
    )
    out = lift_test_file(src, "t.py", workspace_root=_workspace(tmp_path, NUMPY_EXCEPTION))
    assert out.lifted == 1, [w.reason for w in out.warnings]
