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
