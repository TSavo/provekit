from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-requests/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_requests.realizer import MissingTemplateError, emit_stub


def _proof_candidate(
    *,
    body: str,
    signature_shape_cid: str = "blake3-512:" + "a" * 128,
    admission_tier: str = "Self-Attested",
    concept_name: str = "concept:http-request",
) -> dict[str, object]:
    return {
        "admission_tier": admission_tier,
        "body": body,
        "concept_name": concept_name,
        "package_evidence": {
            "kind": "python-distribution",
            "opaque_handle": "site-packages/provekit_shim_requests",
        },
        "signature_shape_cid": signature_shape_cid,
        "target_language": "python",
        "target_library_tag": "requests",
    }


def test_proof_backed_binding_wins_over_legacy_body_template() -> None:
    sig = "blake3-512:" + "b" * 128

    result = emit_stub(
        function="fetch_status",
        params=["url"],
        param_types=["str"],
        return_type="int",
        concept_name="concept:http-request",
        signature_shape_cid=sig,
        binding_candidates=[
            _proof_candidate(
                signature_shape_cid=sig,
                body="response = requests.request('GET', url)\nreturn response.status_code",
            )
        ],
    )

    assert result["source"] == (
        "def fetch_status(url):\n"
        "    response = requests.request('GET', url)\n"
        "    return response.status_code\n"
    )
    assert result["selection"]["source"] == "proof-backed-library-binding"
    assert result["selection"]["package_evidence"] == {
        "kind": "python-distribution",
        "opaque_handle": "site-packages/provekit_shim_requests",
    }


def test_proof_backed_binding_tie_breaks_by_admission_tier() -> None:
    sig = "blake3-512:" + "c" * 128

    result = emit_stub(
        function="fetch_status",
        params=["url"],
        param_types=["str"],
        return_type="int",
        concept_name="concept:http-request",
        signature_shape_cid=sig,
        binding_candidates=[
            _proof_candidate(
                signature_shape_cid=sig,
                admission_tier="Third-party Inferred",
                body="return 599",
            ),
            _proof_candidate(
                signature_shape_cid=sig,
                admission_tier="Self-Attested",
                body="return 200",
            ),
        ],
    )

    assert result["source"] == "def fetch_status(url):\n    return 200\n"
    assert result["selection"]["admission_tier"] == "Self-Attested"


def test_strict_mode_refuses_legacy_json_fallback() -> None:
    try:
        emit_stub(
            function="fetch_status",
            params=["url"],
            param_types=["str"],
            return_type="int",
            concept_name="concept:http-request",
            strict_proof_bindings=True,
        )
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "concept:http-request",
                "args_shape": ["str"],
                "function": "fetch_status",
                "term_position": "body",
            }
        ]
    else:
        raise AssertionError("strict proof-backed mode should refuse JSON fallback")


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
