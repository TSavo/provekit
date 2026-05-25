from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-requests/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))
SHIM_SRC = ROOT / "examples/provekit-shim-python-requests"
if str(SHIM_SRC) not in sys.path:
    sys.path.insert(0, str(SHIM_SRC))

from provekit_realize_python_requests import realizer
from provekit_realize_python_requests.realizer import (
    MissingTemplateError,
    body_template_for,
    emit_stub,
)


HTTP_REQUEST_TREE = {
    "conceptName": "concept:http-request",
    "operationKind": "http-request",
    "shapeCid": "blake3-512:http-request",
    "args": [
        {"name": "method", "sort": "HttpMethod", "args": []},
        {"name": "url", "sort": "Url", "args": []},
        {"name": "headers", "sort": "HeaderMap", "args": []},
        {"name": "body", "sort": "Optional<ByteStreamOrBytes>", "args": []},
    ],
}


def test_http_request_uses_requests_get() -> None:
    result = emit_stub(
        function="fetch_status",
        params=["url"],
        param_types=["str"],
        return_type="int",
        concept_name="concept:http-request",
    )

    assert result == {
        "source": "def fetch_status(url):\n    import requests\n    response = requests.get(url)\n    return response.status_code\n",
        "is_stub": False,
        "extension": "py",
    }


def test_requests_entries_self_resolve_from_shim_proof() -> None:
    proof_path = realizer._resolve_shim_proof_path()
    assert proof_path is not None
    assert proof_path.suffix == ".proof"

    entries = realizer._entries_from_shim_proof(proof_path, "requests")
    concepts = {entry.concept_name for entry in entries}

    assert "concept:http-request" in concepts
    assert "concept:http-response" in concepts


def test_http_request_catalog_shape_uses_requests_request() -> None:
    body = body_template_for(
        "concept:http-request",
        ["method", "url", "headers", "body"],
        ["HttpMethod", "Url", "HeaderMap", "Optional<ByteStreamOrBytes>"],
        "HttpResponse",
    )

    assert body == (
        "import requests\n"
        "kwargs = {\"headers\": headers, \"data\": body}\n"
        "return requests.request(method, url, **kwargs)"
    )


def test_http_request_uses_named_term_tree_catalog_shape() -> None:
    body = body_template_for(
        "concept:http-request",
        ["ignored"],
        ["object"],
        "HttpResponse",
        named_term_tree=HTTP_REQUEST_TREE,
    )

    assert body == (
        "import requests\n"
        "kwargs = {\"headers\": headers, \"data\": body}\n"
        "return requests.request(method, url, **kwargs)"
    )


def test_http_response_catalog_shape_constructs_requests_response() -> None:
    body = body_template_for(
        "concept:http-response",
        ["status", "headers", "body"],
        ["HttpStatus", "HeaderMap", "ByteStreamOrBytes"],
        "HttpResponse",
    )

    assert body == (
        "import requests\n"
        "response = requests.Response()\n"
        "response.status_code = status\n"
        "response.headers.update(headers or {})\n"
        "response._content = bytes(body)\n"
        "return response"
    )


def test_unknown_concept_refuses_missing_body_template() -> None:
    try:
        emit_stub("missing", ["x"], ["int"], "int", "missing-concept")
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "missing-concept",
                "args_shape": ["int"],
                "function": "missing",
                "term_position": "body",
            }
        ]
    else:
        raise AssertionError("missing body-template should refuse")
