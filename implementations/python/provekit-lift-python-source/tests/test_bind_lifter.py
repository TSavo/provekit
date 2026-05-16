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


CONCEPT_SKIP_CID = (
    "blake3-512:"
    "9a905548a44fce23882b17d857d275d7822bd235ab71dbf786cd991563cc1de9e"
    "610594f50ad3c89a3b7eeb43234a31b36caa8031914c85227158030669c63cb"
)


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


def _concept_citation_payload(overrides: dict | None = None) -> tuple[dict, str]:
    args = [{"kind": "var", "name": "x"}]
    payload = {
        "args_jcs": args,
        "args_jcs_cid": cid_of_json(args),
        "artifact_kind": "provekit-concept-citation-comment-sugar",
        "concept_cid": CONCEPT_SKIP_CID,
        "concept_name": "concept:skip",
        "concept_site_cid": _cid("a"),
        "emitted_by": {
            "kit_cid": _cid("b"),
            "kit_id": "provekit-realize-python-core@0.1.0",
            "kit_kind": "realize",
            "target_language": "python",
            "target_library_tag": "python",
        },
        "loss_record_cid": _cid("c"),
        "operation_kind": "skip",
        "policy_cid": _cid("d"),
        "schema_version": "1",
        "shape_cid": CONCEPT_SKIP_CID,
        "sugar_dict_cid": _cid("e"),
        "term_position": [0],
    }
    if overrides:
        payload.update(overrides)
    return payload, cid_of_json(payload)


def _concept_comment_lines(payload: dict, payload_cid: str) -> str:
    return (
        "# provekit-concept: "
        + json.dumps(
            payload,
            separators=(",", ":"),
            sort_keys=True,
            ensure_ascii=False,
        )
        + "\n"
        + f"# provekit-concept-payload-cid: {payload_cid}\n"
    )


def _concept_diagnostics(result: object) -> set[str]:
    diagnostics = getattr(result, "diagnostics")
    return {diag["kind"] for diag in diagnostics}


def _catalog_concept_cid(name: str) -> str:
    index_path = ROOT / "menagerie/concept-shapes/catalog/index.json"
    index = json.loads(index_path.read_text(encoding="utf-8"))
    for entry in index["entries"].values():
        if entry.get("kind") == "algorithm" and entry.get("name") == name:
            cid = entry["cid"]
            assert isinstance(cid, str)
            return cid
    raise AssertionError(f"missing catalog concept: {name}")


def _walk_objects(value: object) -> list[dict]:
    found: list[dict] = []
    if isinstance(value, dict):
        found.append(value)
        for child in value.values():
            found.extend(_walk_objects(child))
    elif isinstance(value, list):
        for child in value:
            found.extend(_walk_objects(child))
    return found


def _operator_atoms(term_shape: dict) -> list[dict]:
    return [node for node in _walk_objects(term_shape) if "op_cid" in node]


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
    eq_shape = {
        "args": [{"kind": "call"}, {"kind": "opaque"}],
        "concept_name": "concept:eq",
        "kind": "rel",
        "op": "==",
        "op_cid": _catalog_concept_cid("concept:eq"),
    }
    neg_shape = {
        "args": [{"kind": "opaque"}],
        "concept_name": "concept:neg",
        "kind": "neg",
        "op": "-",
        "op_cid": _catalog_concept_cid("concept:neg"),
    }
    assert result.ir[2]["term_shape"] == {
        "kind": "body",
        "stmts": [
            {"kind": "let"},
            {
                "cond": eq_shape,
                "else": {"kind": "block", "stmts": [{"kind": "exit"}]},
                "kind": "if",
                "then": {
                    "kind": "block",
                    "stmts": [{"kind": "exit", "value": neg_shape}],
                },
            },
        ],
    }
    for entry in result.ir:
        assert entry["kind"] == "bind-lift-entry"
        assert entry["file"] == "pkg/foo.py"
        assert entry["term_shape_cid"] == cid_of_json(entry["term_shape"])
    assert "python:" not in json.dumps(result.ir, sort_keys=True)


def test_bind_lift_preserves_operator_concept_cid_atoms() -> None:
    source = (
        "def add(x, y):\n"
        "    return x + y\n"
        "\n"
        "def sub(x, y):\n"
        "    return x - y\n"
        "\n"
        "def mul(x, y):\n"
        "    return x * y\n"
        "\n"
        "def div(x, y):\n"
        "    return x / y\n"
        "\n"
        "def eq(x, y):\n"
        "    return x == y\n"
        "\n"
        "def ne(x, y):\n"
        "    return x != y\n"
        "\n"
        "def lt(x, y):\n"
        "    return x < y\n"
        "\n"
        "def le(x, y):\n"
        "    return x <= y\n"
        "\n"
        "def gt(x, y):\n"
        "    return x > y\n"
        "\n"
        "def ge(x, y):\n"
        "    return x >= y\n"
        "\n"
        "def logical_not(x):\n"
        "    return not x\n"
    )

    result = lift_source(source, "pkg/operators.py")

    assert result.diagnostics == []
    expected = {
        "add": ("concept:add", _catalog_concept_cid("concept:add")),
        "sub": ("concept:sub", _catalog_concept_cid("concept:sub")),
        "mul": ("concept:mul", _catalog_concept_cid("concept:mul")),
        "div": ("concept:div", _catalog_concept_cid("concept:div")),
        "eq": ("concept:eq", _catalog_concept_cid("concept:eq")),
        "ne": ("concept:ne", _catalog_concept_cid("concept:ne")),
        "lt": ("concept:lt", _catalog_concept_cid("concept:lt")),
        "le": ("concept:le", _catalog_concept_cid("concept:le")),
        "gt": ("concept:gt", _catalog_concept_cid("concept:gt")),
        "ge": ("concept:ge", _catalog_concept_cid("concept:ge")),
        "logical_not": ("concept:not", _catalog_concept_cid("concept:not")),
    }
    by_name = {entry["fn_name"]: entry for entry in result.ir}
    for fn_name, (concept_name, op_cid) in expected.items():
        atoms = _operator_atoms(by_name[fn_name]["term_shape"])
        assert {"concept_name": concept_name, "op_cid": op_cid}.items() <= atoms[0].items()


def test_bind_lift_operator_atoms_make_distinct_term_shape_cids() -> None:
    add = lift_source("def f(x, y):\n    return x + y\n", "pkg/add.py").ir[0]
    sub = lift_source("def f(x, y):\n    return x - y\n", "pkg/sub.py").ir[0]

    assert add["term_shape_cid"] != sub["term_shape_cid"]


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
    assign_shape = result.ir[0]["term_shape"]["stmts"][0]
    assert assign_shape["kind"] == "assign"
    assert {
        "concept_name": "concept:add",
        "op_cid": _catalog_concept_cid("concept:add"),
    }.items() <= assign_shape["value"].items()
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


def test_concept_citation_relift_recovers_identity() -> None:
    payload, payload_cid = _concept_citation_payload()
    source = (
        "def transport_skip(x: object):\n"
        + "    "
        + _concept_comment_lines(payload, payload_cid).replace("\n", "\n    ")
        + "pass\n"
    )

    result = lift_source(source, "pkg/foo.py")

    assert result.diagnostics == []
    citations = result.ir[0]["concept_citations"]
    assert len(citations) == 1
    citation = citations[0]
    assert citation["concept_cid"] == payload["concept_cid"]
    assert citation["operation_kind"] == payload["operation_kind"]
    assert citation["shape_cid"] == payload["shape_cid"]
    assert citation["term_position"] == payload["term_position"]
    assert citation["args_jcs_cid"] == payload["args_jcs_cid"]
    assert citation["source_kind"] == "native-surface"
    assert citation["extension_fields"]["payload_cid"] == payload_cid
    assert result.ir[0]["witnesses"] == []


def test_concept_citation_payload_cid_mismatch_refuses() -> None:
    payload, _payload_cid = _concept_citation_payload()
    source = _concept_comment_lines(payload, _cid("8")) + "def f(x: object):\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert result.ir[0].get("concept_citations", []) == []
    assert "concept-citation:payload-cid-mismatch" in _concept_diagnostics(result)


def test_concept_citation_args_cid_mismatch_refuses() -> None:
    payload, payload_cid = _concept_citation_payload({"args_jcs_cid": _cid("8")})
    source = _concept_comment_lines(payload, payload_cid) + "def f(x: object):\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert result.ir[0].get("concept_citations", []) == []
    assert "concept-citation:args-cid-mismatch" in _concept_diagnostics(result)


def test_concept_citation_unknown_schema_version_refuses() -> None:
    payload, payload_cid = _concept_citation_payload({"schema_version": "999"})
    source = _concept_comment_lines(payload, payload_cid) + "def f(x: object):\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert result.ir[0].get("concept_citations", []) == []
    assert "concept-citation:unknown-schema-version" in _concept_diagnostics(result)


def test_concept_citation_orphan_payload_cid_line_refuses() -> None:
    source = "# provekit-concept-payload-cid: " + _cid("8") + "\ndef f():\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert result.ir[0].get("concept_citations", []) == []
    assert "concept-citation:orphan-cid-line" in _concept_diagnostics(result)


def test_concept_citation_round_trip() -> None:
    args = [{"kind": "var", "name": "x"}]
    transported_op = {
        "args_jcs": args,
        "concept_cid": CONCEPT_SKIP_CID,
        "concept_name": "concept:skip",
        "concept_site_cid": _cid("a"),
        "loss_record_cid": _cid("c"),
        "operation_kind": "skip",
        "policy_cid": _cid("d"),
        "shape_cid": CONCEPT_SKIP_CID,
        "sugar_dict_cid": _cid("e"),
        "term_position": [0],
    }
    emitted = emit_stub(
        function="transport_skip",
        params=["x"],
        param_types=["object"],
        return_type="()",
        concept_name="missing-python-skip-carrier",
        transported_op=transported_op,
    )

    result = lift_source(emitted["source"], "pkg/foo.py")

    assert result.diagnostics == []
    assert result.ir[0]["concept_citations"][0]["concept_cid"] == transported_op["concept_cid"]
    assert result.ir[0]["concept_citations"][0]["args_jcs_cid"] == cid_of_json(args)


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


def test_concept_citation_shape_mismatch_refuses_surrounding_relift() -> None:
    from provekit_lift_python_source.bind_lifter import _concept_shape_catalog

    assert _concept_shape_catalog() is not None, (
        "catalog must be present for this test to exercise row-7 path"
    )

    # shape_cid differs from what the catalog records for CONCEPT_SKIP_CID
    payload, payload_cid = _concept_citation_payload({"shape_cid": _cid("8")})
    bad_fn_source = (
        "def good_fn(x: int) -> int:\n"
        "    return x\n"
        "\n"
        "def bad_fn(x: object):\n"
        "    " + _concept_comment_lines(payload, payload_cid).replace("\n", "\n    ") + "    pass\n"
    )

    result = lift_source(bad_fn_source, "pkg/foo.py")

    # bad_fn must not produce an IR entry; good_fn must still produce one
    fn_names = [entry["fn_name"] for entry in result.ir]
    assert "good_fn" in fn_names
    assert "bad_fn" not in fn_names
    assert len(result.ir) == 1
    assert "concept-citation:shape-mismatch" in _concept_diagnostics(result)


def test_concept_citation_operation_kind_mismatch_refuses_surrounding_relift() -> None:
    from provekit_lift_python_source.bind_lifter import _concept_shape_catalog

    assert _concept_shape_catalog() is not None, (
        "catalog must be present for this test to exercise row-8 path"
    )

    # operation_kind differs from what the catalog records for CONCEPT_SKIP_CID ("skip")
    payload, payload_cid = _concept_citation_payload({"operation_kind": "not-skip"})
    bad_fn_source = (
        "def good_fn(x: int) -> int:\n"
        "    return x\n"
        "\n"
        "def bad_fn(x: object):\n"
        "    " + _concept_comment_lines(payload, payload_cid).replace("\n", "\n    ") + "    pass\n"
    )

    result = lift_source(bad_fn_source, "pkg/foo.py")

    fn_names = [entry["fn_name"] for entry in result.ir]
    assert "good_fn" in fn_names
    assert "bad_fn" not in fn_names
    assert len(result.ir) == 1
    assert "concept-citation:operation-kind-mismatch" in _concept_diagnostics(result)


def test_concept_citation_missing_operation_kind_field_tags_as_malformed_json() -> None:
    # Build payload without operation_kind to trigger the row-1 (malformed-json) path
    args = [{"kind": "var", "name": "x"}]
    payload: dict = {
        "args_jcs": args,
        "args_jcs_cid": cid_of_json(args),
        "artifact_kind": "provekit-concept-citation-comment-sugar",
        "concept_cid": CONCEPT_SKIP_CID,
        "concept_name": "concept:skip",
        "concept_site_cid": _cid("a"),
        "emitted_by": {
            "kit_cid": _cid("b"),
            "kit_id": "provekit-realize-python-core@0.1.0",
            "kit_kind": "realize",
            "target_language": "python",
            "target_library_tag": "python",
        },
        "loss_record_cid": _cid("c"),
        "policy_cid": _cid("d"),
        "schema_version": "1",
        "shape_cid": CONCEPT_SKIP_CID,
        "sugar_dict_cid": _cid("e"),
        "term_position": [0],
        # operation_kind intentionally omitted
    }
    payload_cid = cid_of_json(payload)
    bad_fn_source = (
        "def good_fn(x: int) -> int:\n"
        "    return x\n"
        "\n"
        "def bad_fn(x: object):\n"
        "    " + _concept_comment_lines(payload, payload_cid).replace("\n", "\n    ") + "    pass\n"
    )

    result = lift_source(bad_fn_source, "pkg/foo.py")

    # drop-and-continue: bad_fn still gets an IR entry, just with no citation
    fn_names = [entry["fn_name"] for entry in result.ir]
    assert "good_fn" in fn_names
    assert "bad_fn" in fn_names
    assert result.ir[1].get("concept_citations", []) == []
    assert "concept-citation:malformed-json" in _concept_diagnostics(result)
