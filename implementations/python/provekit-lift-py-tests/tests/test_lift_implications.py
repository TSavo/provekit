# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import json
import os
import subprocess
import sys
from typing import List


_CID_A = "blake3-512:" + "a" * 128
_CID_B = "blake3-512:" + "b" * 128
_CID_C = "blake3-512:" + "c" * 128


def _lsp_cmd() -> List[str]:
    return [sys.executable, "-m", "provekit_lift_py_tests.lsp"]


def _run_lsp(ndjson_input: str) -> List[dict]:
    src_dir = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "src"))
    env = os.environ.copy()
    existing = env.get("PYTHONPATH")
    env["PYTHONPATH"] = src_dir if not existing else os.pathsep.join([src_dir, existing])
    result = subprocess.run(
        _lsp_cmd(),
        input=ndjson_input,
        capture_output=True,
        text=True,
        timeout=10,
        env=env,
    )
    assert result.returncode == 0, result.stderr
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def _run_lift_implications(tmp_path, source: str, *, source_paths=None, contract_bindings=None):
    (tmp_path / "example.py").write_text(source, encoding="utf-8")
    msgs = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "provekit.plugin.lift_implications",
            "params": {
                "workspace_root": str(tmp_path),
                "source_paths": source_paths or ["example.py"],
                "contract_bindings": contract_bindings or [],
            },
        },
        {"jsonrpc": "2.0", "id": 3, "method": "shutdown"},
    ]
    responses = _run_lsp("\n".join(json.dumps(m) for m in msgs) + "\n")
    return next(r for r in responses if r.get("id") == 2)


def test_lift_implications_rpc_emits_bridge_per_call_expression(tmp_path):
    response = _run_lift_implications(
        tmp_path,
        """\
def caller(value):
    parsed = parse_input(value)
    return parsed.normalize_value()
""",
        contract_bindings=[
            {"name": "parse_input@example.py:10:4", "contract_cid": _CID_A},
            {"name": "normalize_value@example.py:20:4", "contract_cid": _CID_B},
        ],
    )

    assert "error" not in response, response
    result = response["result"]
    assert result["kind"] == "ir-document"
    ir = result["ir"]
    assert len(ir) == 2

    parse_bridge = next(entry for entry in ir if entry["sourceSymbol"] == "parse_input")
    assert parse_bridge["kind"] == "bridge"
    assert parse_bridge["sourceLayer"] == "python"
    assert parse_bridge["targetLayer"] == "python-tests"
    assert parse_bridge["targetContractCid"] == _CID_A
    assert parse_bridge["target"] == {"kind": "contract", "cid": _CID_A}
    assert parse_bridge["name"].startswith("intra-body:python:parse_input@example.py:")

    method_bridge = next(entry for entry in ir if entry["sourceSymbol"] == "normalize_value")
    assert method_bridge["kind"] == "bridge"
    assert method_bridge["targetContractCid"] == _CID_B


def test_lift_implications_emits_lift_gap_for_unmatched_callee(tmp_path):
    response = _run_lift_implications(
        tmp_path,
        """\
def caller():
    return completely_unknown_function(0)
"""
    )

    assert "error" not in response, response
    assert response["result"]["ir"] == []
    diagnostics = response["result"]["diagnostics"]
    assert len(diagnostics) == 1
    assert diagnostics[0]["kind"] == "lift-gap"
    assert diagnostics[0]["reason"] == "no-contract-for-callee"
    assert diagnostics[0]["callee"] == "completely_unknown_function"


def test_lift_implications_uses_last_attribute_segment_as_callee_name(tmp_path):
    response = _run_lift_implications(
        tmp_path,
        """\
def caller(payload):
    return json.loads(payload)
""",
        contract_bindings=[
            {"name": "loads@json.py:1:1", "contract_cid": _CID_C},
        ],
    )

    assert "error" not in response, response
    ir = response["result"]["ir"]
    assert [entry["sourceSymbol"] for entry in ir] == ["loads"]
    assert ir[0]["targetContractCid"] == _CID_C


def test_lift_implications_matches_module_qualified_producer_contract_name(tmp_path):
    response = _run_lift_implications(
        tmp_path,
        """\
def caller(value):
    return callee(value)
""",
        contract_bindings=[
            {"name": "app.callee", "contract_cid": _CID_A},
        ],
    )

    assert "error" not in response, response
    ir = response["result"]["ir"]
    assert [entry["sourceSymbol"] for entry in ir] == ["callee"]
    assert ir[0]["targetContractCid"] == _CID_A
    assert response["result"]["diagnostics"] == []
