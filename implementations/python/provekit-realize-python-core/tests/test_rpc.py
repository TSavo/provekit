from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core.rpc import dispatch


def _cid(ch: str) -> str:
    return "blake3-512:" + ch * 128


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


def test_plugin_invoke_threads_transported_op() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "transport_drop",
                "params": ["x"],
                "param_types": ["object"],
                "return_type": "()",
                "concept_name": "missing-python-drop-surface",
                "transported_op": {
                    "args_jcs": [{"kind": "var", "name": "x"}],
                    "concept_cid": _cid("a"),
                    "concept_name": "concept:drop",
                    "concept_site_cid": _cid("b"),
                    "loss_record_cid": _cid("c"),
                    "operation_kind": "drop",
                    "policy_cid": _cid("d"),
                    "shape_cid": _cid("e"),
                    "sugar_dict_cid": _cid("f"),
                    "term_position": [3, 0],
                },
            },
        }
    )

    source = response["result"]["source"]
    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 10
    assert "# provekit-concept:" in source
    assert "# provekit-concept-payload-cid: blake3-512:" in source
    assert "    pass\n" in source


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


def test_plugin_invoke_threads_named_term_tree() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 9,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "compose_tree",
                "params": ["value"],
                "param_types": ["int"],
                "return_type": "int",
                "concept_name": "UNNAMED-CONCEPT-1",
                "named_term_tree": {
                    "conceptName": "concept:seq",
                    "operationKind": "seq",
                    "shapeCid": "blake3-512:seq",
                    "args": [
                        {
                            "conceptName": "identity",
                            "operationKind": "call",
                            "shapeCid": "blake3-512:call",
                            "args": [],
                        },
                        {
                            "conceptName": "concept:return",
                            "operationKind": "return",
                            "shapeCid": "blake3-512:return",
                            "args": [],
                        },
                    ],
                },
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 9,
        "result": {
            "source": "def compose_tree(value):\n    return value\n    return value\n",
            "is_stub": False,
            "extension": "py",
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
