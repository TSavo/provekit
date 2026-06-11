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

import ast

from sugar_lift_py_tests.ir import _ConstStr, _Ctor, str_const
from sugar_lift_py_tests.layer2 import (
    _comparison_from_symbol,
    _euf_args_all_concrete,
    _translate_term,
    lift_file_layer2,
)
from sugar_lift_py_tests.translate_universe import translate_universe_for_callee


def _term(expr: str):
    return _translate_term(ast.parse(expr, mode="eval").body)


# --- the bytes term: ASCII-gated, kind-wrapped ---


def test_ascii_bytes_lift_as_wrapped_ctor():
    term = _term('b"YmFy"')
    assert isinstance(term, _Ctor)
    assert term.name == "python:bytes"
    assert term.args == (str_const("YmFy"),)


def test_bytes_and_str_are_distinct_terms():
    assert _term('b"a"') != _term('"a"')


def test_non_ascii_bytes_refuse_loudly():
    with pytest.raises(ValueError, match="ASCII-gated"):
        _term('b"\\xff"')


# --- euf concreteness: a bytes literal is a fixed value ---


def test_bytes_arg_is_concrete_for_euf():
    call_term = _Ctor("callresult_f_a1", (_term('b"abc"'),))
    assert _euf_args_all_concrete(call_term)


def test_symbolic_arg_still_not_concrete():
    from sugar_lift_py_tests.ir import make_var

    call_term = _Ctor("callresult_f_a1", (make_var("x"),))
    assert not _euf_args_all_concrete(call_term)


# --- the kind-constant guard ---


def test_bytes_vs_str_literal_equality_refuses():
    with pytest.raises(ValueError, match="kind-constant"):
        _comparison_from_symbol("=", _term('b"a"'), _term('"a"'))


def test_bytes_vs_str_literal_inequality_refuses():
    with pytest.raises(ValueError, match="kind-constant"):
        _comparison_from_symbol("≠", _term('"a"'), _term('b"a"'))


def test_bytes_vs_bytes_equality_lifts():
    formula = _comparison_from_symbol("=", _term('b"a"'), _term('b"a"'))
    assert formula is not None


# --- end to end: bytes args form euf terms; the universe row attaches ---

VENDOR_TRANSLATE = '''
_tab = bytes.maketrans(b"+/", b"-_")


def _enc(s):
    return s


def urlsafe(s):
    return _enc(s).translate(_tab)
'''


def test_bytes_args_carry_the_universe_row(tmp_path, monkeypatch):
    (tmp_path / "vendbytes_l2.py").write_text(textwrap.dedent(VENDOR_TRANSLATE))
    monkeypatch.syspath_prepend(str(tmp_path))
    translate_universe_for_callee.cache_clear()
    out = lift_file_layer2(
        textwrap.dedent(
            """
            import vendbytes_l2

            def test_urlsafe():
                assert vendbytes_l2.urlsafe(b"abc") == b"abc"
            """
        ),
        "test_mod.py",
    )
    from sugar_lift_py_tests.layer2 import _iter_conjuncts

    atoms = [
        a
        for d in out.decls
        if d.name.endswith("::assertion") and d.inv is not None
        for a in _iter_conjuncts(d.inv)
        if a.name == "str.chars-not-in-set"
    ]
    assert len(atoms) == 1, [d.name for d in out.decls]
    assert atoms[0].args[1] == str_const("+/")
