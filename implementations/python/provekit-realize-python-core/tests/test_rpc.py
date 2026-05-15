from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.rpc import dispatch


def test_plugin_invoke_returns_source_and_stub_flag() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "wrap_identity",
                "params": ["x"],
                "param_types": ["int"],
                "return_type": "int",
                "concept_name": "identity",
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "source": "def wrap_identity(x):\n    return x\n",
            "is_stub": False,
            "extension": "py",
        },
    }


def test_plugin_invoke_returns_structured_missing_template_error() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "unknown_call",
                "params": ["x"],
                "param_types": ["int"],
                "return_type": "int",
                "concept_name": "return(call:Widget::build(x))",
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
                    "operation_kind": "call:Widget::build",
                    "args_shape": ["int"],
                    "function": "unknown_call",
                    "term_position": "body.return.call:Widget::build",
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
                        "concept_name": "return(call:Widget::build(x))",
                    },
                    {
                        "function": "second",
                        "params": ["y"],
                        "param_types": ["str"],
                        "return_type": "str",
                        "concept_name": "missing-concept",
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
                    "operation_kind": "call:Widget::build",
                    "args_shape": ["int"],
                    "function": "first",
                    "term_position": "body.return.call:Widget::build",
                },
                {
                    "operation_kind": "missing-concept",
                    "args_shape": ["str"],
                    "function": "second",
                    "term_position": "body",
                },
            ],
        },
    }


def test_plugin_shutdown_returns_null() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "provekit.plugin.shutdown",
        }
    )

    assert response == {"jsonrpc": "2.0", "id": 2, "result": None}
