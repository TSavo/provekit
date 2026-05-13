from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.realizer import emit_stub


def test_identity_renders_python_function_from_body_template() -> None:
    result = emit_stub(
        function="wrap_identity",
        params=["x"],
        param_types=["int"],
        return_type="int",
        concept_name="identity",
    )

    assert result == {
        "source": "def wrap_identity(x):\n    return x\n",
        "is_stub": False,
        "extension": "py",
    }


def test_trinity_concepts_render_real_bodies() -> None:
    cases = [
        ("do_nothing", [], [], "()", "unit", "def do_nothing():\n    return None\n"),
        ("toggle", ["flag"], ["bool"], "bool", "bool-cell", "def toggle(flag):\n    return not flag\n"),
        (
            "assert_positive",
            ["x"],
            ["int"],
            "int",
            "assert",
            'def assert_positive(x):\n    if x <= 0:\n        raise RuntimeError("assertion failed: concept:assert violated")\n    return x\n',
        ),
        (
            "sum_items",
            ["items"],
            ["list[int]"],
            "int",
            "list",
            "def sum_items(items):\n    acc = 0\n    for v in items:\n        acc += v\n    return acc\n",
        ),
        ("maybe_first", ["items"], ["list[int]"], "int", "option", "def maybe_first(items):\n    return -1 if len(items) == 0 else items[0]\n"),
        (
            "option_bind_double",
            ["items"],
            ["list[int]"],
            "int",
            "option-bind",
            "def option_bind_double(items):\n    if len(items) == 0:\n        return -1\n    v = items[0]\n    return -1 if v <= 0 else v * 2\n",
        ),
        ("swap_pair", ["a", "b"], ["int", "int"], "tuple[int, int]", "pair", "def swap_pair(a, b):\n    return (b, a)\n"),
        ("safe_divide", ["num", "denom"], ["int", "int"], "int", "result", "def safe_divide(num, denom):\n    return -1 if denom == 0 else int(num / denom)\n"),
        (
            "safe_divide_then_double",
            ["num", "denom"],
            ["int", "int"],
            "int",
            "result-bind",
            "def safe_divide_then_double(num, denom):\n    if denom == 0:\n        return -1\n    q = int(num / denom)\n    return -1 if q < 0 else q * 2\n",
        ),
        (
            "retry_until_success",
            ["max_attempts"],
            ["int"],
            "bool",
            "retry-loop",
            "def retry_until_success(max_attempts):\n    attempt = 0\n    while attempt < max_attempts:\n        attempt += 1\n        if attempt >= 1:\n            return True\n    return False\n",
        ),
        (
            "classify",
            ["x"],
            ["int"],
            "int",
            "tagged-union",
            "def classify(x):\n    if x < 0:\n        return 0\n    if x == 0:\n        return 1\n    return 2\n",
        ),
    ]

    for function, params, param_types, return_type, concept_name, expected in cases:
        result = emit_stub(function, params, param_types, return_type, concept_name)
        assert result["source"] == expected
        assert result["is_stub"] is False
        assert result["extension"] == "py"


def test_unknown_concept_falls_back_to_python_stub() -> None:
    result = emit_stub("missing", ["x"], ["int"], "int", "missing-concept")

    assert result == {
        "source": 'def missing(x):\n    raise NotImplementedError("provekit-bind canonical: missing-concept")\n',
        "is_stub": True,
        "extension": "py",
    }
