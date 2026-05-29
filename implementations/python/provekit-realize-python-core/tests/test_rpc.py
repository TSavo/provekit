from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_core import realizer
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


def test_rpc_body_template_entries_returns_core_template_entries() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "provekit.plugin.body_template_entries",
            "params": {"target_library_tag": "urllib"},
        }
    )

    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 4
    result = response["result"]
    assert result["template_authority"] == realizer.KIT_ID
    entries = result["entries"]
    assert entries
    assert {entry["target_library_tag"] for entry in entries} == {"urllib"}
    concepts = {entry["concept_name"] for entry in entries}
    assert "identity" in concepts
    assert "http-request" in concepts
    assert "concept:blake3-512-of" not in concepts

    identity = next(entry for entry in entries if entry["concept_name"] == "identity")
    assert identity == {
        "concept_name": "identity",
        "emission_template": {"kind": "verbatim", "template": "return ${param0}"},
        "signature_guard": {"min_params": 1, "max_params": 1},
        "target_library_tag": "urllib",
    }


def test_rpc_body_template_entries_reports_missing_core_resource(monkeypatch) -> None:
    monkeypatch.setattr(realizer, "_package_body_template_resource", lambda _relative: None)
    monkeypatch.setattr(realizer, "_find_repo_file", lambda _relative: None)

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "provekit.plugin.body_template_entries",
            "params": {"target_library_tag": "urllib"},
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 5,
        "error": {
            "code": 1404,
            "message": (
                "BODY_TEMPLATE_RESOURCE_NOT_FOUND: provekit-realize-python-core "
                "could not resolve body-template resources for target_library_tag=urllib"
            ),
            "data": {
                "template_authority": realizer.KIT_ID,
                "target_library_tag": "urllib",
                "missing_resources": [str(realizer.BODY_TEMPLATE_REL)],
            },
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


def test_plugin_check_runs_python_compile(tmp_path: Path) -> None:
    source = tmp_path / "sample.py"
    source.write_text("def ok():\n    return 1\n", encoding="utf-8")

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 11,
            "method": "provekit.plugin.check",
            "params": {"kind": "materialize", "out_dir": str(tmp_path)},
        }
    )

    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 11
    assert response["result"]["ok"] is True
    assert response["result"]["command"] == "python -m py_compile"
    assert str(source) in response["result"]["checked_files"]


def test_plugin_check_reports_python_compile_failure(tmp_path: Path) -> None:
    source = tmp_path / "bad.py"
    source.write_text("def bad(\n    return 1\n", encoding="utf-8")

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 12,
            "method": "provekit.plugin.check",
            "params": {"kind": "materialize", "out_dir": str(tmp_path)},
        }
    )

    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 12
    assert response["result"]["ok"] is False
    assert response["result"]["command"] == "python -m py_compile"
    assert str(source) in response["result"]["stderr"]


def test_plugin_shutdown_returns_null() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "provekit.plugin.shutdown",
        }
    )

    assert response == {"jsonrpc": "2.0", "id": 2, "result": None}
