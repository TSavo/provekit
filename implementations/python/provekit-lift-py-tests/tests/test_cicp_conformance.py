# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from provekit_lift_py_tests.canonicalizer import (
    Value,
    jcs_hash,
    varr,
    vbool,
    vint,
    vnull,
    vobj,
    vstr,
)


REPO_ROOT = Path(__file__).resolve().parents[4]
CICP_VECTOR_DIR = REPO_ROOT / "protocol" / "conformance" / "cicp"


def _json_to_value(value: Any) -> Value:
    if value is None:
        return vnull()
    if isinstance(value, bool):
        return vbool(value)
    if isinstance(value, int):
        return vint(value)
    if isinstance(value, str):
        return vstr(value)
    if isinstance(value, list):
        return varr([_json_to_value(item) for item in value])
    if isinstance(value, dict):
        return vobj([(key, _json_to_value(item)) for key, item in value.items()])
    raise TypeError(f"unsupported JSON value in CICP vector: {value!r}")


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _assert_no_missing_blast_radius_input_cids(body: dict[str, Any]) -> None:
    input_cids = set(body["inputCids"])
    required_cids = {
        body["protocolCatalogCid"],
        body["jobDefinitionCid"],
        body["commandCid"],
        body["runnerIdentityCid"],
        body["sourceClosureCid"],
        body["policyCid"],
        *body["toolchainCids"],
        *body["lockfileCids"],
        *body["generatedInputCids"],
        *body["fixtureCids"],
        *body["relevantSpecCids"],
    }

    missing = sorted(required_cids - input_cids)
    if missing:
        raise ValueError(f"inputCids missing required CID: {missing[0]}")


def test_cicp_passing_vectors_match_pinned_cids():
    catalog = _load_json(CICP_VECTOR_DIR / "vectors.json")

    for vector in catalog["vectors"]:
        if not vector["shouldPass"]:
            continue

        body = _load_json(CICP_VECTOR_DIR / vector["body"])

        assert jcs_hash(_json_to_value(body)) == vector["expectedCid"]


def test_cicp_invalid_blast_radius_vector_fails_closed_on_missing_input_cid():
    catalog = _load_json(CICP_VECTOR_DIR / "vectors.json")
    vector = next(v for v in catalog["vectors"] if not v["shouldPass"])
    body = _load_json(CICP_VECTOR_DIR / vector["body"])

    with pytest.raises(ValueError, match=vector["errorContains"]):
        _assert_no_missing_blast_radius_input_cids(body)
