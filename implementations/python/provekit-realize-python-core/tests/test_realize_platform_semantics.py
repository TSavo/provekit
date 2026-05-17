from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.platform_semantics import declaration, dimension_values


EXPECTED_DIMENSIONS = {
    "ArithmeticOverflow": "ArbitraryPrecision",
    "IntegerDivisionRounding": "Floor",
    "ShiftMode": "Arithmetic",
    "NullSemantics": "RaiseZeroDivisionError",
    "BitwiseSemantics": "TwosComplement",
}


def test_python_realize_platform_semantics_declaration_shape() -> None:
    values = dimension_values()
    assert {item["dimension_name"]: item["value_name"] for item in values} == EXPECTED_DIMENSIONS
    for item in values:
        assert item["compare_to"] == {
            "kind": "atomic",
            "name": f"python:{item['value_name']}",
            "args": [],
        }
        assert item["cid"].startswith("blake3-512:")

    semantics = declaration()
    assert semantics["tags"]
    for tag in semantics["tags"]:
        assert set(tag["dimensions"]) == set(EXPECTED_DIMENSIONS)
        assert tag["op_cid"].startswith("blake3-512:")
        assert tag["cid"].startswith("blake3-512:")
