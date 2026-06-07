# SPDX-License-Identifier: Apache-2.0
"""The pandas false-pass gate, invariant across however the exception is structured.

pandas frame/series equality is APPROXIMATE BY DEFAULT (rtol/atol). The one thing
that must never break: ``assert_frame_equal(a, b)`` with no ``check_exact=True`` must
NOT lift as exact equality (it would be a false pass), while pinning
``check_exact=True`` may lift. This holds whether pandas's vocab is a hand override
or derived from ``check_exact`` detection.
"""
import json

import pytest

pytest.importorskip("pandas")
from provekit_lift_py_tests.assertion_vocab_lift import learn_vocab, lift_test_file

DEFAULT = (
    "from pandas.testing import assert_frame_equal\n"
    "def test_x():\n    assert_frame_equal(a, b)\n"
)
PINNED = (
    "from pandas.testing import assert_frame_equal\n"
    "def test_x():\n    assert_frame_equal(a, b, check_exact=True)\n"
)


def _ws(tmp_path):
    # Use the canonical externalized exception (whatever its current shape is).
    import os
    src = os.path.join(
        os.path.dirname(__file__), "..", "..",
        "provekit-lift-py-tests", "tests", "workspace",
    )
    return os.path.abspath(src)


def test_frame_equal_is_in_equality_but_gated_on_check_exact(tmp_path):
    ws = _ws(tmp_path)
    learn_vocab.cache_clear()
    v = learn_vocab("pandas.testing", (
        __import__("os").path.join(ws, ".provekit", "vocab-exceptions"),
    ))
    # It is an equality assertion (conditionally), and check_exact gates the lift.
    assert "assert_frame_equal" in v.equality
    assert "check_exact" in v.require_true_kwargs


def test_default_frame_equal_is_refused_not_lifted_as_exact(tmp_path):
    out = lift_test_file(DEFAULT, "t.py", workspace_root=_ws(tmp_path))
    assert out.lifted == 0, [w.reason for w in out.warnings]


def test_check_exact_true_may_lift(tmp_path):
    out = lift_test_file(PINNED, "t.py", workspace_root=_ws(tmp_path))
    assert out.lifted == 1, [w.reason for w in out.warnings]


def test_check_exact_rule_derives_conditional_equality_with_no_override():
    # The "better pandas" derivation rules: with NO override at all, the frame funcs
    # derive as equality (via the check_exact toggle) and the toggle becomes a
    # require_true kwarg; extra_modules pulls the tm vocabulary into `other`.
    from provekit_lift_py_tests.assertion_vocab_lift import derive_vocab
    d = derive_vocab("pandas.testing", "pandas-testing", extra_modules=("pandas._testing",))
    assert "assert_frame_equal" in d.equality
    assert "assert_series_equal" in d.equality
    assert "check_exact" in d.require_true_kwargs
    # tm vocabulary derived as `other`; the cross-library assert_equal is refused
    # (in `other`), NOT lifted as exact equality
    assert "assert_categorical_equal" in d.other
    assert "assert_equal" not in d.equality
