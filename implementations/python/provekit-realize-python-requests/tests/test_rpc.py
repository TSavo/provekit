from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-requests/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

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


def test_rpc_error_for_unknown_method_is_json_serializable() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 2, "method": "missing"})

    assert response["error"]["code"] == -32601
    json.dumps(response)
