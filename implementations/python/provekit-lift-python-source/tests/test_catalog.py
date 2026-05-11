from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
SPECS = ROOT / "menagerie/python-language-signature/specs"


def _load(name: str) -> dict[str, object]:
    with (SPECS / name).open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _op(name: str) -> dict[str, object]:
    return _load(f"op_{name}.spec.json")


def test_python_signature_catalog_has_draft_version_and_expected_ops() -> None:
    signature = _load("language_signature_python.spec.json")

    assert signature["kind"] == "language_signature"
    assert signature["fn_name"] == "python"
    assert signature["version"] == "0.1.0-draft"
    assert "op_source-unit.spec.json" in signature["operations"]
    assert "op_assign.spec.json" in signature["operations"]
    assert "op_compare.spec.json" in signature["operations"]
    assert "op_unknown.spec.json" not in signature["operations"]
    assert "op_binop.spec.json" not in signature["operations"]


def test_required_arity_shapes_are_explicit() -> None:
    assign_shape = _op("assign")["post"]["arity_shape"]
    assert assign_shape == {
        "kind": "named",
        "slots": [{"name": "target"}, {"name": "value"}],
    }

    seq_shape = _op("seq")["post"]["arity_shape"]
    assert seq_shape == {"kind": "positional", "arity": 2}

    add_shape = _op("add")["post"]["arity_shape"]
    assert add_shape == {
        "kind": "named",
        "slots": [{"name": "lhs"}, {"name": "rhs"}],
    }

    and_shape = _op("and")["post"]["arity_shape"]
    assert and_shape == {
        "kind": "named",
        "slots": [
            {"name": "lhs"},
            {"name": "rhs", "evaluation": "unevaluated"},
        ],
    }

    source_shape = _op("source-unit")["post"]["arity_shape"]
    assert source_shape == {
        "kind": "named",
        "slots": [
            {"name": "bytes", "evaluation": "unevaluated", "slot_sort": "literal"},
            {"name": "operational_term"},
        ],
    }
