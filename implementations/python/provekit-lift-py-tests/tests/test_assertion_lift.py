# SPDX-License-Identifier: Apache-2.0
"""The one assertion lifter, exercised against the real libraries (numpy, pandas,
sklearn). ``lift_test_file`` learns each library's vocabulary from a test file's
imports and applies the EXTERNALIZED `.provekit/vocab-exceptions/<module>.json`
declaration. No per-library seat. This consolidates what were three seat test
suites into one, since there is now one lifter.
"""
import json
import os

import pytest

from provekit_lift_py_tests.assertion_vocab_lift import (
    derive_vocab,
    learn_vocab,
    lift_test_file,
)

# The canonical externalized exceptions live next to this test, in a workspace.
WORKSPACE = os.path.join(os.path.dirname(__file__), "workspace")
EXC_DIR = (os.path.join(WORKSPACE, ".provekit", "vocab-exceptions"),)


def _drop_workspace(tmp_path, module, exception):
    d = tmp_path / ".provekit" / "vocab-exceptions"
    d.mkdir(parents=True)
    if exception is not None:
        (d / f"{module}.json").write_text(json.dumps(exception))
    return str(tmp_path)


# --- numpy --------------------------------------------------------------------

NUMPY_SRC = (
    "from numpy.testing import assert_almost_equal, assert_equal\n"
    "def test_tol():\n    assert_almost_equal(x, y)\n"
    "def test_exact():\n    assert_equal(p, q)\n"
)


def test_numpy_learns_from_imports_with_canonical_exception():
    pytest.importorskip("numpy")
    out = lift_test_file(NUMPY_SRC, "t.py", workspace_root=WORKSPACE)
    assert out.lifted == 2, [w.reason for w in out.warnings]


def test_exception_is_external_data_dropping_a_file_flips_behavior(tmp_path):
    pytest.importorskip("numpy")
    learn_vocab.cache_clear()
    bare = lift_test_file(NUMPY_SRC, "t.py", workspace_root=_drop_workspace(tmp_path / "a", "numpy.testing", None))
    assert bare.lifted == 0  # pure derivation: tolerance refused, assert_equal -> other
    learn_vocab.cache_clear()
    exc = {
        "overrides": {"equality": ["assert_equal", "assert_equals"], "truth": ["assert_"]},
        "tolerances": [
            {"name": "assert_almost_equal", "decimal_default": 7},
            {"name": "assert_array_almost_equal", "decimal_default": 6},
        ],
    }
    withf = lift_test_file(NUMPY_SRC, "t.py", workspace_root=_drop_workspace(tmp_path / "b", "numpy.testing", exc))
    assert withf.lifted == 2  # behavior changed by adding a DATA FILE, no code change


def test_numpy_soundness_split_derives_from_source():
    pytest.importorskip("numpy")
    d = derive_vocab("numpy.testing", "numpy-testing")
    for n in ("assert_allclose", "assert_almost_equal", "assert_array_almost_equal",
              "assert_array_almost_equal_nulp", "assert_array_max_ulp"):
        assert n in d.approx, n
    assert "assert_array_equal" in d.equality
    assert not (d.approx & d.equality)


def test_numpy_private_impl_does_not_shadow_public_exception():
    pytest.importorskip("numpy")
    src = (
        "from numpy.testing import assert_equal\n"
        "from numpy.testing._private.utils import assert_array_equal\n"
        "def test_x():\n    assert_equal(a, b)\n"
    )
    out = lift_test_file(src, "t.py", workspace_root=WORKSPACE)
    assert out.lifted == 1, [w.reason for w in out.warnings]


# --- sklearn ------------------------------------------------------------------


def test_sklearn_lifts_with_zero_exception():
    pytest.importorskip("sklearn")
    # sklearn has no exception file -> pure derivation. assert_array_equal is exact,
    # the allclose family is approximate (refused without a tolerance spec).
    d = learn_vocab("sklearn.utils._testing", EXC_DIR)
    assert "assert_array_equal" in d.equality
    assert "assert_allclose" in d.approx


# --- pandas: the false-pass gate (must hold) ----------------------------------

PANDAS_DEFAULT = "from pandas.testing import assert_frame_equal\ndef test_x():\n    assert_frame_equal(a, b)\n"
PANDAS_PINNED = "from pandas.testing import assert_frame_equal\ndef test_x():\n    assert_frame_equal(a, b, check_exact=True)\n"


def test_pandas_frame_equal_default_is_refused_not_exact():
    pytest.importorskip("pandas")
    out = lift_test_file(PANDAS_DEFAULT, "t.py", workspace_root=WORKSPACE)
    assert out.lifted == 0, [w.reason for w in out.warnings]


def test_pandas_frame_equal_check_exact_true_may_lift():
    pytest.importorskip("pandas")
    out = lift_test_file(PANDAS_PINNED, "t.py", workspace_root=WORKSPACE)
    assert out.lifted == 1, [w.reason for w in out.warnings]


def test_pandas_derivation_rules_shrink_the_exception():
    pytest.importorskip("pandas")
    # check_exact -> conditional equality; extra_modules pulls the tm vocabulary
    d = derive_vocab("pandas.testing", "pandas-testing", extra_modules=("pandas._testing",))
    assert "assert_frame_equal" in d.equality
    assert "check_exact" in d.require_true_kwargs
    assert "assert_categorical_equal" in d.other
    assert "assert_equal" not in d.equality  # cross-library generic stays refused
