from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-requests/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))
SHIM_SRC = ROOT / "examples/provekit-shim-python-requests"
if str(SHIM_SRC) not in sys.path:
    sys.path.insert(0, str(SHIM_SRC))

from provekit_realize_python_requests.rpc import dispatch


def test_rpc_invoke_renders_requests_body() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "fetch_status",
                "params": ["url"],
                "param_types": ["str"],
                "return_type": "int",
                "concept_name": "concept:http-request",
            },
        }
    )

    assert response["id"] == 1
    assert response["result"]["is_stub"] is False
    assert "requests.get" in response["result"]["source"]


def test_rpc_invoke_renders_catalog_http_request_shape() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "send_request",
                "params": ["method", "url", "headers", "body"],
                "param_types": [
                    "HttpMethod",
                    "Url",
                    "HeaderMap",
                    "Optional<ByteStreamOrBytes>",
                ],
                "return_type": "HttpResponse",
                "concept_name": "concept:http-request",
            },
        }
    )

    assert response["id"] == 3
    assert response["result"]["source"] == (
        "def send_request(method, url, headers, body):\n"
        "    import requests\n"
        "    kwargs = {\"headers\": headers, \"data\": body}\n"
        "    return requests.request(method, url, **kwargs)\n"
    )


def test_rpc_body_template_entries_returns_shim_proof_entries() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "provekit.plugin.body_template_entries",
            "params": {"target_library_tag": "requests"},
        }
    )

    assert response["id"] == 4
    assert response["result"]["proof_path"].endswith(".proof")
    concepts = {entry["concept_name"] for entry in response["result"]["entries"]}
    assert "concept:http-request" in concepts
    assert "concept:http-response" in concepts


def test_plugin_invoke_returns_structured_missing_template_error() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "missing",
                "params": ["x"],
                "param_types": ["int"],
                "return_type": "int",
                "concept_name": "missing-concept",
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 7,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "missing-concept",
                    "args_shape": ["int"],
                    "function": "missing",
                    "term_position": "body",
                }
            ],
        },
    }


def test_emit_module_returns_all_missing_template_errors() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 8,
            "method": "provekit.plugin.emit_module",
            "params": {
                "functions": [
                    {
                        "function": "first",
                        "params": ["x"],
                        "param_types": ["int"],
                        "return_type": "int",
                        "concept_name": "first-missing",
                    },
                    {
                        "function": "second",
                        "params": ["y"],
                        "param_types": ["str"],
                        "return_type": "str",
                        "concept_name": "second-missing",
                    },
                ]
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 8,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "first-missing",
                    "args_shape": ["int"],
                    "function": "first",
                    "term_position": "body",
                },
                {
                    "operation_kind": "second-missing",
                    "args_shape": ["str"],
                    "function": "second",
                    "term_position": "body",
                },
            ],
        },
    }


def test_rpc_error_for_unknown_method_is_json_serializable() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 2, "method": "missing"})

    assert response["error"]["code"] == -32601
    json.dumps(response)
