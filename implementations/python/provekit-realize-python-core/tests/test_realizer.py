from __future__ import annotations

import json
import sys
from pathlib import Path

import blake3

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

import provekit_realize_python_core.realizer as realizer
from provekit_realize_python_core.realizer import (
    BodyTemplateEntry,
    MissingTemplateError,
    emit_stub,
)


def _cid(ch: str) -> str:
    return "blake3-512:" + ch * 128


def _json_cid(value: object) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return "blake3-512:" + blake3.blake3(encoded).digest(length=64).hex()


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


def _contract_payload() -> dict:
    return {
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
            },
            {
                "role": "post",
                "predicate": _formula_out_eq_x(),
                "predicate_text": "out == x",
                "source_kind": "native-surface",
            },
        ],
    }


def _contract_comment_payloads(source: str) -> list[dict]:
    payloads: list[dict] = []
    for line in source.splitlines():
        stripped = line.strip()
        if stripped.startswith("# provekit-contract: "):
            payloads.append(json.loads(stripped.removeprefix("# provekit-contract: ")))
    return payloads


def _concept_citation_payloads(source: str) -> list[dict]:
    payloads: list[dict] = []
    for line in source.splitlines():
        stripped = line.strip()
        if stripped.startswith("# provekit-concept: "):
            payloads.append(json.loads(stripped.removeprefix("# provekit-concept: ")))
    return payloads


def _transported_op_payload() -> dict:
    return {
        "args_jcs": [{"kind": "var", "name": "x"}],
        "callsite_cid": _cid("0"),
        "concept_cid": _cid("a"),
        "concept_name": "concept:drop",
        "concept_site_cid": _cid("b"),
        "loss_record_cid": _cid("c"),
        "operation_kind": "drop",
        "policy_cid": _cid("d"),
        "shape_cid": _cid("e"),
        "sugar_dict_cid": _cid("f"),
        "term_position": [3, 0],
    }


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


def test_unknown_concept_refuses_missing_body_template() -> None:
    try:
        emit_stub("missing", ["x"], ["int"], "int", "missing-concept")
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "missing-concept",
                "args_shape": ["int"],
                "function": "missing",
                "term_position": "body",
            }
        ]
    else:
        raise AssertionError("missing body-template should refuse")


def test_method_term_surface_renders_python_method_call() -> None:
    result = emit_stub(
        function="trim",
        params=["value"],
        param_types=["str"],
        return_type="str",
        concept_name="return(method:strip(value, []))",
    )

    assert result == {
        "source": "def trim(value):\n    return value.strip()\n",
        "is_stub": False,
        "extension": "py",
    }


def test_method_term_surface_uses_libprovekit_body_template_binding() -> None:
    result = emit_stub(
        function="value_string",
        params=["text"],
        param_types=["str"],
        return_type="Value",
        concept_name="method:string(Value, [text])",
    )

    assert result == {
        "source": "def value_string(text):\n    return Value.string(text)\n",
        "is_stub": False,
        "extension": "py",
    }


def test_rust_runtime_concepts_render_from_body_template_catalog() -> None:
    cases = [
        (
            "identity_new",
            ["value"],
            ["Value"],
            "Arc<Value>",
            "concept:new",
            "def identity_new(value):\n    return value\n",
        ),
        (
            "literal_return",
            ["value"],
            ["Value"],
            "Value",
            "concept:return",
            "def literal_return(value):\n    return value\n",
        ),
        (
            "reserve_string",
            ["capacity"],
            ["usize"],
            "String",
            "concept:string-with-capacity",
            "def reserve_string(capacity):\n    return \"\"\n",
        ),
        (
            "append_string",
            ["target", "suffix"],
            ["String", "&str"],
            "String",
            "concept:string-push-str",
            "def append_string(target, suffix):\n    return target + suffix\n",
        ),
        (
            "repeat_array",
            ["value", "count"],
            ["Value", "usize"],
            "list[Value]",
            "concept:array-repeat",
            "def repeat_array(value, count):\n    return [value] * count\n",
        ),
        (
            "string_len",
            ["value"],
            ["String"],
            "usize",
            "concept:str-len",
            "def string_len(value):\n    return len(value)\n",
        ),
        (
            "method_len",
            ["value"],
            ["String"],
            "usize",
            "concept:method-len",
            "def method_len(value):\n    return len(value)\n",
        ),
        (
            "checked_add",
            ["left", "right"],
            ["i64", "u32"],
            "i64",
            "concept:add",
            "def checked_add(left, right):\n    return left + right\n",
        ),
        (
            "borrow_value",
            ["value"],
            ["Value"],
            "&Value",
            "concept:borrow",
            "def borrow_value(value):\n    return value\n",
        ),
    ]

    for function, params, param_types, return_type, concept_name, expected in cases:
        result = emit_stub(function, params, param_types, return_type, concept_name)
        assert result["source"] == expected
        assert result["is_stub"] is False
        assert result["extension"] == "py"


def test_call_term_surface_renders_python_call() -> None:
    result = emit_stub(
        function="reserve_text",
        params=["capacity"],
        param_types=["int"],
        return_type="str",
        concept_name="return(call:String::with_capacity(capacity))",
    )

    assert result == {
        "source": "def reserve_text(capacity):\n    return \"\"\n",
        "is_stub": False,
        "extension": "py",
    }


def test_rust_runtime_constructor_call_term_surface_uses_body_template() -> None:
    result = emit_stub(
        function="null",
        params=[],
        param_types=[],
        return_type="Arc<Value>",
        concept_name="return(call:new(Arc::new, [Null]))",
    )

    assert result == {
        "source": "def null():\n    return Null\n",
        "is_stub": False,
        "extension": "py",
    }


def test_blake3_512_term_surface_lowers_to_byte_correct_python() -> None:
    term_surface = (
        "let(pattern_bind(hasher), call:new(blake3::Hasher::new, []), "
        "let(pattern_bind(hasher_v1), method:update(hasher, [bytes]), "
        "let(pattern_bind(out), array_repeat(0, 64), "
        "let(pattern_bind(hasher_v2), method:fill(method:finalize_xof(hasher_v1, []), [out]), "
        "let(pattern_bind(out_v1), hasher_v2, "
        "let(pattern_bind(hex), call:encode(hex::encode, [out_v1]), "
        "let(pattern_bind(s), call:with_capacity(String::with_capacity, [add(method:len(BLAKE3_512_PREFIX, []), method:len(hex, []))]), "
        "let(pattern_bind(s_v1), method:push_str(s, [BLAKE3_512_PREFIX]), "
        "let(pattern_bind(s_v2), method:push_str(s_v1, [borrow(hex)]), "
        "return(s_v2))))))))))"
    )

    result = emit_stub(
        function="blake3_512_of",
        params=["bytes"],
        param_types=["bytes"],
        return_type="String",
        concept_name=term_surface,
    )

    assert result == {
        "source": (
            "def blake3_512_of(bytes):\n"
            "    hasher = blake3.blake3()\n"
            "    hasher_v1 = (hasher.update(bytes) or hasher)\n"
            "    out = [0] * 64\n"
            "    hasher_v2 = hasher_v1.digest(length=64)\n"
            "    out_v1 = hasher_v2\n"
            "    hex = out_v1.hex()\n"
            "    s = \"\"\n"
            "    s_v1 = s + BLAKE3_512_PREFIX\n"
            "    s_v2 = s_v1 + hex\n"
            "    return s_v2\n"
        ),
        "is_stub": False,
        "extension": "py",
    }


def test_unsupported_qualified_call_term_surface_refuses_missing_template() -> None:
    try:
        emit_stub(
            function="unknown_call",
            params=["x"],
            param_types=["int"],
            return_type="int",
            concept_name="return(call:Widget::build(x))",
        )
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "call:Widget::build",
                "args_shape": ["int"],
                "function": "unknown_call",
                "term_position": "body.return.call:Widget::build",
            }
        ]
    else:
        raise AssertionError("unsupported qualified call should refuse")


def test_term_surface_collects_all_missing_templates() -> None:
    term_surface = (
        "let(pattern_bind(a), call:Widget::build(x), "
        "let(pattern_bind(b), mystery_op(call:Gadget::make(x), x), return(x)))"
    )

    try:
        emit_stub(
            function="both_missing",
            params=["x"],
            param_types=["int"],
            return_type="int",
            concept_name=term_surface,
        )
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "call:Widget::build",
                "args_shape": ["int"],
                "function": "both_missing",
                "term_position": "body.let.rhs.call:Widget::build",
            },
            {
                "operation_kind": "mystery_op",
                "args_shape": ["call:Gadget::make", "int"],
                "function": "both_missing",
                "term_position": "body.let.cont.let.rhs.mystery_op",
            },
            {
                "operation_kind": "call:Gadget::make",
                "args_shape": ["int"],
                "function": "both_missing",
                "term_position": "body.let.cont.let.rhs.mystery_op.args[0].call:Gadget::make",
            },
        ]
    else:
        raise AssertionError("all missing body-templates should be collected")


def test_let_term_surface_renders_python_assignment_with_type_ascription() -> None:
    result = emit_stub(
        function="bind_value",
        params=["source"],
        param_types=["int"],
        return_type="None",
        concept_name="let(result: i64, call:identity(source))",
    )

    assert result == {
        "source": "def bind_value(source):\n    result: int = identity(source)\n",
        "is_stub": False,
        "extension": "py",
    }


def test_let_continuation_term_surface_flattens_python_statements() -> None:
    result = emit_stub(
        function="bind_then_return",
        params=["source"],
        param_types=["int"],
        return_type="int",
        concept_name=(
            "let(pattern_bind(a), call:identity(source), "
            "let(pattern_bind(b), call:identity(a), return(b)))"
        ),
    )

    assert result == {
        "source": (
            "def bind_then_return(source):\n"
            "    a = identity(source)\n"
            "    b = identity(a)\n"
            "    return b\n"
        ),
        "is_stub": False,
        "extension": "py",
    }


def test_let_continuation_term_surface_omits_skip_tail() -> None:
    result = emit_stub(
        function="bind_then_skip",
        params=["source"],
        param_types=["int"],
        return_type="None",
        concept_name="let(pattern_bind(result), call:identity(source), skip)",
    )

    assert result == {
        "source": "def bind_then_skip(source):\n    result = identity(source)\n",
        "is_stub": False,
        "extension": "py",
    }


def test_named_term_tree_walk_emits_composed_body(monkeypatch) -> None:
    existing_entries = realizer.entries()
    monkeypatch.setattr(
        realizer,
        "entries",
        lambda: (
            *existing_entries,
            BodyTemplateEntry(
                concept_name="concept:call",
                template_kind="verbatim",
                template="return ${param0}",
                min_params=1,
                max_params=1,
                requires_param_types=None,
                requires_return_type=None,
            ),
        ),
    )
    tree = {
        "conceptName": "concept:seq",
        "operationKind": "seq",
        "shapeCid": "blake3-512:seq",
        "args": [
            {
                "conceptName": "concept:call",
                "operationKind": "call",
                "shapeCid": "blake3-512:call",
                "args": [],
            },
            {
                "conceptName": "concept:return",
                "operationKind": "return",
                "shapeCid": "blake3-512:return",
                "args": [],
            },
        ],
    }

    result = emit_stub(
        function="compose_tree",
        params=["value"],
        param_types=["int"],
        return_type="int",
        concept_name="UNNAMED-CONCEPT-1",
        named_term_tree=tree,
    )

    assert result == {
        "source": "def compose_tree(value):\n    return value\n    return value\n",
        "is_stub": False,
        "extension": "py",
    }
    assert "NotImplementedError" not in result["source"]


def test_named_term_tree_missing_node_concept_refuses_loudly() -> None:
    tree = {
        "conceptName": "concept:seq",
        "operationKind": "seq",
        "shapeCid": "blake3-512:seq",
        "args": [
            {
                "conceptName": "missing-node-concept",
                "operationKind": "call",
                "shapeCid": "blake3-512:missing",
                "args": [],
            }
        ],
    }

    try:
        emit_stub(
            function="missing_tree",
            params=["value"],
            param_types=["int"],
            return_type="int",
            concept_name="UNNAMED-CONCEPT-1",
            named_term_tree=tree,
        )
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "missing-node-concept",
                "args_shape": ["int"],
                "function": "missing_tree",
                "term_position": "body.namedTermTree.args[0]",
            }
        ]
    else:
        raise AssertionError("missing namedTermTree concept should refuse")


def test_contract_witnesses_emit_liftable_contract_comment_payloads() -> None:
    result = emit_stub(
        function="wrap_identity",
        params=["x"],
        param_types=["int"],
        return_type="int",
        concept_name="identity",
        contract=_contract_payload(),
    )

    source = result["source"]
    assert source.index("# provekit-contract:") < source.index("def wrap_identity")
    assert source.count("# provekit-contract-payload-cid: blake3-512:") == 2

    payloads = _contract_comment_payloads(source)
    assert [payload["role"] for payload in payloads] == ["pre", "post"]
    for payload in payloads:
        assert payload["artifact_kind"] == "provekit-contract-comment-sugar"
        assert payload["schema_version"] == "1"
        assert payload["concept_site_cid"] == _cid("1")
        assert payload["contract_cid"] == _cid("2")
        assert payload["local_contract_cid"] == _cid("2")
        assert payload["emitted_by"]["kit_kind"] == "realize"
        assert payload["emitted_by"]["target_language"] == "python"
        assert payload["ir_formula_jcs_cid"].startswith("blake3-512:")
        assert payload["loss_record_cid"].startswith("blake3-512:")
        assert payload["policy_cid"].startswith("blake3-512:")
        assert payload["sugar_dict_cid"].startswith("blake3-512:")
        assert isinstance(payload["ir_formula_jcs"], dict)


def test_concept_citation_comment_emitted_for_transported_operation() -> None:
    transported_op = _transported_op_payload()

    result = emit_stub(
        function="transport_drop",
        params=["x"],
        param_types=["object"],
        return_type="()",
        concept_name="missing-python-drop-surface",
        transported_op=transported_op,
    )

    source = result["source"]
    assert "# provekit-concept:" in source
    assert "# provekit-concept-payload-cid: blake3-512:" in source
    assert "    pass\n" in source
    assert "NotImplementedError" not in source
    assert source.index("# provekit-concept:") < source.index("    pass")

    payloads = _concept_citation_payloads(source)
    assert len(payloads) == 1
    payload = payloads[0]
    assert payload["artifact_kind"] == "provekit-concept-citation-comment-sugar"
    assert payload["schema_version"] == "1"
    assert payload["args_jcs"] == transported_op["args_jcs"]
    assert payload["args_jcs_cid"] == _json_cid(transported_op["args_jcs"])
    assert payload["callsite_cid"] == transported_op["callsite_cid"]
    assert payload["concept_cid"] == transported_op["concept_cid"]
    assert payload["concept_name"] == transported_op["concept_name"]
    assert payload["concept_site_cid"] == transported_op["concept_site_cid"]
    assert payload["loss_record_cid"] == transported_op["loss_record_cid"]
    assert payload["operation_kind"] == transported_op["operation_kind"]
    assert payload["policy_cid"] == transported_op["policy_cid"]
    assert payload["shape_cid"] == transported_op["shape_cid"]
    assert payload["sugar_dict_cid"] == transported_op["sugar_dict_cid"]
    assert payload["term_position"] == transported_op["term_position"]
    assert payload["emitted_by"]["kit_cid"].startswith("blake3-512:")
    assert payload["emitted_by"]["kit_id"] == "provekit-realize-python-core@0.1.0"
    assert payload["emitted_by"]["kit_kind"] == "realize"
    assert payload["emitted_by"]["target_language"] == "python"
    assert payload["emitted_by"]["target_library_tag"] == "python"

    payload_cid = _json_cid(payload)
    assert f"# provekit-concept-payload-cid: {payload_cid}" in source
