"""RPC dispatch conventions for the Hypothesis emitter."""

from __future__ import annotations

import json

from provekit_emit_python_hypothesis.rpc import dispatch


def _op(name: str, *args: dict) -> dict:
    return {"kind": "op", "name": name, "args": list(args)}


def _var(name: str) -> dict:
    return {"kind": "var", "name": name}


def test_describe_returns_emit_plugin_memento_for_hypothesis() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 1, "method": "provekit.plugin.describe"})
    result = response["result"]

    assert response["id"] == 1
    assert set(result.keys()) == {"envelope", "header", "metadata"}
    header = result["header"]
    assert header["kind"] == "emit"
    assert "pep/1.7.0" in header["protocol_versions"]
    assert header["content"]["target_language"] == "python"
    assert header["content"]["target_framework"] == "hypothesis"
    assert "concept:lt" in header["content"]["capabilities"]["predicates"]
    json.dumps(response)


def test_invoke_emits_hypothesis_module() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "provekit.plugin.invoke",
            "params": {
                "contract_id": "concept:lt",
                "function": "ordered",
                "params": ["x", "y"],
                "param_types": ["int", "int"],
                "predicates": [_op("concept:lt", _var("x"), _var("y"))],
            },
        }
    )

    result = response["result"]
    assert response["id"] == 2
    assert result["kind"] == "hypothesis-test-emission"
    assert result["path"] == "test_ordered_hypothesis_contract.py"
    assert result["extension"] == "py"
    assert "@given(data=st.data())" in result["source"]
    assert result["emitted_predicates"] == ["lt"]
    assert result["is_complete"] is True
    assert result["emitted_artifact_cid"].startswith("blake3-512:")
    json.dumps(response)


def test_invoke_reports_unsupported_gap() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "provekit.plugin.invoke",
            "params": {"function": "f", "predicates": [_op("concept:fallible-err", _var("f"))]},
        }
    )

    result = response["result"]
    assert result["unsupported_predicates"] == ["fallible-err"]
    assert result["is_complete"] is False


def test_shutdown_returns_null_result() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 4, "method": "provekit.plugin.shutdown"})
    assert response == {"jsonrpc": "2.0", "id": 4, "result": None}


def test_invalid_params_error() -> None:
    response = dispatch(
        {"jsonrpc": "2.0", "id": 5, "method": "provekit.plugin.invoke", "params": []}
    )
    assert response["error"]["code"] == -32602
