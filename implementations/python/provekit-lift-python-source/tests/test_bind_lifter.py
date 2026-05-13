from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-lift-python-source/src"
PY_TESTS_SRC = ROOT / "implementations/python/provekit-lift-py-tests/src"
if str(PY_TESTS_SRC) not in sys.path:
    sys.path.insert(0, str(PY_TESTS_SRC))
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_lift_python_source.bind_lifter import lift_source
from provekit_lift_python_source.bind_rpc import dispatch, initialize_result
from provekit_lift_python_source.canonical import cid_of_json


def test_bind_lift_source_emits_language_neutral_entries() -> None:
    source = (
        "# concept: identity\n"
        "# @requires: x >= 0\n"
        "# @ensures: result >= 0\n"
        "def wrap_identity(x: int) -> int:\n"
        "    return x\n"
        "\n"
        "class Cell:\n"
        "    # unrelated comment\n"
        "    # concept: bool-cell\n"
        "    @staticmethod\n"
        "    def toggle(flag: bool) -> bool:\n"
        "        return not flag\n"
        "\n"
        "# concept: option\n"
        "def maybe_first(items: list) -> int:\n"
        "    first = 0\n"
        "    if len(items) == 0:\n"
        "        return -1\n"
        "    else:\n"
        "        return items[0]\n"
    )

    result = lift_source(source, "pkg/foo.py")

    assert result.diagnostics == []
    assert [entry["fn_name"] for entry in result.ir] == [
        "wrap_identity",
        "toggle",
        "maybe_first",
    ]
    assert [entry["concept_annotation"] for entry in result.ir] == [
        "identity",
        "bool-cell",
        "option",
    ]
    assert result.ir[0]["attr_pre"] == "x >= 0"
    assert result.ir[0]["attr_post"] == "result >= 0"
    assert result.ir[0]["param_names"] == ["x"]
    assert result.ir[0]["param_types"] == ["int"]
    assert result.ir[0]["return_type"] == "int"
    assert result.ir[0]["term_shape"] == {
        "kind": "body",
        "stmts": [{"kind": "exit"}],
    }
    assert result.ir[2]["term_shape"] == {
        "kind": "body",
        "stmts": [
            {"kind": "let"},
            {
                "cond": {"kind": "rel", "op": "=="},
                "else": {"kind": "block", "stmts": [{"kind": "exit"}]},
                "kind": "if",
                "then": {"kind": "block", "stmts": [{"kind": "exit"}]},
            },
        ],
    }
    for entry in result.ir:
        assert entry["kind"] == "bind-lift-entry"
        assert entry["file"] == "pkg/foo.py"
        assert entry["term_shape_cid"] == cid_of_json(entry["term_shape"])
    assert "python:" not in json.dumps(result.ir, sort_keys=True)


def test_bind_lift_filters_unnamed_concepts_and_void_return() -> None:
    source = (
        "# concept: UNNAMED-CONCEPT-deadbeef\n"
        "def generated(x):\n"
        "    x += 1\n"
        "    return None\n"
        "\n"
        "def no_annotation(y) -> None:\n"
        "    return None\n"
    )

    result = lift_source(source, "foo.py")

    assert [entry["concept_annotation"] for entry in result.ir] == [None, None]
    assert result.ir[0]["param_names"] == ["x"]
    assert result.ir[0]["param_types"] == ["Any"]
    assert result.ir[0]["return_type"] == "Any"
    assert result.ir[0]["term_shape"]["stmts"][0] == {"kind": "assign"}
    assert result.ir[1]["return_type"] == "()"


def test_bind_rpc_initialize_declares_bind_ir_surface() -> None:
    result = initialize_result()

    assert result["name"] == "provekit-lift-python-bind"
    assert result["protocol_version"] == "pep/1.7.0"
    assert result["capabilities"] == {
        "authoring_surfaces": ["python", "python-bind"],
        "emits_signed_mementos": False,
        "ir_version": "bind-ir/1.0.0",
    }


def test_bind_rpc_lift_returns_ir_document(tmp_path: Path) -> None:
    source = tmp_path / "foo.py"
    source.write_text("# concept: identity\ndef f(x: int) -> int:\n    return x\n", encoding="utf-8")

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "lift",
            "params": {
                "workspace_root": str(tmp_path),
                "source_paths": ["foo.py"],
            },
        }
    )

    assert response["id"] == 7
    assert response["result"]["kind"] == "ir-document"
    assert response["result"]["diagnostics"] == []
    assert response["result"]["ir"][0]["concept_annotation"] == "identity"
