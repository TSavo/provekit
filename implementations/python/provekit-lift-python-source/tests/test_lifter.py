from __future__ import annotations

import ast
import json
import subprocess
import sys
from pathlib import Path

import pytest

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
from provekit_lift_python_source.rpc import dispatch, initialize_result

KIT_DECLARATION_RPC_METHOD = "provekit.plugin.kit_declaration"
RUNTIME_FAILURE_EFFECT_LEAF = {
    "surface": "python-source",
    "local": "python:raise",
    "concept": "concept:panic-freedom.leaf.runtime-failure-site",
}
ATTRIBUTE_RUNTIME_FAILURE_EFFECT_LEAF = {
    "surface": "python-source",
    "local": "python:attribute",
    "concept": "concept:panic-freedom.leaf.runtime-failure-site",
}
SUBSCRIPT_RUNTIME_FAILURE_EFFECT_LEAF = {
    "surface": "python-source",
    "local": "python:subscript",
    "concept": "concept:panic-freedom.leaf.runtime-failure-site",
}
RUNTIME_FAILURE_EFFECT_LEAVES = [
    RUNTIME_FAILURE_EFFECT_LEAF,
    ATTRIBUTE_RUNTIME_FAILURE_EFFECT_LEAF,
    SUBSCRIPT_RUNTIME_FAILURE_EFFECT_LEAF,
]
PANIC_FREEDOM_EFFECT_KIND = "concept:panic-freedom"
RUNTIME_FAILURE_SITE_CONCEPT = "concept:panic-freedom.leaf.runtime-failure-site"


def _canon(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _parse_top_level_toml(path: Path) -> dict[str, object]:
    values: dict[str, object] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or line.startswith("[") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        raw_value = value.strip()
        if raw_value == "true":
            values[key.strip()] = True
        elif raw_value == "false":
            values[key.strip()] = False
        else:
            values[key.strip()] = ast.literal_eval(raw_value)
    return values


def _plugin_entries(path: Path) -> list[dict[str, object]]:
    entries: list[dict[str, object]] = []
    current: dict[str, object] | None = None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line == "[[plugins]]":
            current = {}
            entries.append(current)
            continue
        if current is not None and "=" in line:
            key, value = line.split("=", 1)
            current[key.strip()] = ast.literal_eval(value.strip())
    return entries


def _build_kit_declaration_session() -> str:
    messages = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": KIT_DECLARATION_RPC_METHOD},
        {"jsonrpc": "2.0", "id": 3, "method": "shutdown"},
    ]
    return "\n".join(json.dumps(message) for message in messages) + "\n"


def _python_source_manifest() -> dict[str, object]:
    return _parse_top_level_toml(
        ROOT / "implementations/python/.provekit/lift/python-source/manifest.toml"
    )


def _contract(ir: list[dict[str, object]], suffix: str) -> dict[str, object]:
    for item in ir:
        if str(item.get("fnName", "")).endswith(suffix):
            return item
    raise AssertionError(f"missing contract ending in {suffix!r}: {ir!r}")


def _runtime_failure_loci(contract: dict[str, object]) -> list[dict[str, object]]:
    loci = contract.get("panicLoci")
    assert isinstance(loci, list), contract
    return [locus for locus in loci if isinstance(locus, dict)]


def _var(name: str) -> dict[str, object]:
    return {"kind": "var", "name": name}


def _str_const(value: str) -> dict[str, object]:
    return {
        "kind": "const",
        "value": value,
        "sort": {"kind": "primitive", "name": "String"},
    }


def _attr(value: dict[str, object], name: str) -> dict[str, object]:
    return {"kind": "ctor", "name": "python:attribute", "args": [value, _str_const(name)]}


def _subscript(value: dict[str, object], index: dict[str, object]) -> dict[str, object]:
    return {"kind": "ctor", "name": "python:subscript", "args": [value, index]}


def _slice(
    lower: dict[str, object],
    upper: dict[str, object],
    step: dict[str, object],
) -> dict[str, object]:
    return {"kind": "ctor", "name": "python:slice", "args": [lower, upper, step]}


def _none_const() -> dict[str, object]:
    return {
        "kind": "const",
        "value": None,
        "sort": {"kind": "primitive", "name": "Unit"},
    }


def _no_value() -> dict[str, object]:
    return {"kind": "ctor", "name": "python:no_value", "args": []}


def _aug_assign(
    target: dict[str, object], op: str, value: dict[str, object]
) -> dict[str, object]:
    return {
        "kind": "ctor",
        "name": "python:aug_assign",
        "args": [target, _str_const(op), value],
    }


def _ann_assign(
    target: dict[str, object],
    annotation: dict[str, object],
    value: dict[str, object],
) -> dict[str, object]:
    return {
        "kind": "ctor",
        "name": "python:ann_assign",
        "args": [target, annotation, value],
    }


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


def _compare_pairs(node: object) -> list[tuple[str, str, str]]:
    pairs: list[tuple[str, str, str]] = []
    if isinstance(node, dict):
        if node.get("kind") == "ctor" and node.get("name") == "python:compare":
            args = node["args"]
            pairs.append((args[0]["value"], args[1]["name"], args[2]["name"]))
        for child in node.get("args", []):
            pairs.extend(_compare_pairs(child))
    elif isinstance(node, list):
        for child in node:
            pairs.extend(_compare_pairs(child))
    return pairs


def _assert_guard(term: object, expected_head: str, expected_arg: str) -> dict[str, object]:
    assert isinstance(term, dict)
    assert term["kind"] == "ctor"
    assert term["name"] == "cf_guarded"
    args = term["args"]
    guard = args[0]
    assert guard == {
        "kind": "ctor",
        "name": expected_head,
        "args": [{"kind": "var", "name": expected_arg}],
    }
    return args[1]


def _assert_none_guarded_if(
    body: dict[str, object],
    *,
    op: str,
    then_head: str,
    else_head: str,
) -> None:
    assert body["kind"] == "ctor"
    assert body["name"] == "cf_ite"
    cond, then_branch, else_branch = body["args"]
    assert cond == {
        "kind": "ctor",
        "name": "python:compare",
        "args": [
            {"kind": "const", "value": op, "sort": {"kind": "primitive", "name": "String"}},
            {"kind": "var", "name": "x"},
            {"kind": "const", "value": None, "sort": {"kind": "primitive", "name": "Unit"}},
        ],
    }
    assert _assert_guard(then_branch, then_head, "x")["name"] == "python:return"
    assert _assert_guard(else_branch, else_head, "x")["name"] == "python:return"


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


@pytest.mark.parametrize(
    ("expr", "expected_pairs"),
    [
        ("a < b", [("<", "a", "b")]),
        ("a == b", [("==", "a", "b")]),
        ("a >= b", [(">=", "a", "b")]),
    ],
)
def test_compare_single_op_lifts_pairwise_discriminated_term(
    expr: str,
    expected_pairs: list[tuple[str, str, str]],
) -> None:
    result = lift_source(f"def f(a, b):\n    return {expr}\n", "compare_single.py")

    body = _contract(result.ir, ".f")["post"]["args"][1]

    assert result.refusals == []
    assert _compare_pairs(body) == expected_pairs
    assert "python:and" not in _ctor_names(body)


@pytest.mark.parametrize(
    ("expr", "expected_pairs"),
    [
        ("a < b < c", [("<", "a", "b"), ("<", "b", "c")]),
        ("a < b >= c", [("<", "a", "b"), (">=", "b", "c")]),
        ("a == b != c", [("==", "a", "b"), ("!=", "b", "c")]),
    ],
)
def test_compare_two_chain_lifts_to_pairwise_and_composition(
    expr: str,
    expected_pairs: list[tuple[str, str, str]],
) -> None:
    result = lift_source(f"def f(a, b, c):\n    return {expr}\n", "compare_two.py")

    body = _contract(result.ir, ".f")["post"]["args"][1]

    assert result.refusals == []
    assert _compare_pairs(body) == expected_pairs
    assert _ctor_names(body).count("python:and") == 1


@pytest.mark.parametrize(
    ("expr", "expected_pairs"),
    [
        ("a < b <= c != d", [("<", "a", "b"), ("<=", "b", "c"), ("!=", "c", "d")]),
        ("a > b >= c == d", [(">", "a", "b"), (">=", "b", "c"), ("==", "c", "d")]),
        ("a != b < c > d", [("!=", "a", "b"), ("<", "b", "c"), (">", "c", "d")]),
    ],
)
def test_compare_three_chain_mixed_ops_lifts_to_pairwise_and_composition(
    expr: str,
    expected_pairs: list[tuple[str, str, str]],
) -> None:
    result = lift_source(f"def f(a, b, c, d):\n    return {expr}\n", "compare_three.py")

    body = _contract(result.ir, ".f")["post"]["args"][1]

    assert result.refusals == []
    assert _compare_pairs(body) == expected_pairs
    assert _ctor_names(body).count("python:and") == 2


def test_if_is_none_lifts_to_cf_guarded_option_guards() -> None:
    source = (
        "def f(x):\n"
        "    if x is None:\n"
        "        return 0\n"
        "    else:\n"
        "        return 1\n"
    )

    result = lift_source(source, "none_guard.py")

    body = _contract(result.ir, ".f")["post"]["args"][1]
    assert result.refusals == []
    _assert_none_guarded_if(
        body,
        op="is",
        then_head="is_none",
        else_head="is_some",
    )
    assert "python:compare" in _ctor_names(body)


def test_if_is_not_none_lifts_to_cf_guarded_option_guards() -> None:
    source = (
        "def f(x):\n"
        "    if x is not None:\n"
        "        return 0\n"
        "    else:\n"
        "        return 1\n"
    )

    result = lift_source(source, "some_guard.py")

    body = _contract(result.ir, ".f")["post"]["args"][1]
    assert result.refusals == []
    _assert_none_guarded_if(
        body,
        op="is not",
        then_head="is_some",
        else_head="is_none",
    )
    assert "python:compare" in _ctor_names(body)


def test_raise_emits_runtime_failure_locus_without_changing_effect_set() -> None:
    source = "def f():\n    raise ValueError\n"

    result = lift_source(source, "raises.py")

    contract = _contract(result.ir, ".f")
    body = contract["post"]["args"][1]
    assert result.refusals == []
    assert body == {
        "kind": "ctor",
        "name": "python:raise",
        "args": [{"kind": "var", "name": "ValueError"}],
    }
    assert contract["effects"] == [{"kind": "panics"}]
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "explicit-raise",
            "exceptionClass": "ValueError",
            "argTerm": {"kind": "var", "name": "ValueError"},
            "file": "raises.py",
            "line": 2,
            "col": 4,
        }
    ]


def test_multiple_raise_sites_keep_distinct_runtime_failure_loci() -> None:
    source = (
        "def f(flag):\n"
        "    if flag:\n"
        "        raise ValueError\n"
        "    raise RuntimeError\n"
    )

    result = lift_source(source, "multi_raise.py")

    contract = _contract(result.ir, ".f")
    assert result.refusals == []
    assert contract["effects"] == [{"kind": "panics"}]
    loci = _runtime_failure_loci(contract)
    assert [locus["exceptionClass"] for locus in loci] == ["ValueError", "RuntimeError"]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(3, 8), (4, 4)]
    assert all(locus["effectKind"] == PANIC_FREEDOM_EFFECT_KIND for locus in loci)
    assert all(locus["callee"] == RUNTIME_FAILURE_SITE_CONCEPT for locus in loci)
    assert all(locus["subkind"] == "explicit-raise" for locus in loci)


def test_bare_raise_emits_runtime_failure_locus_with_unit_arg() -> None:
    source = "def f():\n    raise\n"

    result = lift_source(source, "bare_raise.py")

    contract = _contract(result.ir, ".f")
    assert result.refusals == []
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "explicit-raise",
            "argTerm": {
                "kind": "const",
                "value": None,
                "sort": {"kind": "primitive", "name": "Unit"},
            },
            "file": "bare_raise.py",
            "line": 2,
            "col": 4,
        }
    ]


def test_attribute_load_emits_runtime_failure_locus_and_panics_effect() -> None:
    source = "def f(obj):\n    return obj.name\n"

    result = lift_source(source, "attr_access.py")

    contract = _contract(result.ir, ".f")
    assert result.refusals == []
    assert contract["effects"] == [{"kind": "panics"}]
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "attribute-access",
            "exceptionClass": "AttributeError",
            "argTerm": {
                "kind": "ctor",
                "name": "python:attribute",
                "args": [
                    {"kind": "var", "name": "obj"},
                    {
                        "kind": "const",
                        "value": "name",
                        "sort": {"kind": "primitive", "name": "String"},
                    },
                ],
            },
            "file": "attr_access.py",
            "line": 2,
            "col": 11,
        }
    ]


def test_subscript_load_emits_runtime_failure_locus_and_panics_effect() -> None:
    source = "def f(xs, key):\n    return xs[key]\n"

    result = lift_source(source, "subscript_access.py")

    contract = _contract(result.ir, ".f")
    assert result.refusals == []
    assert contract["effects"] == [{"kind": "panics"}]
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "subscript-access",
            "argTerm": {
                "kind": "ctor",
                "name": "python:subscript",
                "args": [
                    {"kind": "var", "name": "xs"},
                    {"kind": "var", "name": "key"},
                ],
            },
            "file": "subscript_access.py",
            "line": 2,
            "col": 11,
        }
    ]


def test_slice_load_emits_subscript_access_runtime_failure_locus() -> None:
    source = "def f(xs, a, b):\n    value = xs[a:b]\n    return value\n"

    result = lift_source(source, "slice_access.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    target = _subscript(_var("xs"), _slice(_var("a"), _var("b"), _none_const()))
    body = contract["post"]["args"][1]
    assert contract["effects"] == [{"kind": "panics"}]
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "subscript-access",
            "argTerm": target,
            "file": "slice_access.py",
            "line": 2,
            "col": 12,
        }
    ]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [_var("value"), target],
    }


@pytest.mark.parametrize(
    ("source_expr", "expected_target"),
    [
        (
            "xs[a:b:c]",
            _subscript(_var("xs"), _slice(_var("a"), _var("b"), _var("c"))),
        ),
        (
            "xs[:b]",
            _subscript(_var("xs"), _slice(_none_const(), _var("b"), _none_const())),
        ),
        (
            "xs[a:]",
            _subscript(_var("xs"), _slice(_var("a"), _none_const(), _none_const())),
        ),
        (
            "xs[:]",
            _subscript(_var("xs"), _slice(_none_const(), _none_const(), _none_const())),
        ),
    ],
)
def test_slice_load_preserves_slice_shape_in_body_and_locus(
    source_expr: str,
    expected_target: dict[str, object],
) -> None:
    source = f"def f(xs, a, b, c):\n    value = {source_expr}\n    return value\n"

    result = lift_source(source, "slice_access_shape.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == ["subscript-access"]
    assert [locus["argTerm"] for locus in loci] == [expected_target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 12)]
    assert "exceptionClass" not in loci[0]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [_var("value"), expected_target],
    }


def test_nested_slice_load_receiver_emits_intermediate_access_and_slice_access() -> None:
    source = "def f(obj, a, b):\n    value = obj.inner[a:b]\n    return value\n"

    result = lift_source(source, "nested_slice_access.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    obj_inner = _attr(_var("obj"), "inner")
    target = _subscript(obj_inner, _slice(_var("a"), _var("b"), _none_const()))
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "subscript-access",
    ]
    assert [locus["argTerm"] for locus in loci] == [obj_inner, target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 12), (2, 12)]
    assert loci[0]["exceptionClass"] == "AttributeError"
    assert "exceptionClass" not in loci[1]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [_var("value"), target],
    }


def test_slice_load_bound_expressions_resurface_load_loci_before_slice_access() -> None:
    source = "def f(xs, obj):\n    value = xs[obj.i:obj.j]\n    return value\n"

    result = lift_source(source, "slice_access_bounds.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    obj_i = _attr(_var("obj"), "i")
    obj_j = _attr(_var("obj"), "j")
    target = _subscript(_var("xs"), _slice(obj_i, obj_j, _none_const()))
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "attribute-access",
        "subscript-access",
    ]
    assert [locus["argTerm"] for locus in loci] == [obj_i, obj_j, target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [
        (2, 15),
        (2, 21),
        (2, 12),
    ]
    assert [locus.get("exceptionClass") for locus in loci] == [
        "AttributeError",
        "AttributeError",
        None,
    ]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [_var("value"), target],
    }


def test_slice_load_receiver_is_evaluated_before_slice_bounds() -> None:
    source = (
        "def f(obj, other):\n"
        "    value = obj.inner[other.i:other.j]\n"
        "    return value\n"
    )

    result = lift_source(source, "slice_access_order.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    obj_inner = _attr(_var("obj"), "inner")
    other_i = _attr(_var("other"), "i")
    other_j = _attr(_var("other"), "j")
    target = _subscript(obj_inner, _slice(other_i, other_j, _none_const()))
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "attribute-access",
        "attribute-access",
        "subscript-access",
    ]
    assert [locus["argTerm"] for locus in loci] == [
        obj_inner,
        other_i,
        other_j,
        target,
    ]
    assert [(locus["line"], locus["col"]) for locus in loci] == [
        (2, 12),
        (2, 22),
        (2, 30),
        (2, 12),
    ]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [_var("value"), target],
    }


@pytest.mark.parametrize(
    "source",
    [
        "def f(xs, a, b):\n    value = xs[a:b]\n    return value\n",
        "def f(xs, a, b):\n    lower = xs[:b]\n    upper = xs[a:]\n    all_items = xs[:]\n    return all_items\n",
        "def f(xs, a, b, c):\n    value = xs[a:b:c]\n    return value\n",
    ],
)
def test_compile_lift_roundtrip_preserves_slice_load_body(source: str) -> None:
    lifted = lift_source(source, "roundtrip_slice_access.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_slice_access.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


def test_mixed_runtime_failure_sites_share_deduped_panics_effect() -> None:
    source = (
        "def f(obj, xs, key):\n"
        "    a = obj.name\n"
        "    b = xs[key]\n"
        "    raise RuntimeError\n"
    )

    result = lift_source(source, "mixed_runtime.py")

    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    assert result.refusals == []
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "subscript-access",
        "explicit-raise",
    ]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 8), (3, 8), (4, 4)]


def test_attribute_and_subscript_store_targets_emit_runtime_failure_loci() -> None:
    source = (
        "def f(obj, xs, key, value):\n"
        "    obj.name = value\n"
        "    xs[key] = value\n"
        "    return value\n"
    )

    result = lift_source(source, "store_targets.py")

    contract = _contract(result.ir, ".f")
    assert result.refusals == []
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.name"},
        {"kind": "writes", "target": "xs[key]"},
        {"kind": "panics"},
    ]
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "attribute-write",
            "exceptionClass": "AttributeError",
            "argTerm": {
                "kind": "ctor",
                "name": "python:attribute",
                "args": [
                    {"kind": "var", "name": "obj"},
                    {
                        "kind": "const",
                        "value": "name",
                        "sort": {"kind": "primitive", "name": "String"},
                    },
                ],
            },
            "file": "store_targets.py",
            "line": 2,
            "col": 4,
        },
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "subscript-write",
            "argTerm": {
                "kind": "ctor",
                "name": "python:subscript",
                "args": [
                    {"kind": "var", "name": "xs"},
                    {"kind": "var", "name": "key"},
                ],
            },
            "file": "store_targets.py",
            "line": 3,
            "col": 4,
        },
    ]


def test_nested_attribute_and_subscript_store_targets_resurface_load_loci() -> None:
    source = (
        "def f(obj, xs, ys, i, value):\n"
        "    obj.inner.name = value\n"
        "    xs[ys[i]] = value\n"
        "    return value\n"
    )

    result = lift_source(source, "nested_store_targets.py")

    contract = _contract(result.ir, ".f")
    assert result.refusals == []
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.inner.name"},
        {"kind": "writes", "target": "xs[ys[i]]"},
        {"kind": "panics"},
    ]
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "attribute-access",
            "exceptionClass": "AttributeError",
            "argTerm": {
                "kind": "ctor",
                "name": "python:attribute",
                "args": [
                    {"kind": "var", "name": "obj"},
                    {
                        "kind": "const",
                        "value": "inner",
                        "sort": {"kind": "primitive", "name": "String"},
                    },
                ],
            },
            "file": "nested_store_targets.py",
            "line": 2,
            "col": 4,
        },
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "attribute-write",
            "exceptionClass": "AttributeError",
            "argTerm": {
                "kind": "ctor",
                "name": "python:attribute",
                "args": [
                    {
                        "kind": "ctor",
                        "name": "python:attribute",
                        "args": [
                            {"kind": "var", "name": "obj"},
                            {
                                "kind": "const",
                                "value": "inner",
                                "sort": {"kind": "primitive", "name": "String"},
                            },
                        ],
                    },
                    {
                        "kind": "const",
                        "value": "name",
                        "sort": {"kind": "primitive", "name": "String"},
                    },
                ],
            },
            "file": "nested_store_targets.py",
            "line": 2,
            "col": 4,
        },
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "subscript-access",
            "argTerm": {
                "kind": "ctor",
                "name": "python:subscript",
                "args": [
                    {"kind": "var", "name": "ys"},
                    {"kind": "var", "name": "i"},
                ],
            },
            "file": "nested_store_targets.py",
            "line": 3,
            "col": 7,
        },
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "subscript-write",
            "argTerm": {
                "kind": "ctor",
                "name": "python:subscript",
                "args": [
                    {"kind": "var", "name": "xs"},
                    {
                        "kind": "ctor",
                        "name": "python:subscript",
                        "args": [
                            {"kind": "var", "name": "ys"},
                            {"kind": "var", "name": "i"},
                        ],
                    },
                ],
            },
            "file": "nested_store_targets.py",
            "line": 3,
            "col": 4,
        },
    ]


def test_slice_assign_emits_subscript_write_runtime_failure_locus() -> None:
    source = "def f(xs, a, b, value):\n    xs[a:b] = value\n    return xs\n"

    result = lift_source(source, "slice_assign.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    target = _subscript(_var("xs"), _slice(_var("a"), _var("b"), _none_const()))
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": "xs[a:b]"},
        {"kind": "panics"},
    ]
    assert _runtime_failure_loci(contract) == [
        {
            "effectKind": PANIC_FREEDOM_EFFECT_KIND,
            "callee": RUNTIME_FAILURE_SITE_CONCEPT,
            "subkind": "subscript-write",
            "argTerm": target,
            "file": "slice_assign.py",
            "line": 2,
            "col": 4,
        }
    ]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [target, _var("value")],
    }


@pytest.mark.parametrize(
    ("source_target", "expected_target", "expected_write"),
    [
        (
            "xs[a:b:c]",
            _subscript(_var("xs"), _slice(_var("a"), _var("b"), _var("c"))),
            "xs[a:b:c]",
        ),
        (
            "xs[:b]",
            _subscript(_var("xs"), _slice(_none_const(), _var("b"), _none_const())),
            "xs[:b]",
        ),
        (
            "xs[a:]",
            _subscript(_var("xs"), _slice(_var("a"), _none_const(), _none_const())),
            "xs[a:]",
        ),
        (
            "xs[:]",
            _subscript(_var("xs"), _slice(_none_const(), _none_const(), _none_const())),
            "xs[:]",
        ),
    ],
)
def test_slice_assign_preserves_slice_shape_in_body_and_locus(
    source_target: str,
    expected_target: dict[str, object],
    expected_write: str,
) -> None:
    source = f"def f(xs, a, b, c, value):\n    {source_target} = value\n    return xs\n"

    result = lift_source(source, "slice_assign_shape.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": expected_write},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == ["subscript-write"]
    assert [locus["argTerm"] for locus in loci] == [expected_target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4)]
    assert "exceptionClass" not in loci[0]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [expected_target, _var("value")],
    }


def test_nested_slice_assign_receiver_emits_intermediate_access_and_slice_write() -> None:
    source = "def f(obj, a, b, value):\n    obj.inner[a:b] = value\n    return obj\n"

    result = lift_source(source, "nested_slice_assign.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    obj_inner = _attr(_var("obj"), "inner")
    target = _subscript(obj_inner, _slice(_var("a"), _var("b"), _none_const()))
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.inner[a:b]"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "subscript-write",
    ]
    assert [locus["argTerm"] for locus in loci] == [obj_inner, target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4), (2, 4)]
    assert loci[0]["exceptionClass"] == "AttributeError"
    assert "exceptionClass" not in loci[1]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [target, _var("value")],
    }


def test_slice_assign_bound_expressions_resurface_load_loci_before_slice_write() -> None:
    source = "def f(xs, obj, value):\n    xs[obj.i:obj.j] = value\n    return xs\n"

    result = lift_source(source, "slice_assign_bounds.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    obj_i = _attr(_var("obj"), "i")
    obj_j = _attr(_var("obj"), "j")
    target = _subscript(_var("xs"), _slice(obj_i, obj_j, _none_const()))
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": "xs[obj.i:obj.j]"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "attribute-access",
        "subscript-write",
    ]
    assert [locus["argTerm"] for locus in loci] == [obj_i, obj_j, target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [
        (2, 7),
        (2, 13),
        (2, 4),
    ]
    assert [locus.get("exceptionClass") for locus in loci] == [
        "AttributeError",
        "AttributeError",
        None,
    ]
    assert body["args"][0] == {
        "kind": "ctor",
        "name": "python:assign",
        "args": [target, _var("value")],
    }


@pytest.mark.parametrize(
    "source",
    [
        "def f(xs, a, b, value):\n    xs[a:b] = value\n    return xs\n",
        "def f(xs, a, b, value):\n    xs[:b] = value\n    xs[a:] = value\n    xs[:] = value\n    return xs\n",
        "def f(xs, a, b, c, value):\n    xs[a:b:c] = value\n    return xs\n",
    ],
)
def test_compile_lift_roundtrip_preserves_slice_assign_body(source: str) -> None:
    lifted = lift_source(source, "roundtrip_slice_assign.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_slice_assign.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


@pytest.mark.parametrize(
    ("source", "module", "function_name"),
    [
        (
            "def f(xs, a, b, value):\n    xs[a:b] += value\n    return xs\n",
            "slice_augassign_refusal.py",
            "slice_augassign_refusal.f",
        ),
        (
            "def f(xs, a, b, value):\n    xs[a:b]: int = value\n    return xs\n",
            "slice_annassign_refusal.py",
            "slice_annassign_refusal.f",
        ),
    ],
)
def test_non_load_or_assign_slice_subscripts_remain_refused_for_slice_9_scope(
    source: str,
    module: str,
    function_name: str,
) -> None:
    result = lift_source(source, module)

    assert result.refusals == [
        {
            "kind": "unhandled-syntax",
            "function": function_name,
            "line": 2,
            "reason": "slice subscripts are refused",
        }
    ]
    assert len(result.ir) == 1


def test_name_augassign_lifts_to_aug_assign_without_runtime_failure_loci() -> None:
    source = "def f(x, y):\n    x += y\n    return x\n"

    result = lift_source(source, "name_augassign.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    body = contract["post"]["args"][1]
    assert contract["effects"] == []
    assert contract.get("panicLoci", []) == []
    assert body["name"] == "python:seq"
    assert body["args"][0] == _aug_assign(_var("x"), "python:add", _var("y"))


def test_attribute_augassign_emits_access_and_write_runtime_failure_loci() -> None:
    source = "def f(obj, y):\n    obj.name += y\n    return obj\n"

    result = lift_source(source, "attribute_augassign.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    target = _attr(_var("obj"), "name")
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.name"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "attribute-write",
    ]
    assert [locus["argTerm"] for locus in loci] == [target, target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4), (2, 4)]
    assert all(locus["exceptionClass"] == "AttributeError" for locus in loci)
    assert body["args"][0] == _aug_assign(target, "python:add", _var("y"))


def test_subscript_augassign_emits_access_and_write_runtime_failure_loci() -> None:
    source = "def f(xs, key, y):\n    xs[key] += y\n    return xs\n"

    result = lift_source(source, "subscript_augassign.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    target = _subscript(_var("xs"), _var("key"))
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": "xs[key]"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == [
        "subscript-access",
        "subscript-write",
    ]
    assert [locus["argTerm"] for locus in loci] == [target, target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4), (2, 4)]
    assert "exceptionClass" not in loci[0]
    assert "exceptionClass" not in loci[1]
    assert body["args"][0] == _aug_assign(target, "python:add", _var("y"))


def test_nested_augassign_targets_evaluate_navigation_once() -> None:
    source = (
        "def f(obj, xs, ys, i, y):\n"
        "    obj.inner.name += y\n"
        "    xs[ys[i]] += y\n"
        "    return y\n"
    )

    result = lift_source(source, "nested_augassign.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    obj_inner = _attr(_var("obj"), "inner")
    obj_inner_name = _attr(obj_inner, "name")
    ys_i = _subscript(_var("ys"), _var("i"))
    xs_ys_i = _subscript(_var("xs"), ys_i)
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.inner.name"},
        {"kind": "writes", "target": "xs[ys[i]]"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "attribute-access",
        "attribute-write",
        "subscript-access",
        "subscript-access",
        "subscript-write",
    ]
    assert [locus["argTerm"] for locus in loci] == [
        obj_inner,
        obj_inner_name,
        obj_inner_name,
        ys_i,
        xs_ys_i,
        xs_ys_i,
    ]
    assert [(locus["line"], locus["col"]) for locus in loci] == [
        (2, 4),
        (2, 4),
        (2, 4),
        (3, 7),
        (3, 4),
        (3, 4),
    ]
    assert [locus["argTerm"] for locus in loci].count(obj_inner) == 1
    assert [locus["argTerm"] for locus in loci].count(ys_i) == 1


def test_augassign_complex_rhs_emits_rhs_load_loci_after_target_loci() -> None:
    source = "def f(obj, xs, key):\n    obj.name += xs[key]\n    return obj\n"

    result = lift_source(source, "augassign_rhs.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    obj_name = _attr(_var("obj"), "name")
    xs_key = _subscript(_var("xs"), _var("key"))
    loci = _runtime_failure_loci(contract)
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.name"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "attribute-write",
        "subscript-access",
    ]
    assert [locus["argTerm"] for locus in loci] == [obj_name, obj_name, xs_key]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4), (2, 4), (2, 16)]


def test_compile_lift_roundtrip_preserves_attribute_augassign_body() -> None:
    source = "def f(obj, y):\n    obj.name += y\n    return obj\n"
    lifted = lift_source(source, "roundtrip_aug_attr.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_aug_attr.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


def test_compile_lift_roundtrip_preserves_subscript_augassign_body() -> None:
    source = "def f(xs, key, y):\n    xs[key] += y\n    return xs\n"
    lifted = lift_source(source, "roundtrip_aug_subscript.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_aug_subscript.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


def test_name_annassign_without_value_has_no_runtime_failure_loci_or_effects() -> None:
    source = "def f():\n    x: int\n    return 0\n"

    result = lift_source(source, "name_annassign_no_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    body = contract["post"]["args"][1]
    assert contract["effects"] == []
    assert contract.get("panicLoci", []) == []
    assert body["name"] == "python:seq"
    assert body["args"][0] == _ann_assign(_var("x"), _var("int"), _no_value())


def test_name_annassign_with_value_has_no_runtime_failure_loci_or_effects() -> None:
    source = "def f(y):\n    x: int = y\n    return x\n"

    result = lift_source(source, "name_annassign_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    body = contract["post"]["args"][1]
    assert contract["effects"] == []
    assert contract.get("panicLoci", []) == []
    assert body["args"][0] == _ann_assign(_var("x"), _var("int"), _var("y"))


def test_direct_attribute_annassign_without_value_does_not_access_final_attribute() -> None:
    source = "def f(obj):\n    obj.name: int\n    return obj\n"

    result = lift_source(source, "attr_annassign_no_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    body = contract["post"]["args"][1]
    target = _attr(_var("obj"), "name")
    assert contract["effects"] == []
    assert contract.get("panicLoci", []) == []
    assert body["args"][0] == _ann_assign(target, _var("int"), _no_value())


def test_direct_attribute_annassign_with_value_emits_store_write_locus_only() -> None:
    source = "def f(obj, y):\n    obj.name: int = y\n    return obj\n"

    result = lift_source(source, "attr_annassign_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    target = _attr(_var("obj"), "name")
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.name"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == ["attribute-write"]
    assert [locus["argTerm"] for locus in loci] == [target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4)]
    assert loci[0]["exceptionClass"] == "AttributeError"
    assert body["args"][0] == _ann_assign(target, _var("int"), _var("y"))


def test_direct_subscript_annassign_without_value_does_not_access_final_subscript() -> None:
    source = "def f(xs, key):\n    xs[key]: int\n    return xs\n"

    result = lift_source(source, "subscript_annassign_no_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    body = contract["post"]["args"][1]
    target = _subscript(_var("xs"), _var("key"))
    assert contract["effects"] == []
    assert contract.get("panicLoci", []) == []
    assert body["args"][0] == _ann_assign(target, _var("int"), _no_value())


def test_direct_subscript_annassign_with_value_emits_store_write_locus_only() -> None:
    source = "def f(xs, key, y):\n    xs[key]: int = y\n    return xs\n"

    result = lift_source(source, "subscript_annassign_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    target = _subscript(_var("xs"), _var("key"))
    loci = _runtime_failure_loci(contract)
    body = contract["post"]["args"][1]
    assert contract["effects"] == [
        {"kind": "writes", "target": "xs[key]"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == ["subscript-write"]
    assert [locus["argTerm"] for locus in loci] == [target]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4)]
    assert "exceptionClass" not in loci[0]
    assert body["args"][0] == _ann_assign(target, _var("int"), _var("y"))


def test_nested_annassign_without_value_emits_only_intermediate_navigation_loci() -> None:
    source = (
        "def f(obj, xs, ys, i):\n"
        "    obj.inner.name: int\n"
        "    xs[ys[i]]: int\n"
        "    return obj\n"
    )

    result = lift_source(source, "nested_annassign_no_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    obj_inner = _attr(_var("obj"), "inner")
    ys_i = _subscript(_var("ys"), _var("i"))
    body = contract["post"]["args"][1]
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "subscript-access",
    ]
    assert [locus["argTerm"] for locus in loci] == [obj_inner, ys_i]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 4), (3, 7)]
    statements = body["args"][0]["args"]
    assert statements[0] == _ann_assign(
        _attr(obj_inner, "name"), _var("int"), _no_value()
    )
    assert statements[1] == _ann_assign(
        _subscript(_var("xs"), ys_i), _var("int"), _no_value()
    )


def test_annassign_missing_value_and_explicit_none_have_distinct_body_terms() -> None:
    source = "def f():\n    missing: int\n    explicit: int = None\n    return explicit\n"

    result = lift_source(source, "annassign_none_discrimination.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    body = contract["post"]["args"][1]
    statements = body["args"][0]["args"]
    missing = _ann_assign(_var("missing"), _var("int"), _no_value())
    explicit_none = _ann_assign(_var("explicit"), _var("int"), _none_const())
    assert statements[0] == missing
    assert statements[1] == explicit_none
    assert missing != explicit_none


def test_nested_annassign_with_value_reuses_store_target_navigation_once() -> None:
    source = (
        "def f(obj, xs, ys, i, value):\n"
        "    obj.inner.name: int = value\n"
        "    xs[ys[i]]: int = value\n"
        "    return value\n"
    )

    result = lift_source(source, "nested_annassign_value.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    obj_inner = _attr(_var("obj"), "inner")
    obj_inner_name = _attr(obj_inner, "name")
    ys_i = _subscript(_var("ys"), _var("i"))
    xs_ys_i = _subscript(_var("xs"), ys_i)
    assert contract["effects"] == [
        {"kind": "writes", "target": "obj.inner.name"},
        {"kind": "writes", "target": "xs[ys[i]]"},
        {"kind": "panics"},
    ]
    assert [locus["subkind"] for locus in loci] == [
        "attribute-access",
        "attribute-write",
        "subscript-access",
        "subscript-write",
    ]
    assert [locus["argTerm"] for locus in loci] == [
        obj_inner,
        obj_inner_name,
        ys_i,
        xs_ys_i,
    ]
    assert [(locus["line"], locus["col"]) for locus in loci] == [
        (2, 4),
        (2, 4),
        (3, 7),
        (3, 4),
    ]
    assert [locus["argTerm"] for locus in loci].count(obj_inner) == 1
    assert [locus["argTerm"] for locus in loci].count(ys_i) == 1


def test_no_value_annassign_evaluates_receiver_but_not_final_attribute() -> None:
    source = "def f(make):\n    make().name: int\n    return 0\n"

    result = lift_source(source, "annassign_receiver_call.py")

    assert result.refusals == []
    contract = _contract(result.ir, ".f")
    assert contract["effects"] == [{"kind": "unresolved_call", "name": "make"}]
    assert contract.get("panicLoci", []) == []


def test_compile_lift_roundtrip_preserves_attribute_annassign_body_with_value() -> None:
    source = "def f(obj, y):\n    obj.name: int = y\n    return obj\n"
    lifted = lift_source(source, "roundtrip_ann_attr_value.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_ann_attr_value.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


def test_compile_lift_roundtrip_preserves_attribute_annassign_body_without_value() -> None:
    source = "def f(obj):\n    obj.name: int\n    return obj\n"
    lifted = lift_source(source, "roundtrip_ann_attr_no_value.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_ann_attr_no_value.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


def test_compile_lift_roundtrip_preserves_name_annassign_without_value() -> None:
    source = "def f():\n    x: int\n    return 0\n"
    lifted = lift_source(source, "roundtrip_ann_name_no_value.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_ann_name_no_value.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)
    assert "x: int = None" not in compiled


def test_compile_lift_roundtrip_preserves_name_annassign_explicit_none_value() -> None:
    source = "def f():\n    x: int = None\n    return x\n"
    lifted = lift_source(source, "roundtrip_ann_name_explicit_none.py")
    assert lifted.refusals == []
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_ann_name_explicit_none.py")
    assert relifted.refusals == []
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)
    assert "x: int = None" in compiled


def test_none_guarded_attribute_access_emits_one_runtime_failure_locus() -> None:
    source = (
        "def f(obj):\n"
        "    if obj.name is None:\n"
        "        return 0\n"
        "    return 1\n"
    )

    result = lift_source(source, "attr_guard.py")

    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    assert result.refusals == []
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == ["attribute-access"]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 7)]


def test_none_guarded_subscript_access_emits_one_runtime_failure_locus() -> None:
    source = (
        "def f(xs, key):\n"
        "    if xs[key] is None:\n"
        "        return 0\n"
        "    return 1\n"
    )

    result = lift_source(source, "subscript_guard.py")

    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    assert result.refusals == []
    assert contract["effects"] == [{"kind": "panics"}]
    assert [locus["subkind"] for locus in loci] == ["subscript-access"]
    assert [(locus["line"], locus["col"]) for locus in loci] == [(2, 7)]


def test_method_call_callee_attribute_emits_runtime_failure_locus() -> None:
    source = "def f(obj):\n    return obj.method()\n"

    result = lift_source(source, "method_call.py")

    contract = _contract(result.ir, ".f")
    loci = _runtime_failure_loci(contract)
    assert result.refusals == []
    assert {"kind": "panics"} in contract["effects"]
    assert {"kind": "unresolved_call", "name": "obj.method"} in contract["effects"]
    assert [locus["subkind"] for locus in loci] == ["attribute-access"]
    assert loci[0]["exceptionClass"] == "AttributeError"
    assert loci[0]["argTerm"] == {
        "kind": "ctor",
        "name": "python:attribute",
        "args": [
            {"kind": "var", "name": "obj"},
            {
                "kind": "const",
                "value": "method",
                "sort": {"kind": "primitive", "name": "String"},
            },
        ],
    }
    assert (loci[0]["line"], loci[0]["col"]) == (2, 11)


def test_compile_lift_roundtrip_preserves_cf_guarded_none_if_body() -> None:
    source = (
        "def f(x):\n"
        "    if x is None:\n"
        "        return 0\n"
        "    else:\n"
        "        return 1\n"
    )
    lifted = lift_source(source, "roundtrip_none.py")
    contract = _contract(lifted.ir, ".f")
    body = contract["post"]["args"][1]

    compiled = compile_body_term(
        body,
        fn_name="f",
        formals=[str(formal) for formal in contract["formals"]],
    )
    relifted = lift_source(compiled, "roundtrip_none.py")
    relifted_body = _contract(relifted.ir, ".f")["post"]["args"][1]

    assert canonical_json_bytes(relifted_body) == canonical_json_bytes(body)


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


def test_list_comprehension_refusal_does_not_fire_different_variant() -> None:
    source = "def bad(xs):\n    return [x for x in xs]\n"

    result = lift_source(source, "badmodule.py")

    assert [refusal["kind"] for refusal in result.refusals] == ["unhandled-syntax"]
    assert "syntax-error" not in _canon(result.refusals)


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


def test_checked_in_project_registers_python_source_lift_surface() -> None:
    entries = _plugin_entries(ROOT / "implementations/python/.provekit/config.toml")

    assert {
        "name": "python-source",
        "kind": "lift",
        "surface": "python-source",
    } in entries


def test_checked_in_python_source_manifest_invokes_module_form_and_declares_kit() -> None:
    manifest = _python_source_manifest()

    assert manifest["command"] == [
        "python3",
        "-m",
        "provekit_lift_python_source",
        "--rpc",
    ]
    assert manifest["working_dir"] == "provekit-lift-python-source/src"

    completed = subprocess.run(
        manifest["command"],
        cwd=ROOT / "implementations/python" / str(manifest["working_dir"]),
        input=_build_kit_declaration_session(),
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )

    assert completed.returncode == 0, completed.stderr
    responses = [
        json.loads(line) for line in completed.stdout.splitlines() if line.strip()
    ]
    declaration = next(response for response in responses if response.get("id") == 2)
    assert "error" not in declaration, declaration
    assert declaration["result"]["kit"]["id"] == "python-source"
    assert declaration["result"]["effectLeaves"] == RUNTIME_FAILURE_EFFECT_LEAVES


def test_kit_declaration_returns_python_source_lift_surface() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 2, "method": KIT_DECLARATION_RPC_METHOD})

    assert "error" not in response, response
    result = response["result"]
    assert result["kit"] == {
        "id": "python-source",
        "language": "python",
        "version": "0.1.0-draft",
    }
    required_by_name = {
        method["name"]: method["required"] for method in result["rpc"]["methods"]
    }
    assert required_by_name == {
        "initialize": True,
        KIT_DECLARATION_RPC_METHOD: True,
        "lift": True,
        "compile": False,
        "shutdown": False,
    }
    assert result["proofResolution"] == {"strategy": "pip"}
    assert result["effectKinds"] == ["concept:panic-freedom"]
    assert result["effectLeaves"] == RUNTIME_FAILURE_EFFECT_LEAVES
    assert all("subkind" not in leaf for leaf in result["effectLeaves"])
    assert result["guardPredicates"] == [
        {
            "surface": "python-source",
            "local": "is_some",
            "concept": "concept:panic-freedom.option.some",
        },
        {
            "surface": "python-source",
            "local": "is_none",
            "concept": "concept:panic-freedom.option.none",
        },
    ]
    assert result["controlCarriers"] == [
        {
            "surface": "python-source",
            "local": "cf_guarded",
            "concept": "concept:panic-freedom.guard",
        },
        {
            "surface": "python-source",
            "local": "cf_ite",
            "concept": "concept:panic-freedom.choice",
        },
    ]
    assert result["residueCategories"] == []
