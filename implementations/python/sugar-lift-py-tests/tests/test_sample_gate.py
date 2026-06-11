from __future__ import annotations

import sys
import textwrap
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[4]
PY_TESTS_SRC = ROOT / "implementations/python/sugar-lift-py-tests/src"
PKG_SRC = ROOT / "implementations/python/sugar-lift-python-source/src"
for p in (str(PY_TESTS_SRC), str(PKG_SRC)):
    if p not in sys.path:
        sys.path.insert(0, p)

from sugar_lift_py_tests.layer2 import lift_file_layer2
from sugar_lift_py_tests.translate_universe import translate_universe_for_callee

VENDOR_TRANSLATE = '''
_tab = bytes.maketrans(b"+/", b"-_")


def _enc(s):
    return s


def urlsafe(s):
    return _enc(s).translate(_tab)
'''


@pytest.fixture()
def fixture_dir(tmp_path, monkeypatch):
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()

    def write(module_name: str, source: str) -> None:
        (tmp_path / f"{module_name}.py").write_text(textwrap.dedent(source))

    return write


# --- the gate's evidence is the VENDOR's corpus ---


def test_clean_vendor_vectors_license_the_universe(fixture_dir):
    fixture_dir("vendg2_ok", VENDOR_TRANSLATE)
    fixture_dir(
        "test_vendg2_ok",
        """
        import vendg2_ok

        def test_vendor_vector():
            assert vendg2_ok.urlsafe("abc") == "abc"
            assert vendg2_ok.urlsafe("xy") == "x-y"
        """,
    )
    universe, refusal = translate_universe_for_callee("vendg2_ok.urlsafe")
    assert refusal is None
    assert universe is not None
    assert universe.vendor_vectors_checked == 2
    assert universe.vendor_vector_source.endswith("test_vendg2_ok.py")


def test_violating_vendor_vector_refuses_the_walk(fixture_dir):
    # The VENDOR's own test swears an output containing a forbidden char:
    # our walk misread the body or the vendor contradicts their own source.
    # Either way the universe is refused at the walk, loudly.
    fixture_dir("vendg2_bad", VENDOR_TRANSLATE)
    fixture_dir(
        "test_vendg2_bad",
        """
        import vendg2_bad

        def test_vendor_vector():
            assert vendg2_bad.urlsafe("abc") == "ab+c"
        """,
    )
    universe, refusal = translate_universe_for_callee("vendg2_bad.urlsafe")
    assert universe is None
    assert refusal is not None
    assert "sample-gate" in refusal.reason
    assert "'ab+c'" in refusal.reason


def test_assert_equal_style_vendor_vectors_count(fixture_dir):
    fixture_dir("vendg2_ue", VENDOR_TRANSLATE)
    fixture_dir(
        "test_vendg2_ue",
        """
        import unittest
        import vendg2_ue

        class T(unittest.TestCase):
            def test_vector(self):
                self.assertEqual(vendg2_ue.urlsafe(b"abc"), b"abc")
        """,
    )
    universe, refusal = translate_universe_for_callee("vendg2_ue.urlsafe")
    assert refusal is None
    assert universe.vendor_vectors_checked == 1


def test_no_vendor_corpus_is_said_plainly(fixture_dir):
    fixture_dir("vendg2_lone", VENDOR_TRANSLATE)
    universe, refusal = translate_universe_for_callee("vendg2_lone.urlsafe")
    assert refusal is None
    assert universe.vendor_vectors_checked == 0
    assert universe.vendor_vector_source is None


# --- the marquee property: a consumer's lying claim is NOT gate evidence ---


def test_consumer_bad_twin_does_not_eat_the_universe(fixture_dir):
    # The bad twin asserts the urlsafe confusion. The universe row must STILL
    # be emitted -- the contradiction is check's verdict to deliver, not the
    # gate's evidence to consume. (The first gate version got this wrong and
    # disarmed the marquee's own refutation.)
    fixture_dir("vendg2_twin", VENDOR_TRANSLATE)
    out = lift_file_layer2(
        textwrap.dedent(
            """
            import vendg2_twin

            def test_confusion():
                assert vendg2_twin.urlsafe("abc") == "ab+c"
            """
        ),
        "bad_twin.py",
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    atoms = [
        a
        for d in out.decls
        if d.name.endswith("::assertion") and d.inv is not None
        for a in _iter_conjuncts(d.inv)
        if a.name == "str.chars-not-in-set"
    ]
    assert len(atoms) == 1
    # the lying equality and the universe share one conjoined inv: the
    # contradiction is check's to refute at verify
    assert any(d.name.endswith("::assertion") for d in out.decls)
