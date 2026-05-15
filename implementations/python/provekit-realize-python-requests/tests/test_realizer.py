from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-requests/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_requests.realizer import MissingTemplateError, emit_stub


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
