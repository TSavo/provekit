from __future__ import annotations

import json
import py_compile
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
CORE_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(CORE_SRC) not in sys.path:
    sys.path.insert(0, str(CORE_SRC))
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


def test_rpc_assemble_returns_compileable_python_module(tmp_path: Path) -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "provekit.plugin.assemble",
            "params": {
                "target_lang": "python",
                "file_basename": "client",
                "fragments": [
                    {
                        "concept_name": "concept:http-request",
                        "source": (
                            "def fetch_status(url):\n"
                            "    return requests.get(url).status_code\n"
                        ),
                        "imports": ["requests"],
                        "helpers": ["DEFAULT_TIMEOUT = 30"],
                    }
                ],
            },
        }
    )

    assert "error" not in response
    result = response["result"]
    assert result["compile_classpath"] == []
    assert len(result["files"]) == 1
    file = result["files"][0]
    assert file["path"] == "client.py"
    content = file["content"]
    assert "import requests" in content
    assert "DEFAULT_TIMEOUT = 30" in content
    assert "def fetch_status(url):" in content
    assert "return requests.get(url).status_code" in content

    module = tmp_path / "client.py"
    module.write_text(content, encoding="utf-8")
    py_compile.compile(str(module), doraise=True)


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


def test_plugin_check_runs_python_compile(tmp_path: Path) -> None:
    source = tmp_path / "client.py"
    source.write_text("def fetch_status(url):\n    return 200\n", encoding="utf-8")

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 9,
            "method": "provekit.plugin.check",
            "params": {"kind": "materialize", "out_dir": str(tmp_path)},
        }
    )

    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 9
    assert response["result"]["ok"] is True
    assert response["result"]["command"] == "python -m py_compile"
    assert str(source) in response["result"]["checked_files"]


def test_plugin_check_reports_python_compile_failure(tmp_path: Path) -> None:
    source = tmp_path / "bad_client.py"
    source.write_text("def fetch_status(\n    return 200\n", encoding="utf-8")

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "provekit.plugin.check",
            "params": {"kind": "materialize", "out_dir": str(tmp_path)},
        }
    )

    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 10
    assert response["result"]["ok"] is False
    assert response["result"]["command"] == "python -m py_compile"
    assert str(source) in response["result"]["stderr"]


def test_rpc_error_for_unknown_method_is_json_serializable() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 2, "method": "missing"})

    assert response["error"]["code"] == -32601
    json.dumps(response)
