from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

import blake3
import pytest

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


def _compiled_namespace(source: str) -> dict[str, object]:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "rendered.py"
        path.write_text(source, encoding="utf-8")
        subprocess.run(
            [sys.executable, "-m", "py_compile", str(path)],
            check=True,
            capture_output=True,
            text=True,
        )
    namespace: dict[str, object] = {}
    exec(source, namespace)
    return namespace


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


def _loss_record_contribution(loss_name: str) -> dict:
    return {
        "loss_record_contribution": {
            "form": "literal",
            "value": {
                loss_name: {
                    "args": [],
                    "head": "atomic",
                    "name": loss_name,
                }
            },
        }
    }


def _shape(concept_name: str, args: list[dict] | None = None) -> dict:
    return {"concept_name": concept_name, "args": args or []}


def _literal_shape(value: object) -> dict:
    return {"concept_name": "concept:literal", "args": [], "value": value}


def _safe_divide_then_double_shape() -> dict:
    return _shape(
        "concept:conditional",
        [
            _shape("concept:eq", [{}, {}]),
            _shape("concept:neg", [{}]),
            _shape(
                "concept:seq",
                [
                    _shape("concept:div", [{}, {}]),
                    _shape(
                        "concept:conditional",
                        [
                            _shape("concept:lt", [{}, {}]),
                            _shape("concept:neg", [{}]),
                            _shape("concept:mul", [{}, {}]),
                        ],
                    ),
                ],
            ),
        ],
    )


def _safe_divide_then_double_bindings() -> list[dict]:
    return [
        {"position": [0, 0], "symbol": "denom"},
        {"position": [0, 1], "symbol": "0"},
        {"position": [1, 0], "symbol": "1"},
        {"position": [2, 0, 0], "symbol": "num"},
        {"position": [2, 0, 1], "symbol": "denom"},
        {"position": [2, 1, 0, 0], "symbol": "q"},
        {"position": [2, 1, 0, 1], "symbol": "0"},
        {"position": [2, 1, 1, 0], "symbol": "1"},
        {"position": [2, 1, 2, 0], "symbol": "q"},
        {"position": [2, 1, 2, 1], "symbol": "2"},
    ]


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
            "def checked_add(left, right):\n    return (left) + (right)\n",
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


def test_concept_conditional_body_template_renders_valid_python_and_executes() -> None:
    result = emit_stub(
        function="pick",
        params=["cond", "then_value", "else_value"],
        param_types=["bool", "int", "int"],
        return_type="int",
        concept_name="concept:conditional",
    )

    assert result["source"] == (
        "def pick(cond, then_value, else_value):\n"
        "    return (then_value) if (cond) else (else_value)\n"
    )
    namespace = _compiled_namespace(result["source"])
    pick = namespace["pick"]
    assert pick(True, 11, 22) == 11
    assert pick(False, 11, 22) == 22


def test_concept_eq_body_template_renders_valid_python_and_executes() -> None:
    result = emit_stub(
        function="same",
        params=["left", "right"],
        param_types=["int", "int"],
        return_type="bool",
        concept_name="concept:eq",
    )

    assert result["source"] == "def same(left, right):\n    return (left) == (right)\n"
    namespace = _compiled_namespace(result["source"])
    same = namespace["same"]
    assert same(3, 3) is True
    assert same(3, 4) is False


def test_concept_decl_body_template_renders_valid_python_and_executes() -> None:
    body = realizer.body_template_for(
        "concept:decl",
        ["result", "value"],
        ["str", "int"],
        "None",
    )
    assert body == "result = value"
    source = f"def assign(value):\n    {body}\n    return result\n"

    namespace = _compiled_namespace(source)
    assign = namespace["assign"]
    assert assign(42) == 42


def test_concept_decl_refuses_expression_position() -> None:
    try:
        emit_stub(
            function="bad_decl_expr",
            params=["value"],
            param_types=["int"],
            return_type="int",
            concept_name="return(decl(result, value))",
        )
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "concept:decl",
                "args_shape": ["expr", "int"],
                "function": "bad_decl_expr",
                "term_position": "body.return.decl",
            }
        ]
    else:
        raise AssertionError("decl in expression position should refuse")


def test_concept_lt_body_template_renders_valid_python_and_executes() -> None:
    result = emit_stub(
        function="less_than",
        params=["left", "right"],
        param_types=["int", "int"],
        return_type="bool",
        concept_name="concept:lt",
    )

    assert result["source"] == "def less_than(left, right):\n    return (left) < (right)\n"
    namespace = _compiled_namespace(result["source"])
    less_than = namespace["less_than"]
    assert less_than(2, 5) is True
    assert less_than(5, 2) is False


def test_concept_mul_body_template_renders_valid_python_and_executes() -> None:
    result = emit_stub(
        function="product",
        params=["left", "right"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="concept:mul",
    )

    assert result["source"] == "def product(left, right):\n    return (left) * (right)\n"
    namespace = _compiled_namespace(result["source"])
    product = namespace["product"]
    assert product(6, 7) == 42


def test_a10_operator_body_templates_render_valid_python_and_execute() -> None:
    cases = [
        ("concept:add", ["left", "right"], ["i64", "i64"], "i64", "return (left) + (right)", (7, 3), 10),
        ("concept:sub", ["left", "right"], ["i64", "i64"], "i64", "return (left) - (right)", (7, 3), 4),
        ("concept:mul", ["left", "right"], ["i64", "i64"], "i64", "return (left) * (right)", (7, 3), 21),
        ("concept:div", ["left", "right"], ["i64", "i64"], "i64", "return (left) // (right)", (7, 3), 2),
        ("concept:eq", ["left", "right"], ["i64", "i64"], "bool", "return (left) == (right)", (7, 7), True),
        ("concept:ne", ["left", "right"], ["i64", "i64"], "bool", "return (left) != (right)", (7, 3), True),
        ("concept:lt", ["left", "right"], ["i64", "i64"], "bool", "return (left) < (right)", (3, 7), True),
        ("concept:le", ["left", "right"], ["i64", "i64"], "bool", "return (left) <= (right)", (7, 7), True),
        ("concept:gt", ["left", "right"], ["i64", "i64"], "bool", "return (left) > (right)", (7, 3), True),
        ("concept:ge", ["left", "right"], ["i64", "i64"], "bool", "return (left) >= (right)", (7, 7), True),
        ("concept:and", ["left", "right"], ["bool", "bool"], "bool", "return (left) and (right)", (True, False), False),
        ("concept:or", ["left", "right"], ["bool", "bool"], "bool", "return (left) or (right)", (True, False), True),
        ("concept:not", ["value"], ["bool"], "bool", "return not (value)", (False,), True),
        ("concept:mod", ["left", "right"], ["i64", "i64"], "i64", "return (left) % (right)", (7, 3), 1),
        ("concept:shl", ["left", "right"], ["i64", "i64"], "i64", "return (left) << (right)", (3, 2), 12),
        ("concept:shr", ["left", "right"], ["i64", "i64"], "i64", "return (left) >> (right)", (8, 1), 4),
        ("concept:bitand", ["left", "right"], ["i64", "i64"], "i64", "return (left) & (right)", (6, 3), 2),
        ("concept:bitor", ["left", "right"], ["i64", "i64"], "i64", "return (left) | (right)", (4, 1), 5),
        ("concept:bitxor", ["left", "right"], ["i64", "i64"], "i64", "return (left) ^ (right)", (6, 3), 5),
        ("concept:neg", ["value"], ["i64"], "i64", "return -(value)", (7,), -7),
        ("concept:bitnot", ["value"], ["i64"], "i64", "return ~(value)", (7,), -8),
    ]

    for index, (concept_name, params, param_types, return_type, body, args, expected) in enumerate(cases):
        function = f"op_{index}"
        result = emit_stub(
            function=function,
            params=params,
            param_types=param_types,
            return_type=return_type,
            concept_name=concept_name,
        )

        assert result["source"] == f"def {function}({', '.join(params)}):\n    {body}\n"
        namespace = _compiled_namespace(result["source"])
        assert namespace[function](*args) == expected


def test_term_shape_sidecar_renders_safe_divide_then_double_source_symbols() -> None:
    result = emit_stub(
        function="",
        params=["num", "denom"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="result-bind",
        term_shape=_safe_divide_then_double_shape(),
        operand_bindings=_safe_divide_then_double_bindings(),
        source_function_name="safe_divide_then_double",
    )

    source = result["source"]
    assert source.startswith("def safe_divide_then_double(num, denom):\n")
    assert "if (denom) == (0):" in source
    assert "q = (num) // (denom)" in source
    assert "if (q) < (0):" in source
    assert "return (q) * (2)" in source
    namespace = _compiled_namespace(source)
    fn = namespace["safe_divide_then_double"]
    assert fn(8, 2) == 8
    assert fn(8, 0) == -1
    assert fn(-5, 2) == -1


def test_empty_term_shape_uses_root_operand_binding_literal() -> None:
    result = emit_stub(
        function="",
        params=[],
        param_types=[],
        return_type="str",
        concept_name="UNNAMED-CONCEPT-1",
        term_shape={},
        operand_bindings=[{"position": [], "symbol": '"hello"'}],
        source_function_name="hello",
    )

    assert result["source"] == "def hello():\n    return \"hello\"\n"
    namespace = _compiled_namespace(result["source"])
    assert namespace["hello"]() == "hello"


def test_term_shape_sidecar_wins_over_lossy_named_tree_for_nested_unary_ops() -> None:
    lossy_tree = {
        "conceptName": "concept:seq",
        "operationKind": "seq",
        "shapeCid": _cid("a"),
        "args": [
            {
                "conceptName": "concept:neg",
                "operationKind": "op-application",
                "shapeCid": _cid("b"),
                "args": [],
            }
        ],
    }

    result = emit_stub(
        function="",
        params=["num", "denom"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="result-bind",
        named_term_tree=lossy_tree,
        term_shape=_safe_divide_then_double_shape(),
        operand_bindings=_safe_divide_then_double_bindings(),
        source_function_name="safe_divide_then_double",
    )

    source = result["source"]
    assert "return -(1)" in source
    namespace = _compiled_namespace(source)
    assert namespace["safe_divide_then_double"](8, 2) == 8


def test_term_shape_comment_emits_python_comment_surface() -> None:
    result = emit_stub(
        function="comment_only",
        params=[],
        param_types=[],
        return_type="()",
        concept_name="concept:comment",
        term_shape=_shape(
            "concept:comment",
            [{"kind": "literal", "value": "// keep me exactly"}],
        ),
    )

    assert result["source"] == (
        "def comment_only():\n"
        "    # // keep me exactly\n"
        "    pass\n"
    )


def test_term_shape_operand_bindings_discriminate_symbol_order() -> None:
    shape = _shape("concept:div", [{}, {}])
    left = emit_stub(
        function="divide_left",
        params=["num", "denom"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="concept:div",
        term_shape=shape,
        operand_bindings=[
            {"position": [0], "symbol": "num"},
            {"position": [1], "symbol": "denom"},
        ],
    )["source"]
    right = emit_stub(
        function="divide_right",
        params=["num", "denom"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="concept:div",
        term_shape=shape,
        operand_bindings=[
            {"position": [0], "symbol": "denom"},
            {"position": [1], "symbol": "num"},
        ],
    )["source"]

    assert "return (num) // (denom)" in left
    assert "return (denom) // (num)" in right
    assert left != right


def test_term_shape_operand_bindings_resolve_nested_positions() -> None:
    result = emit_stub(
        function="nested",
        params=["a", "b", "c"],
        param_types=["int", "int", "int"],
        return_type="int",
        concept_name="concept:add",
        term_shape=_shape(
            "concept:add",
            [_shape("concept:mul", [{}, {}]), {}],
        ),
        operand_bindings=[
            {"position": [0, 0], "symbol": "a"},
            {"position": [0, 1], "symbol": "b"},
            {"position": [1], "symbol": "c"},
        ],
    )

    assert result["source"] == (
        "def nested(a, b, c):\n"
        "    return ((a) * (b)) + (c)\n"
    )


def test_term_shape_operand_bindings_allow_concept_literal_positions() -> None:
    result = emit_stub(
        function="add_literal",
        params=["x"],
        param_types=["int"],
        return_type="int",
        concept_name="concept:add",
        term_shape=_shape("concept:add", [{}, _literal_shape(2)]),
        operand_bindings=[
            {"position": [0], "symbol": "x"},
            {"position": [1], "symbol": "2"},
        ],
    )

    assert result["source"] == "def add_literal(x):\n    return (x) + (2)\n"


def test_term_shape_concept_literal_does_not_require_operand_binding() -> None:
    result = emit_stub(
        function="add_literal",
        params=["x"],
        param_types=["int"],
        return_type="int",
        concept_name="concept:add",
        term_shape=_shape("concept:add", [{}, _literal_shape(2)]),
        operand_bindings=[{"position": [0], "symbol": "x"}],
    )

    assert result["source"] == "def add_literal(x):\n    return (x) + (2)\n"


def test_term_shape_assignment_sequence_preserves_named_temps_and_return() -> None:
    result = emit_stub(
        function="compute_sum",
        params=["a", "b"],
        param_types=["int", "int"],
        return_type="int",
        concept_name="concept:seq",
        term_shape=_shape(
            "concept:seq",
            [
                _shape("concept:assign", [{}, _shape("concept:add", [{}, {}])]),
                _shape("concept:assign", [{}, _shape("concept:mul", [{}, _literal_shape(2)])]),
                _shape("concept:assign", [{}, _shape("concept:sub", [{}, _literal_shape(1)])]),
                _shape("concept:return", [{}]),
            ],
        ),
        operand_bindings=[
            {"position": [0, 0], "symbol": "total"},
            {"position": [0, 1, 0], "symbol": "a"},
            {"position": [0, 1, 1], "symbol": "b"},
            {"position": [1, 0], "symbol": "scaled"},
            {"position": [1, 1, 0], "symbol": "total"},
            {"position": [1, 1, 1], "symbol": "2"},
            {"position": [2, 0], "symbol": "reduced"},
            {"position": [2, 1, 0], "symbol": "scaled"},
            {"position": [2, 1, 1], "symbol": "1"},
            {"position": [3, 0], "symbol": "reduced"},
        ],
    )

    assert result["source"] == (
        "def compute_sum(a, b):\n"
        "    total = (a) + (b)\n"
        "    scaled = (total) * (2)\n"
        "    reduced = (scaled) - (1)\n"
        "    return reduced\n"
    )


def test_term_shape_operand_binding_gate_refuses_missing_and_extra_positions() -> None:
    shape = _shape("concept:add", [{}, {}])

    try:
        emit_stub(
            function="missing",
            params=["x", "y"],
            param_types=["int", "int"],
            return_type="int",
            concept_name="concept:add",
            term_shape=shape,
            operand_bindings=[{"position": [0], "symbol": "x"}],
        )
    except realizer.OperandBindingMisalignmentError as exc:
        assert exc.missing_positions == [[1]]
        assert exc.extra_positions == []
        assert "missing_positions=[[1]]" in str(exc)
    else:
        raise AssertionError("missing operand binding should refuse")

    try:
        emit_stub(
            function="extra",
            params=["x", "y"],
            param_types=["int", "int"],
            return_type="int",
            concept_name="concept:add",
            term_shape=shape,
            operand_bindings=[
                {"position": [0], "symbol": "x"},
                {"position": [1], "symbol": "y"},
                {"position": [2], "symbol": "z"},
            ],
        )
    except realizer.OperandBindingMisalignmentError as exc:
        assert exc.missing_positions == []
        assert exc.extra_positions == [[2]]
        assert "extra_positions=[[2]]" in str(exc)
    else:
        raise AssertionError("extra operand binding should refuse")


def test_a10_operator_templates_discriminate_distinct_ops() -> None:
    add = emit_stub(
        function="calc_add",
        params=["left", "right"],
        param_types=["i64", "i64"],
        return_type="i64",
        concept_name="concept:add",
    )["source"]
    sub = emit_stub(
        function="calc_sub",
        params=["left", "right"],
        param_types=["i64", "i64"],
        return_type="i64",
        concept_name="concept:sub",
    )["source"]

    assert add == "def calc_add(left, right):\n    return (left) + (right)\n"
    assert sub == "def calc_sub(left, right):\n    return (left) - (right)\n"
    assert add != sub


def test_a10_operator_term_surface_composes_nested_templates() -> None:
    result = emit_stub(
        function="composed",
        params=["x", "y", "a", "b"],
        param_types=["i64", "i64", "i64", "i64"],
        return_type="i64",
        concept_name="return(mul(add(x, y), sub(a, b)))",
    )

    assert result["source"] == (
        "def composed(x, y, a, b):\n"
        "    return ((x) + (y)) * ((a) - (b))\n"
    )
    namespace = _compiled_namespace(result["source"])
    assert namespace["composed"](2, 3, 11, 4) == 35


def test_core_concept_term_surface_composes_valid_python() -> None:
    result = emit_stub(
        function="classify_or_double",
        params=["value"],
        param_types=["int"],
        return_type="int",
        concept_name="return(conditional(eq(value, 0), -1, conditional(lt(value, 0), 0, mul(value, 2))))",
    )

    assert result["source"] == (
        "def classify_or_double(value):\n"
        "    return (-1) if ((value) == (0)) else ((0) if ((value) < (0)) else ((value) * (2)))\n"
    )
    namespace = _compiled_namespace(result["source"])
    classify_or_double = namespace["classify_or_double"]
    assert classify_or_double(0) == -1
    assert classify_or_double(-3) == 0
    assert classify_or_double(4) == 8


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


def test_term_shape_synthesis_ignores_function_level_concept_annotation() -> None:
    term_shape = {
        "concept_name": "concept:conditional",
        "op_cid": _cid("a"),
        "args": [
            {
                "concept_name": "concept:eq",
                "op_cid": _cid("b"),
                "args": [
                    {"kind": "var", "name": "x"},
                    {"kind": "const", "value": 0},
                ],
            },
            {
                "concept_name": "concept:neg",
                "op_cid": _cid("c"),
                "args": [{"kind": "const", "value": 1}],
            },
            {
                "concept_name": "concept:add",
                "op_cid": _cid("d"),
                "args": [
                    {"kind": "var", "name": "x"},
                    {"kind": "const", "value": 1},
                ],
            },
        ],
    }

    result = emit_stub(
        function="audit_annotated",
        params=["x"],
        param_types=["int"],
        return_type="int",
        concept_name="concept:foo-bar",
        term_shape=term_shape,
    )

    source = result["source"]
    assert source == (
        "def audit_annotated(x):\n"
        "    if (x) == (0):\n"
        "        return -(1)\n"
        "    else:\n"
        "        return (x) + (1)\n"
    )
    assert "concept:foo-bar" not in source
    assert "missing body-template" not in source
    namespace = _compiled_namespace(source)
    audit_annotated = namespace["audit_annotated"]
    assert audit_annotated(0) == -1
    assert audit_annotated(4) == 5


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


@pytest.mark.parametrize(
    ("concept_name", "loss_name"),
    [
        ("concept:addr", "python-references-not-addresses"),
        ("concept:deref", "python-implicit-deref"),
        ("concept:decl", "python-implicit-decl"),
        ("concept:cast", "python-constructor-not-cast"),
        ("concept:do", "python-no-do-while"),
        ("concept:postdec", "python-no-postfix-decrement"),
        ("concept:postinc", "python-no-postfix-increment"),
        ("concept:predec", "python-no-prefix-decrement"),
        ("concept:preinc", "python-no-prefix-increment"),
        ("concept:ushr", "python-unified-shr"),
    ],
)
def test_python_floor_gap_concepts_emit_named_loss_concept_citation_carriers(
    concept_name: str,
    loss_name: str,
) -> None:
    function = "carry_" + concept_name.removeprefix("concept:").replace("-", "_")

    result = emit_stub(
        function=function,
        params=["x"],
        param_types=["object"],
        return_type="object",
        concept_name=concept_name,
    )

    source = result["source"]
    assert "# provekit-concept:" in source
    assert "# provekit-concept-payload-cid: blake3-512:" in source
    assert "    pass\n" in source
    _compiled_namespace(source)

    payloads = _concept_citation_payloads(source)
    assert len(payloads) == 1
    payload = payloads[0]
    assert payload["artifact_kind"] == "provekit-concept-citation-comment-sugar"
    assert payload["concept_name"] == concept_name
    assert payload["operation_kind"] == concept_name.removeprefix("concept:")
    assert payload["args_jcs"] == [{"kind": "var", "name": "x"}]
    assert payload["args_jcs_cid"] == _json_cid(payload["args_jcs"])
    assert payload["term_position"] == [0]
    assert payload["loss_record_cid"] == _json_cid(_loss_record_contribution(loss_name))
    assert result["observed_loss_record"] == _loss_record_contribution(loss_name)
