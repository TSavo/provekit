from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-lift-python-source/src"
PY_TESTS_SRC = ROOT / "implementations/python/provekit-lift-py-tests/src"
REALIZER_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PY_TESTS_SRC) not in sys.path:
    sys.path.insert(0, str(PY_TESTS_SRC))
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))
if str(REALIZER_SRC) not in sys.path:
    sys.path.insert(0, str(REALIZER_SRC))

from provekit_lift_python_source.bind_lifter import lift_source
from provekit_lift_python_source.bind_rpc import dispatch, initialize_result
from provekit_lift_python_source.canonical import cid_of_json
from provekit_realize_python_core.realizer import emit_stub


def _cid(ch: str) -> str:
    return "blake3-512:" + ch * 128


def _formula_gte_x_zero() -> dict:
    return {
        "args": [
            {"kind": "var", "name": "x"},
            {
                "kind": "const",
                "sort": {"kind": "primitive", "name": "Int"},
                "value": 0,
            },
        ],
        "kind": "atomic",
        "name": "≥",
    }


def _formula_out_eq_x() -> dict:
    return {
        "args": [{"kind": "var", "name": "out"}, {"kind": "var", "name": "x"}],
        "kind": "atomic",
        "name": "=",
    }


def _contract_comment_payload(role: str, formula: dict, fol_text: str) -> tuple[dict, str]:
    payload = {
        "artifact_kind": "provekit-contract-comment-sugar",
        "concept_site_cid": _cid("1"),
        "contract_cid": _cid("2"),
        "emitted_by": {
            "kit_cid": _cid("3"),
            "kit_kind": "realize",
            "target_language": "python",
        },
        "fol_text": fol_text,
        "ir_formula_jcs": formula,
        "ir_formula_jcs_cid": cid_of_json(formula),
        "local_contract_cid": _cid("2"),
        "loss_record_cid": _cid("4"),
        "policy_cid": _cid("5"),
        "role": role,
        "schema_version": "1",
        "sugar_dict_cid": _cid("6"),
    }
    return payload, cid_of_json(payload)


def _comment_lines(payload: dict, payload_cid: str) -> str:
    return (
        "# provekit-contract: "
        + json.dumps(payload, separators=(",", ":"), ensure_ascii=False)
        + "\n"
        + f"# provekit-contract-payload-cid: {payload_cid}\n"
    )


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


def test_bind_lift_recovers_contract_comment_witness() -> None:
    payload, payload_cid = _contract_comment_payload("pre", _formula_gte_x_zero(), "x >= 0")
    source = (
        _comment_lines(payload, payload_cid)
        + "# concept: identity\n"
        + "def wrap_identity(x: int) -> int:\n"
        + "    return x\n"
    )

    result = lift_source(source, "pkg/foo.py")

    assert result.diagnostics == []
    witnesses = result.ir[0]["witnesses"]
    assert len(witnesses) == 1
    witness = witnesses[0]
    assert witness["role"] == "pre"
    assert witness["source_kind"] == "native-surface"
    assert witness["confidence_basis_points"] == 10000
    assert witness["predicate"] == _formula_gte_x_zero()
    assert witness["predicate_text"] == "x >= 0"
    assert witness["extension_fields"] == {
        "concept_site_cid": _cid("1"),
        "contract_cid": _cid("2"),
        "ir_formula_jcs_cid": cid_of_json(_formula_gte_x_zero()),
        "local_contract_cid": _cid("2"),
        "loss_record_cid": _cid("4"),
        "payload_cid": payload_cid,
        "policy_cid": _cid("5"),
        "sugar_dict_cid": _cid("6"),
        "surface": "contract-comment-sugar",
    }


def test_bind_lift_recovers_docstring_contract_comment_witness() -> None:
    payload, payload_cid = _contract_comment_payload("post", _formula_out_eq_x(), "out == x")
    source = (
        "def wrap_identity(x: int) -> int:\n"
        "    \"\"\"\n"
        "    human prose stays non-authoritative\n"
        "    provekit-contract: "
        + json.dumps(payload, separators=(",", ":"), ensure_ascii=False)
        + "\n"
        f"    provekit-contract-payload-cid: {payload_cid}\n"
        "    \"\"\"\n"
        "    return x\n"
    )

    result = lift_source(source, "pkg/foo.py")

    assert result.diagnostics == []
    witness = result.ir[0]["witnesses"][0]
    assert witness["role"] == "post"
    assert witness["predicate"] == _formula_out_eq_x()
    assert witness["extension_fields"]["payload_cid"] == payload_cid


def test_bind_lift_contract_comment_fails_closed_for_bad_payloads() -> None:
    payload, payload_cid = _contract_comment_payload("pre", _formula_gte_x_zero(), "x >= 0")
    cases = [
        _comment_lines({**payload, "role": "sideways"}, cid_of_json({**payload, "role": "sideways"})),
        _comment_lines({**payload, "schema_version": "2"}, cid_of_json({**payload, "schema_version": "2"})),
        _comment_lines({**payload, "ir_formula_jcs_cid": _cid("7")}, cid_of_json({**payload, "ir_formula_jcs_cid": _cid("7")})),
        _comment_lines(payload, _cid("8")),
        "# provekit-contract: {not json}\n",
    ]

    for prefix in cases:
        result = lift_source(prefix + "def f(x: int) -> int:\n    return x\n", "pkg/foo.py")

        assert result.ir[0].get("witnesses", []) == []
        assert any(diag["kind"] == "contract-comment-invalid" for diag in result.diagnostics)


def test_bind_lift_recovers_decorator_contract_witnesses() -> None:
    source = (
        "from provekit_lift_py_tests.decorators import contract\n"
        "@contract(pre=\"x >= 0\", post=\"out >= 0\")\n"
        "def nonnegative_identity(x: int) -> int:\n"
        "    return x\n"
    )

    result = lift_source(source, "pkg/foo.py")

    assert result.diagnostics == []
    witnesses = result.ir[0]["witnesses"]
    assert [witness["role"] for witness in witnesses] == ["pre", "post"]
    assert [witness["predicate_text"] for witness in witnesses] == ["x >= 0", "out >= 0"]
    assert all(witness["source_kind"] == "native-surface" for witness in witnesses)
    assert all(
        witness["extension_fields"]["surface"] == "python-decorator-contract"
        for witness in witnesses
    )


def test_python_realize_then_lift_keeps_contract_and_concept_site_cids() -> None:
    realized = emit_stub(
        function="wrap_identity",
        params=["x"],
        param_types=["int"],
        return_type="int",
        concept_name="identity",
        contract={
            "concept_site_cid": _cid("1"),
            "local_contract_cid": _cid("2"),
            "object_fcm_cid": _cid("3"),
            "origin": "evidence-lift[native-surface]",
            "discharge_verdict": "exact",
            "witnesses": [
                {
                    "role": "pre",
                    "predicate": _formula_gte_x_zero(),
                    "predicate_text": "x >= 0",
                    "source_kind": "native-surface",
                }
            ],
        },
    )

    result = lift_source(realized["source"], "generated.py")

    assert result.diagnostics == []
    witness = result.ir[0]["witnesses"][0]
    assert witness["extension_fields"]["concept_site_cid"] == _cid("1")
    assert witness["extension_fields"]["contract_cid"] == _cid("2")
    assert witness["extension_fields"]["local_contract_cid"] == _cid("2")
    assert witness["predicate"] == _formula_gte_x_zero()
