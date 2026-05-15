from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-aiosqlite/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_aiosqlite.rpc import dispatch


def test_rpc_invoke_renders_aiosqlite_body() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "select_rows",
                "params": ["sql", "args"],
                "param_types": ["str", "list[object]"],
                "return_type": "list[object]",
                "concept_name": "concept:sql-query",
            },
        }
    )

    assert response["id"] == 1
    assert response["result"]["is_stub"] is False
    assert "async with db.execute" in response["result"]["source"]


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
