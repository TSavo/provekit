from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

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


def _operator_concepts(term_shape: dict) -> list[str]:
    return [str(atom["concept_name"]) for atom in _operator_atoms(term_shape)]


def _concept_comment_surfaces(term_shape: dict) -> list[str]:
    surfaces: list[str] = []
    for atom in _operator_atoms(term_shape):
        if atom.get("concept_name") != "concept:comment":
            continue
        args = atom.get("args", [])
        if not isinstance(args, list) or not args:
            continue
        surface = args[0].get("value") if isinstance(args[0], dict) else None
        if isinstance(surface, str):
            surfaces.append(surface)
    return surfaces


def _gamma_shape(concept_name: str, args: list[dict] | None = None) -> dict:
    return {
        "args": args or [],
        "concept_name": concept_name,
        "op_cid": _catalog_concept_cid(concept_name),
    }


def _assert_absent_keys(value: object, forbidden: set[str]) -> None:
    if isinstance(value, dict):
        assert forbidden.isdisjoint(value.keys())
        for child in value.values():
            _assert_absent_keys(child, forbidden)
    elif isinstance(value, list):
        for child in value:
            _assert_absent_keys(child, forbidden)


def test_bind_lift_erases_signature_types_from_bind_ir_entries() -> None:
    source = (
        "def add(left: int, right: int) -> int:\n"
        "    return left + right\n"
        "\n"
        "def generated(value):\n"
        "    return value\n"
    )

    result = lift_source(source, "pkg/foo.py")

    assert result.diagnostics == []
    assert len(result.ir) == 2
    add, generated = result.ir
    assert add["param_names"] == ["left", "right"]
    assert "param_types" not in add
    assert "return_type" not in add
    assert "param_types" not in generated
    assert "return_type" not in generated


def test_bind_lift_emits_operand_binding_sidecar_with_integer_positions() -> None:
    source = (
        "def nested(a, b, c):\n"
        "    return (a + b) * c\n"
    )

    result = lift_source(source, "pkg/math.py")

    assert result.diagnostics == []
    assert len(result.ir) == 1
    entry = result.ir[0]
    assert entry["source_function_name"] == "nested"
    assert entry["operand_bindings"] == [
        {"position": [0, 0], "symbol": "a"},
        {"position": [0, 1], "symbol": "b"},
        {"position": [1], "symbol": "c"},
    ]
    assert all(
        isinstance(binding["position"], list)
        and all(isinstance(part, int) for part in binding["position"])
        for binding in entry["operand_bindings"]
    )
    assert "fn_name" not in entry


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
    assert len(result.ir) == 3
    _assert_absent_keys(
        result.ir,
        {"attr_pre", "attr_post", "concept_annotation", "fn_name"},
    )
    assert result.ir[0]["param_names"] == ["x"]
    assert "param_types" not in result.ir[0]
    assert "return_type" not in result.ir[0]
    assert result.ir[0]["term_shape"] == {}
    assert result.ir[1]["term_shape"] == _gamma_shape("concept:not", [{}])
    for entry in result.ir:
        assert entry["kind"] == "bind-lift-entry"
        assert entry["term_shape_cid"] == cid_of_json(entry["term_shape"])
        _assert_absent_keys(
            entry,
            {"file", "fn_line", "line", "column", "col"},
        )
        _assert_absent_keys(entry["term_shape"], {"op", "kind"})
    assert "python:" not in json.dumps(result.ir, sort_keys=True)


def test_bind_lift_contract_surfaces_do_not_emit_envelope_hash_fields() -> None:
    sources = [
        (
            "# @requires: x > 0\n"
            "def add(x, y):\n"
            "    return x + y\n"
        ),
        (
            "from provekit_lift_py_tests.decorators import contract\n"
            "@contract(pre=\"x > 0\")\n"
            "def add(x, y):\n"
            "    return x + y\n"
        ),
    ]
    forbidden = {"attr_pre", "attr_post", "concept_annotation", "fn_name"}

    for source in sources:
        result = lift_source(source, "pkg/add.py")

        assert result.diagnostics == []
        assert len(result.ir) == 1
        assert forbidden.isdisjoint(result.ir[0])
        _assert_absent_keys(result.ir[0], forbidden)


def test_bind_lift_emits_gamma_shape_for_add_return() -> None:
    result = lift_source("def add(x, y):\n    return x + y\n", "pkg/add.py")

    assert result.diagnostics == []
    assert result.ir[0]["term_shape"] == _gamma_shape("concept:add", [{}, {}])


def test_bind_lift_discriminates_sub_gamma_shape() -> None:
    add = lift_source("def f(x, y):\n    return x + y\n", "pkg/add.py").ir[0]
    sub = lift_source("def f(x, y):\n    return x - y\n", "pkg/sub.py").ir[0]

    assert add["term_shape"] == _gamma_shape("concept:add", [{}, {}])
    assert sub["term_shape"] == _gamma_shape("concept:sub", [{}, {}])
    assert add["term_shape_cid"] != sub["term_shape_cid"]


def test_bind_lift_preserves_nested_gamma_composition() -> None:
    result = lift_source(
        "def f(x, y, z):\n    return x + y * z\n",
        "pkg/nested.py",
    )

    assert result.diagnostics == []
    assert result.ir[0]["term_shape"] == _gamma_shape(
        "concept:add",
        [{}, _gamma_shape("concept:mul", [{}, {}])],
    )


def test_bind_lift_emits_gamma_shape_for_statement_concepts() -> None:
    cases = [
        (
            "def f():\n"
            "    while True:\n"
            "        pass\n",
            _gamma_shape("concept:while", [{}, _gamma_shape("concept:skip")]),
        ),
        (
            "def f(items):\n"
            "    for item in items:\n"
            "        pass\n",
            _gamma_shape("concept:for", [_gamma_shape("concept:skip")]),
        ),
        (
            "def f():\n"
            "    while True:\n"
            "        break\n",
            _gamma_shape("concept:while", [{}, _gamma_shape("concept:break")]),
        ),
        (
            "def f():\n"
            "    while True:\n"
            "        continue\n",
            _gamma_shape("concept:while", [{}, _gamma_shape("concept:continue")]),
        ),
        (
            "def f(x, y):\n"
            "    x = y\n",
            _gamma_shape("concept:assign", [{}, {}]),
        ),
        (
            "def f(g, x):\n"
            "    g(x)\n",
            _gamma_shape("concept:call", [{}, {}]),
        ),
        (
            "def f(g, x, y):\n"
            "    x = y\n"
            "    g(x)\n",
            _gamma_shape(
                "concept:seq",
                [
                    _gamma_shape("concept:assign", [{}, {}]),
                    _gamma_shape("concept:call", [{}, {}]),
                ],
            ),
        ),
    ]

    for source, expected in cases:
        result = lift_source(source, "pkg/statements.py")

        assert result.diagnostics == []
        assert result.ir[0]["term_shape"] == expected


def test_bind_lift_discriminates_while_and_for_statement_concepts() -> None:
    while_entry = lift_source(
        "def f(items):\n"
        "    while True:\n"
        "        pass\n",
        "pkg/while.py",
    ).ir[0]
    for_entry = lift_source(
        "def f(items):\n"
        "    for item in items:\n"
        "        pass\n",
        "pkg/for.py",
    ).ir[0]

    assert while_entry["term_shape"] == _gamma_shape(
        "concept:while",
        [{}, _gamma_shape("concept:skip")],
    )
    assert for_entry["term_shape"] == _gamma_shape(
        "concept:for",
        [_gamma_shape("concept:skip")],
    )
    assert while_entry["term_shape_cid"] != for_entry["term_shape_cid"]


def test_bind_lift_preserves_nested_statement_gamma_composition() -> None:
    result = lift_source(
        "def f(x):\n"
        "    if x > 0:\n"
        "        while x > 0:\n"
        "            x = x - 1\n"
        "    return x\n",
        "pkg/nested_statements.py",
    )

    assert result.diagnostics == []
    assert result.ir[0]["term_shape"] == _gamma_shape(
        "concept:conditional",
        [
            _gamma_shape("concept:gt", [{}, {}]),
            _gamma_shape(
                "concept:while",
                [
                    _gamma_shape("concept:gt", [{}, {}]),
                    _gamma_shape(
                        "concept:assign",
                        [{}, _gamma_shape("concept:sub", [{}, {}])],
                    ),
                ],
            ),
            {},
        ],
    )


def test_bind_lift_strips_source_location_keys_from_bind_payload() -> None:
    result = lift_source("def add(x, y):\n    return x + y\n", "/tmp/work/pkg/add.py")

    assert result.diagnostics == []
    _assert_absent_keys(result.ir, {"file", "fn_line", "line", "column", "col"})


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
    for entry, (concept_name, op_cid) in zip(result.ir, expected.values(), strict=True):
        atoms = _operator_atoms(entry["term_shape"])
        assert atoms[0]["concept_name"] == concept_name
        assert atoms[0]["op_cid"] == op_cid
        assert set(atoms[0]) == {"args", "concept_name", "op_cid"}
        assert all(arg == {} for arg in atoms[0]["args"])
        _assert_absent_keys(atoms[0], {"kind", "op", "file", "fn_line", "line", "column"})


def test_bind_lift_operator_atoms_make_distinct_term_shape_cids() -> None:
    add = lift_source("def f(x, y):\n    return x + y\n", "pkg/add.py").ir[0]
    sub = lift_source("def f(x, y):\n    return x - y\n", "pkg/sub.py").ir[0]

    assert add["term_shape_cid"] != sub["term_shape_cid"]


@pytest.mark.parametrize(
    ("expr", "expected"),
    [
        ("a < b", ["concept:lt"]),
        ("a == b", ["concept:eq"]),
        ("a >= b", ["concept:ge"]),
    ],
)
def test_bind_lift_compare_single_op_discriminates_concept_atom(
    expr: str,
    expected: list[str],
) -> None:
    result = lift_source(f"def f(a, b):\n    return {expr}\n", "pkg/compare_single.py")

    assert result.diagnostics == []
    assert _operator_concepts(result.ir[0]["term_shape"]) == expected


@pytest.mark.parametrize(
    ("expr", "expected"),
    [
        ("a < b < c", ["concept:ite", "concept:lt", "concept:lt"]),
        ("a < b >= c", ["concept:ite", "concept:lt", "concept:ge"]),
        ("a == b != c", ["concept:ite", "concept:eq", "concept:ne"]),
    ],
)
def test_bind_lift_compare_two_chain_desugars_to_and_composition(
    expr: str,
    expected: list[str],
) -> None:
    result = lift_source(f"def f(a, b, c):\n    return {expr}\n", "pkg/compare_two.py")

    assert result.diagnostics == []
    assert _operator_concepts(result.ir[0]["term_shape"]) == expected


@pytest.mark.parametrize(
    ("expr", "expected"),
    [
        ("a < b <= c != d", ["concept:ite", "concept:ite", "concept:lt", "concept:le", "concept:ne"]),
        ("a > b >= c == d", ["concept:ite", "concept:ite", "concept:gt", "concept:ge", "concept:eq"]),
        ("a != b < c > d", ["concept:ite", "concept:ite", "concept:ne", "concept:lt", "concept:gt"]),
    ],
)
def test_bind_lift_compare_three_chain_mixed_ops_desugars_to_and_composition(
    expr: str,
    expected: list[str],
) -> None:
    result = lift_source(f"def f(a, b, c, d):\n    return {expr}\n", "pkg/compare_three.py")

    assert result.diagnostics == []
    assert _operator_concepts(result.ir[0]["term_shape"]) == expected


def test_bind_lift_line_comments_as_concept_comment_terms() -> None:
    source = (
        "def f(value):\n"
        "    # first line comment\n"
        "    value = value + 1\n"
        "    # second line comment\n"
        "    # third line comment\n"
        "    return value\n"
    )

    result = lift_source(source, "pkg/comment_lines.py")

    assert result.diagnostics == []
    term_shape = result.ir[0]["term_shape"]
    assert _operator_concepts(term_shape).count("concept:comment") == 3
    assert _concept_comment_surfaces(term_shape) == [
        "first line comment",
        "second line comment",
        "third line comment",
    ]


def test_bind_lift_concept_comment_excludes_comment_carriers() -> None:
    source = (
        "def f(value):\n"
        "    # provekit:concept:skip\n"
        "    # provekit-concept: {}\n"
        "    # provekit-concept-payload-cid: blake3-512:dead\n"
        "    # ordinary comment\n"
        "    return value\n"
    )

    result = lift_source(source, "pkg/comment_carriers.py")

    assert _concept_comment_surfaces(result.ir[0]["term_shape"]) == ["ordinary comment"]


def test_rust_comment_surface_survives_python_comment_hop() -> None:
    surface = "// byte exact route"
    result = emit_stub(
        function="comment_hop",
        params=[],
        param_types=[],
        return_type="()",
        concept_name="concept:comment",
        term_shape=_gamma_shape("concept:comment", [{"kind": "literal", "value": surface}]),
    )

    lifted = lift_source(result["source"], "pkg/comment_hop.py")

    assert lifted.diagnostics == []
    assert _concept_comment_surfaces(lifted.ir[0]["term_shape"]) == [surface]


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

    _assert_absent_keys(result.ir, {"concept_annotation", "fn_name"})
    assert result.ir[0]["param_names"] == ["x"]
    assert "param_types" not in result.ir[0]
    assert "return_type" not in result.ir[0]
    assert result.ir[0]["term_shape"] == _gamma_shape("concept:add", [{}, {}])
    assert "UNNAMED-CONCEPT" not in json.dumps(result.ir, sort_keys=True)
    assert "return_type" not in result.ir[1]


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
    assert "concept_annotation" not in response["result"]["ir"][0]
    assert "fn_name" not in response["result"]["ir"][0]


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


def test_bind_lift_omits_concept_citations_from_wire_payload() -> None:
    args = [{"kind": "var", "name": "x"}]
    concept_skip_cid = _catalog_concept_cid("concept:skip")
    emitted = emit_stub(
        function="transport_skip",
        params=["x"],
        param_types=["object"],
        return_type="()",
        concept_name="missing-python-skip-carrier",
        transported_op={
            "args_jcs": args,
            "concept_cid": concept_skip_cid,
            "concept_name": "concept:skip",
            "concept_site_cid": _cid("a"),
            "loss_record_cid": _cid("c"),
            "operation_kind": "skip",
            "policy_cid": _cid("d"),
            "shape_cid": concept_skip_cid,
            "sugar_dict_cid": _cid("e"),
            "term_position": [0],
        },
    )

    result = lift_source(emitted["source"], "pkg/foo.py")

    assert result.diagnostics == []
    assert "concept_citations" not in result.ir[0]
    assert result.ir[0]["witnesses"] == []


def test_concept_citation_payload_cid_mismatch_refuses() -> None:
    payload, _payload_cid = _concept_citation_payload()
    source = _concept_comment_lines(payload, _cid("8")) + "def f(x: object):\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert "concept_citations" not in result.ir[0]
    assert "concept-citation:payload-cid-mismatch" in _concept_diagnostics(result)


def test_concept_citation_args_cid_mismatch_refuses() -> None:
    payload, payload_cid = _concept_citation_payload({"args_jcs_cid": _cid("8")})
    source = _concept_comment_lines(payload, payload_cid) + "def f(x: object):\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert "concept_citations" not in result.ir[0]
    assert "concept-citation:args-cid-mismatch" in _concept_diagnostics(result)


def test_concept_citation_unknown_schema_version_refuses() -> None:
    payload, payload_cid = _concept_citation_payload({"schema_version": "999"})
    source = _concept_comment_lines(payload, payload_cid) + "def f(x: object):\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert "concept_citations" not in result.ir[0]
    assert "concept-citation:unknown-schema-version" in _concept_diagnostics(result)


def test_concept_citation_orphan_payload_cid_line_refuses() -> None:
    source = "# provekit-concept-payload-cid: " + _cid("8") + "\ndef f():\n    pass\n"

    result = lift_source(source, "pkg/foo.py")

    assert "concept_citations" not in result.ir[0]
    assert "concept-citation:orphan-cid-line" in _concept_diagnostics(result)


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

    # bad_fn must not produce an IR entry; good_fn must still produce one.
    assert len(result.ir) == 1
    assert "fn_name" not in result.ir[0]
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

    assert len(result.ir) == 1
    assert "fn_name" not in result.ir[0]
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

    # Drop and continue: bad_fn still gets an IR entry, just with no citation.
    assert "concept_citations" not in result.ir[1]
    assert len(result.ir) == 2
    assert all("fn_name" not in entry for entry in result.ir)
    assert "concept-citation:malformed-json" in _concept_diagnostics(result)
