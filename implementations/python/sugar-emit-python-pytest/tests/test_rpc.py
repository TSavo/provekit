"""RPC dispatch conventions: describe / invoke / shutdown / errors."""

from __future__ import annotations

import json

from sugar_emit_python_pytest.rpc import dispatch


def _op(name: str, *args: dict) -> dict:
    return {"kind": "op", "name": name, "args": list(args)}


def _var(name: str) -> dict:
    return {"kind": "var", "name": name}


def test_describe_returns_plugin_memento_shape() -> None:
    # The loader requires the result to be a full {envelope, header, metadata}
    # plugin memento, not a flat capability object.
    response = dispatch({"jsonrpc": "2.0", "id": 1, "method": "sugar.plugin.describe"})
    result = response["result"]
    assert response["id"] == 1
    assert set(result.keys()) == {"envelope", "header", "metadata"}
    header = result["header"]
    assert header["schemaVersion"] == "1"
    assert "pep/1.7.0" in header["protocol_versions"]
    assert header["cid"].startswith("blake3-512:")
    # Capabilities live inside header.content (opaque to the loader).
    assert "concept:eq" in header["content"]["capabilities"]["predicates"]
    assert header["content"]["target_framework"] == "pytest"
    json.dumps(response)  # must be serializable


def test_describe_accepts_loader_runtime_protocol_versions_param() -> None:
    # The loader sends params={"runtime_protocol_versions": ["pep/1.7.0"]};
    # dispatch must not crash on it and must still return the memento.
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 11,
            "method": "sugar.plugin.describe",
            "params": {"runtime_protocol_versions": ["pep/1.7.0"]},
        }
    )
    assert set(response["result"].keys()) == {"envelope", "header", "metadata"}


def test_invoke_emits_pytest_module() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "sugar.plugin.invoke",
            "params": {
                "contract_id": "concept:eq",
                "function": "f",
                "params": ["a", "b"],
                "param_types": ["int", "int"],
                "predicates": [_op("concept:eq", _var("a"), _var("b"))],
            },
        }
    )
    result = response["result"]
    assert response["id"] == 2
    assert result["kind"] == "pytest-test-emission"
    assert result["path"] == "test_f_contract.py"
    assert result["extension"] == "py"
    assert "assert a == b" in result["source"]
    assert result["emitted_predicates"] == ["eq"]
    assert result["is_complete"] is True
    assert result["emitted_artifact_cid"].startswith("blake3-512:")
    json.dumps(response)


def test_invoke_reports_unsupported_gap() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "sugar.plugin.invoke",
            "params": {
                "function": "f",
                "predicates": [_op("concept:mystery", _var("a"))],
            },
        }
    )
    result = response["result"]
    assert result["unsupported_predicates"] == ["mystery"]
    assert result["is_complete"] is False


def test_shutdown_returns_null_result() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 4, "method": "sugar.plugin.shutdown"})
    assert response == {"jsonrpc": "2.0", "id": 4, "result": None}


def test_unknown_method_is_json_serializable_error() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 5, "method": "missing"})
    assert response["error"]["code"] == -32601
    json.dumps(response)


def test_invalid_params_error() -> None:
    response = dispatch(
        {"jsonrpc": "2.0", "id": 6, "method": "sugar.plugin.invoke", "params": []}
    )
    assert response["error"]["code"] == -32602
