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

from provekit_lift_py_tests.canonicalizer import jcs_hash, vobj, vstr

from provekit_lift_python_source.canonical import canonical_json_bytes, cid_of_json
from provekit_lift_python_source.compiler import compile_body_term, compile_ir_document
from provekit_lift_python_source.lifter import lift_source
from provekit_lift_python_source.rpc import initialize_result


def _canon(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _contract(ir: list[dict[str, object]], suffix: str) -> dict[str, object]:
    for item in ir:
        if str(item.get("fnName", "")).endswith(suffix):
            return item
    raise AssertionError(f"missing contract ending in {suffix!r}: {ir!r}")


def _ctor_names(node: object) -> list[str]:
    if isinstance(node, dict):
        names = [str(node["name"])] if node.get("kind") == "ctor" else []
        for child in node.get("args", []):
            names.extend(_ctor_names(child))
        return names
    if isinstance(node, list):
        names: list[str] = []
        for child in node:
            names.extend(_ctor_names(child))
        return names
    return []


def test_lift_function_emits_source_unit_and_python_ops() -> None:
    source = "GLOBAL = 3\n\ndef add_one(x):\n    y = x + GLOBAL\n    return y\n"

    result = lift_source(source, "pkg/mod.py")

    assert result.refusals == []
    assert [item["fnName"] for item in result.ir] == [
        "<source-unit:pkg/mod.py>",
        "pkg.mod.add_one",
    ]

    source_unit = result.ir[0]["post"]["args"][1]
    assert source_unit["name"] == "python:source-unit"
    assert source_unit["args"][0]["value"] == source

    function_contract = result.ir[1]
    assert function_contract["formals"] == ["x"]
    assert function_contract["effects"] == [{"kind": "reads", "target": "GLOBAL"}]
    body = function_contract["post"]["args"][1]
    assert _ctor_names(body) == [
        "python:seq",
        "python:assign",
        "python:add",
        "python:return",
    ]
    assert all(not name.endswith(":unknown") for name in _ctor_names(result.ir))


def test_refuses_unhandled_syntax_without_unknown_ops() -> None:
    source = "def bad(xs):\n    return [x for x in xs]\n"

    result = lift_source(source, "badmodule.py")

    assert len(result.ir) == 1
    assert result.ir[0]["fnName"] == "<source-unit:badmodule.py>"
    assert result.ir[0]["post"]["args"][1]["name"] == "python:source-unit"
    assert result.ir[0]["post"]["args"][1]["args"][1]["name"] == "python:pass"
    assert len(result.refusals) == 1
    refusal = result.refusals[0]
    assert refusal["kind"] == "unhandled-syntax"
    assert refusal["function"] == "badmodule.bad"
    assert refusal["line"] == 2
    assert "ListComp" in refusal["reason"]
    assert "python:unknown" not in _canon(result.refusals)
    assert "python:skip" not in _canon(result.refusals)


def test_effects_are_sorted_and_loop_cid_is_blake3_512() -> None:
    source = (
        "def total(xs):\n"
        "    acc = 0\n"
        "    for x in xs:\n"
        "        acc = acc + x\n"
        "    print(acc)\n"
        "    return acc\n"
    )

    result = lift_source(source, "loops.py")

    contract = _contract(result.ir, ".total")
    effects = contract["effects"]
    assert [effect["kind"] for effect in effects] == ["io", "opaque_loop"]
    loop_cid = effects[1]["loopCid"]
    assert loop_cid.startswith("blake3-512:")
    assert len(loop_cid) == len("blake3-512:") + 128


def test_cid_of_json_uses_protocol_jcs_control_char_escaping() -> None:
    value = {"source": "def f():\n  return 1\n"}
    expected = (
        "blake3-512:17778ed1c9bbda5f202e07c2e35c3e9009c03cb314229818cb34b895b1f66fe1e"
        "25347b433538cf3a3848d07ebae051728fe5996cd408f067476ae97c943be05"
    )

    assert jcs_hash(vobj([("source", vstr(value["source"]))])) == expected
    assert cid_of_json(value) == expected


def test_compile_lift_roundtrip_ir_document_is_byte_identical() -> None:
    source = "def f(x):\n    y = x + 1\n    return y\n"

    first = lift_source(source, "roundtrip.py")
    compiled = compile_ir_document(first.ir)
    second = lift_source(compiled, "roundtrip.py")

    assert _canon(second.ir) == _canon(first.ir)


def test_compile_function_contract_without_source_unit_uses_ast_unparse() -> None:
    source = "def f(x):\n    y = x + 1\n    return y\n"
    lifted = lift_source(source, "roundtrip.py")
    contract = _contract(lifted.ir, ".f")

    compiled = compile_ir_document([contract])

    assert "def f(x):" in compiled
    assert "y = x + 1" in compiled
    assert "return y" in compiled


def test_compile_lift_roundtrip_body_term_is_byte_identical() -> None:
    source = "def f(x):\n    y = x + 1\n    return y\n"
    lifted = lift_source(source, "roundtrip.py")
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip.py")
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


def test_rpc_initialize_declares_python_source_draft() -> None:
    result = initialize_result()

    assert result["version"] == "0.1.0-draft"
    assert result["protocol_version"] == "provekit-lift/1"
    assert result["dialect"] == "python-source"
    assert result["capabilities"]["authoring_surfaces"] == ["python-source"]
    assert result["capabilities"]["emits_signed_mementos"] is False
