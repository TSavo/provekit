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
def vendor(tmp_path, monkeypatch):
    def write(module_name: str) -> None:
        (tmp_path / f"{module_name}.py").write_text(textwrap.dedent(VENDOR_TRANSLATE))

    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()
    return write


def _lift(source: str):
    return lift_file_layer2(textwrap.dedent(source), "test_mod.py")


def _universe_decls(out):
    return [d for d in out.decls if d.name.endswith("::universe")]


def _gate_warnings(out):
    return [w for w in out.warnings if "sample-gate rejected" in w.reason]


def test_clean_vector_licenses_the_universe(vendor):
    vendor("vendgate_ok")
    out = _lift(
        """
        import vendgate_ok

        def test_urlsafe():
            assert vendgate_ok.urlsafe("abc") == "abc"
        """
    )
    assert len(_universe_decls(out)) == 1
    assert not _gate_warnings(out)


def test_violating_vector_rejects_the_universe_loudly(vendor):
    # The sworn vector itself contains a forbidden char: either the walk
    # misread the body or the vendor contradicts their own source. The
    # universe must NOT ship; the point rows must remain; the rejection is
    # loud and names the vector and the chars.
    vendor("vendgate_bad")
    out = _lift(
        """
        import vendgate_bad

        def test_urlsafe():
            assert vendgate_bad.urlsafe("abc") == "ab+c"
        """
    )
    assert not _universe_decls(out)
    warnings = _gate_warnings(out)
    assert len(warnings) == 1
    assert "'ab+c'" in warnings[0].reason and "'+'" in warnings[0].reason
    # the sworn point rows survive the rejection
    assert any(d.name.endswith("::assertion") for d in out.decls)


def test_bytes_vector_runs_the_gate_too(vendor):
    vendor("vendgate_bytes")
    out = _lift(
        """
        import vendgate_bytes

        def test_urlsafe():
            assert vendgate_bytes.urlsafe(b"abc") == b"ab/c"
        """
    )
    assert not _universe_decls(out)
    assert _gate_warnings(out)


def test_gate_scopes_per_base(vendor):
    # A violating vector on one base must not reject the universe minted on
    # a DIFFERENT base (different args -> different subject).
    vendor("vendgate_scope")
    out = _lift(
        """
        import vendgate_scope

        def test_clean():
            assert vendgate_scope.urlsafe("abc") == "abc"

        def test_dirty():
            assert vendgate_scope.urlsafe("xyz") == "x+z"
        """
    )
    rows = _universe_decls(out)
    assert len(rows) == 1  # the clean base keeps its universe
    assert len(_gate_warnings(out)) == 1  # the dirty base rejected, loudly
