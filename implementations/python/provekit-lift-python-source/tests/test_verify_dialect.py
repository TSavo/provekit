"""Unit tests for the verify-facing Python lift surface (Go-parity, PR #1445).

Covers the three transform halves and the cardinal-sin division guard:
  - to_verify_dialect lowers a `double` contract to `result == (* x 2)` / Int.
  - division ops stay NAMESPACED (uninterpreted) so the bridge is still written
    and wp refuses -> Undecidable, NEVER a false discharge.
  - an unannotated arithmetic body refuses (no `Value`-sorted obligation).
  - leaf harvester lifts `assert double(3) == 6` -> `=(double(3), 6)`.
  - the `contracts` surface gates emission on `@provekit.boundary`/`@sugar`.
"""

from __future__ import annotations

import pytest

from provekit_lift_python_source.leaf_assertions import harvest_source
from provekit_lift_python_source.lifter import lift_source
from provekit_lift_python_source.verify_dialect import (
    VerifyDialectRefusal,
    collect_int_signatures,
    to_verify_dialect,
)
from provekit_lift_python_source.verify_rpc import lift_workspace


def _fn_contract(source: str, source_path: str = "m.py"):
    result = lift_source(source, source_path)
    for item in result.ir:
        if item.get("kind") == "function-contract" and not str(item["fnName"]).startswith(
            "<source-unit"
        ):
            return item
    raise AssertionError("no function-contract lifted")


def test_double_lowers_to_dischargeable_core_form():
    source = "def double(x: int) -> int:\n    return x * 2\n"
    contract = _fn_contract(source)
    sorts = collect_int_signatures(source)["double"]
    out = to_verify_dialect(contract, sorts)

    assert out["bridgeSourceSymbol"] == "double"
    assert out["formalSorts"] == [{"kind": "primitive", "name": "Int"}]
    assert out["returnSort"] == {"kind": "primitive", "name": "Int"}
    post = out["post"]
    assert post["name"] == "="
    assert post["args"][0] == {"kind": "var", "name": "result"}
    value = post["args"][1]
    assert value["name"] == "*"  # python:mul normalized to SMT-core
    assert value["args"][0] == {"kind": "var", "name": "x"}
    assert value["args"][1]["value"] == 2


def test_addition_and_comparison_normalize():
    source = "def f(x: int, y: int) -> int:\n    return x + y\n"
    contract = _fn_contract(source)
    out = to_verify_dialect(contract, collect_int_signatures(source)["f"])
    assert out["post"]["args"][1]["name"] == "+"


def test_floordiv_stays_namespaced_not_refused():
    # CARDINAL SIN GUARD: `//` has no faithful core mapping. It must STAY
    # namespaced (so the bridge is written + wp refuses -> Undecidable), NOT be
    # refused (which would drop the bridge and risk a vacuous-pass fall-through).
    source = "def halve(x: int) -> int:\n    return x // 2\n"
    contract = _fn_contract(source)
    out = to_verify_dialect(contract, collect_int_signatures(source)["halve"])
    assert out["bridgeSourceSymbol"] == "halve"
    assert out["post"]["args"][1]["name"] == "python:floordiv"


def test_truediv_and_mod_stay_namespaced():
    for op, expected in (("/", "python:div"), ("%", "python:mod")):
        source = f"def g(x: int) -> int:\n    return x {op} 2\n"
        contract = _fn_contract(source)
        out = to_verify_dialect(contract, collect_int_signatures(source)["g"])
        assert out["post"]["args"][1]["name"] == expected


def test_unannotated_arithmetic_body_refuses():
    # No `: int` annotation -> a `Value`-sorted obligation z3 cannot discharge.
    # Refuse rather than emit it.
    source = "def double(x):\n    return x * 2\n"
    contract = _fn_contract(source)
    sorts = collect_int_signatures(source)["double"]
    with pytest.raises(VerifyDialectRefusal):
        to_verify_dialect(contract, sorts)


def test_multistatement_body_refuses():
    # A body that is not a single `return <expr>` is not a value-op.
    source = "def f(x: int) -> int:\n    y = x + 1\n    return y * 2\n"
    contract = _fn_contract(source)
    with pytest.raises(VerifyDialectRefusal):
        to_verify_dialect(contract, collect_int_signatures(source)["f"])


def test_leaf_harvester_lifts_call_eq():
    source = "def test_double():\n    assert double(3) == 6\n"
    result = harvest_source(source, "test_m.py")
    assert len(result.ir) == 1
    contract = result.ir[0]
    assert contract["kind"] == "contract"
    assert contract["name"] == "test_double"
    inv = contract["inv"]
    assert inv["name"] == "="
    call = inv["args"][0]
    assert call == {
        "kind": "ctor",
        "name": "double",
        "args": [{"kind": "const", "value": 3, "sort": {"kind": "primitive", "name": "Int"}}],
    }
    assert inv["args"][1]["value"] == 6


def test_leaf_harvester_negative_int_literal():
    source = "def test_halve():\n    assert halve(-7) == -4\n"
    result = harvest_source(source, "test_m.py")
    inv = result.ir[0]["inv"]
    assert inv["args"][0]["args"][0]["value"] == -7
    assert inv["args"][1]["value"] == -4


def test_contracts_surface_gates_on_boundary_declaration(tmp_path):
    # The `contracts` (ir-document) surface emits a function-contract ONLY for
    # functions carrying a `@provekit.boundary`/`@boundary` declaration.
    (tmp_path / "lib.py").write_text(
        "import provekit\n\n\n"
        "@provekit.boundary(concept='concept:mul')\n"
        "def declared(x: int) -> int:\n    return x * 2\n\n\n"
        "def undeclared(x: int) -> int:\n    return x + 1\n"
    )
    ir, _diag = lift_workspace(str(tmp_path), "contracts")
    fn_names = [i["fnName"].rsplit(".", 1)[-1] for i in ir if i.get("kind") == "function-contract"]
    assert "declared" in fn_names
    assert "undeclared" not in fn_names
    declared = next(i for i in ir if i.get("kind") == "function-contract")
    assert declared["conceptName"] == "concept:mul"
    assert declared["authoringKind"] == "boundary"


def test_bare_surface_emits_all_functions(tmp_path):
    (tmp_path / "lib.py").write_text("def double(x: int) -> int:\n    return x * 2\n")
    ir, _diag = lift_workspace(str(tmp_path), "bare")
    fn_names = [i["fnName"].rsplit(".", 1)[-1] for i in ir if i.get("kind") == "function-contract"]
    assert fn_names == ["double"]
