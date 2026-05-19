from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-sqlite3/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_sqlite3.rpc import dispatch


def test_rpc_invoke_renders_sqlite3_body() -> None:
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
    assert "db.execute" in response["result"]["source"]


def test_rpc_invoke_uses_named_term_tree_shape_for_sql_query() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "select_by_id",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "list[object]",
                "concept_name": "concept:sql-query",
                "named_term_tree": {
                    "conceptName": "concept:sql-query",
                    "operationKind": "sql-query",
                    "shapeCid": "blake3-512:sql-query",
                    "args": [
                        {
                            "conceptName": "Sql",
                            "operationKind": "literal",
                            "shapeCid": "blake3-512:sql",
                            "args": [],
                        },
                        {
                            "conceptName": "SqlArgs",
                            "operationKind": "tuple",
                            "shapeCid": "blake3-512:sql-args",
                            "args": [],
                        },
                    ],
                },
            },
        }
    )

    assert response["id"] == 3
    assert response["result"]["source"] == (
        "def select_by_id(id):\n"
        "    cursor = db.execute(sql, tuple(args))\n"
        "    return cursor.fetchall()\n"
    )
    assert response["result"]["is_stub"] is False


def test_rpc_invoke_without_named_term_tree_keeps_bare_signature_lookup() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "select_by_id",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "list[object]",
                "concept_name": "concept:sql-query",
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 4,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "concept:sql-query",
                    "args_shape": ["int"],
                    "function": "select_by_id",
                    "term_position": "body",
                }
            ],
        },
    }


def test_rpc_missing_template_reports_named_term_tree_shape() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "missing_query",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "list[object]",
                "concept_name": "missing-concept",
                "named_term_tree": {
                    "conceptName": "missing-concept",
                    "operationKind": "missing",
                    "shapeCid": "blake3-512:missing",
                    "args": [
                        {
                            "conceptName": "Sql",
                            "operationKind": "literal",
                            "shapeCid": "blake3-512:sql",
                            "args": [],
                        },
                        {
                            "conceptName": "SqlArgs",
                            "operationKind": "tuple",
                            "shapeCid": "blake3-512:sql-args",
                            "args": [],
                        },
                    ],
                },
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 5,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "missing-concept",
                    "args_shape": ["str", "list[object]"],
                    "function": "missing_query",
                    "term_position": "body",
                }
            ],
        },
    }


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
