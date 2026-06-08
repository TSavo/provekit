# SPDX-License-Identifier: Apache-2.0
"""Direct test of the generic assertion fold -- a library is a vocab table, not
code. A brand-new (fictional) vocabulary lifts with zero new lifter code."""
from sugar_lift_py_tests.assertion_layer import AssertionVocab, lift_file_assertions


# a fifth "library", invented here, costs ZERO lifter code -- just this table.
FAKE = AssertionVocab(
    label="fake-testing",
    equality=frozenset({"assert_same"}),
    truth=frozenset({"assert_truthy"}),
    approx=frozenset({"assert_close"}),
    other=frozenset({"assert_raises_fake"}),
)


def _lift(src):
    return lift_file_assertions(src, "<t>", FAKE)


def test_equality_lifts():
    out = _lift("def test_a():\n    assert_same(x, 1)\n")
    assert out.lifted == 1 and len(out.decls) == 1


def test_same_var_contradicts():
    out = _lift("def test_a():\n    r=make()\n    assert_same(r,1)\n    assert_same(r,2)\n")
    assert out.lifted == 1
    lefts = {getattr((getattr(d.inv,'operands',None) or [d.inv])[0].args[0],'name',None) for d in out.decls}
    assert lefts == {"r$0"}, lefts


def test_truth_lifts():
    out = _lift("def test_a():\n    assert_truthy(x)\n")
    assert out.lifted == 1


def test_approx_refused():
    out = _lift("def test_a():\n    assert_close(x, y)\n")
    assert out.lifted == 0 and out.seen == 1
    assert any("approximate assertion" in w.reason for w in out.warnings)


def test_other_claimed_refused():
    out = _lift("def test_a():\n    assert_raises_fake(x)\n")
    assert out.seen == 1 and out.lifted == 0 and not out.decls


def test_not_our_vocab_not_claimed():
    out = _lift("def test_a():\n    assert_same_OTHER(x, 1)\n")
    assert out.seen == 0


def test_require_true_kwarg_gate():
    # a vocab with a require_true kwarg refuses unless it's pinned True (pandas shape)
    v = AssertionVocab(label="k", equality=frozenset({"eq"}),
                       harmless_kwargs=frozenset({"exact"}), require_true_kwargs=frozenset({"exact"}))
    assert lift_file_assertions("def test_a():\n    eq(x,y)\n", "<t>", v).lifted == 0
    assert lift_file_assertions("def test_a():\n    eq(x,y,exact=True)\n", "<t>", v).lifted == 1
