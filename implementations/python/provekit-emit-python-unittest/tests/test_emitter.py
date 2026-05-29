"""Module-level emission behavior for the unittest emitter."""

from __future__ import annotations

from provekit_emit_python_unittest.emitter import EmitPlan, emit


def _atomic(name: str, *args: dict) -> dict:
    return {"kind": "atomic", "name": name, "args": list(args)}


def _var(name: str) -> dict:
    return {"kind": "var", "name": name}


def test_emits_unittest_case_class_and_native_assertions() -> None:
    plan = EmitPlan(
        contract_id="concept:eq",
        function="identity",
        params=["actual", "expected"],
        param_types=["int", "int"],
        predicates=[
            _atomic("concept:eq", _var("actual"), _var("expected")),
            _atomic("concept:option-is-none", _var("missing")),
        ],
    )

    emission = emit(plan)

    assert emission.path == "test_identity_contract.py"
    assert emission.emitted_predicates == ["eq", "option-is-none"]
    assert emission.unsupported_predicates == []
    assert emission.is_complete
    assert "import unittest" in emission.source
    assert "class TestIdentityContract(unittest.TestCase):" in emission.source
    assert "self.assertEqual(actual, expected)" in emission.source
    assert "self.assertIsNone(missing)" in emission.source
    assert emission.artifact_cid.startswith("blake3-512:")


def test_unsupported_predicate_recorded_as_gap_not_emitted() -> None:
    emission = emit(
        EmitPlan(
            function="f",
            predicates=[
                _atomic("concept:eq", _var("a"), _var("b")),
                _atomic("concept:mystery", _var("a")),
            ],
        )
    )

    assert emission.emitted_predicates == ["eq"]
    assert emission.unsupported_predicates == ["mystery"]
    assert not emission.is_complete
    assert "mystery" not in emission.source


def test_from_params_accepts_rpc_shape_and_aliases() -> None:
    plan = EmitPlan.from_params(
        {
            "concept_name": "concept:eq",
            "function_name": "f",
            "params": ["a", "b"],
            "param_types": ["int", "int"],
            "predicates": [_atomic("concept:eq", _var("a"), _var("b"))],
        }
    )

    assert plan.contract_id == "concept:eq"
    assert plan.function == "f"
    assert plan.params == ["a", "b"]
    assert len(plan.predicates) == 1

