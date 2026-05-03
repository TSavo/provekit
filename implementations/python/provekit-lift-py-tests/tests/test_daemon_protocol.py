# SPDX-License-Identifier: Apache-2.0
#
# Smoke test: protocol conformance of the provekit-lsp-python binary.
#
# Asserts:
#   - The binary (or python3 -m fallback) responds to initialize/parse/shutdown.
#   - parse response has `result.declarations` as a JSON array, not a string.
#   - parse response has `result.callEdges` as a JSON array.
#   - Each declaration in a non-empty result is an object with kind=="contract".
#   - Byte-determinism: two runs on the same input produce identical output.
#
# The binary under test is resolved in order:
#   1. provekit-lsp-python on PATH (installed via pip install -e .)
#   2. The installed user-bin path /Users/tsavo/Library/Python/3.9/bin/provekit-lsp-python
#   3. python3 -m provekit_lift_py_tests.lsp  (module fallback, always available)

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from typing import List

import pytest


# ---------------------------------------------------------------------------
# Helper: resolve the LSP binary command
# ---------------------------------------------------------------------------

def _lsp_cmd() -> List[str]:
    """Return the command list to invoke the Python LSP plugin."""
    # 1. On-PATH binary (post pip install).
    on_path = shutil.which("provekit-lsp-python")
    if on_path:
        return [on_path]

    # 2. Known user-scheme install location (macOS system Python 3.9).
    user_bin = os.path.expanduser("~/Library/Python/3.9/bin/provekit-lsp-python")
    if os.path.isfile(user_bin):
        return [user_bin]

    # 3. Module fallback -- always works when conftest.py has added src to sys.path.
    return [sys.executable, "-m", "provekit_lift_py_tests.lsp"]


# Fixture source containing a bounded-loop contract (Layer 2 pattern 1).
_FIXTURE_SOURCE = """\
def test_range_positive():
    for i in range(1, 10):
        assert i > 0
"""

_FIXTURE_PATH = "test_fixture.py"


def _run_lsp(ndjson_input: str) -> List[dict]:
    """Spawn the LSP binary, feed ndjson_input, return parsed response lines."""
    cmd = _lsp_cmd()
    result = subprocess.run(
        cmd,
        input=ndjson_input,
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode == 0, (
        f"LSP binary exited {result.returncode}; stderr: {result.stderr!r}"
    )
    lines = [line for line in result.stdout.splitlines() if line.strip()]
    return [json.loads(line) for line in lines]


def _build_session(source: str = _FIXTURE_SOURCE, path: str = _FIXTURE_PATH) -> str:
    """Build NDJSON input for initialize -> parse -> shutdown."""
    msgs = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "parse",
         "params": {"path": path, "source": source}},
        {"jsonrpc": "2.0", "id": 3, "method": "shutdown"},
    ]
    return "\n".join(json.dumps(m) for m in msgs) + "\n"


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

class TestDaemonProtocol:
    """Protocol conformance tests for the provekit-lsp-python binary."""

    def test_initialize_response(self):
        """Binary responds to initialize with the expected shape."""
        responses = _run_lsp(_build_session())
        init_resp = next(r for r in responses if r.get("id") == 1)
        result = init_resp["result"]
        assert result["name"] == "provekit-lsp-python"
        assert "parse" in result["capabilities"]

    def test_parse_declarations_is_array(self):
        """parse response: result.declarations is a JSON array, not a string."""
        responses = _run_lsp(_build_session())
        parse_resp = next(r for r in responses if r.get("id") == 2)
        assert "error" not in parse_resp, f"parse returned error: {parse_resp}"
        result = parse_resp["result"]
        assert isinstance(result["declarations"], list), (
            f"declarations should be list, got {type(result['declarations']).__name__}: "
            f"{result['declarations']!r}"
        )

    def test_parse_call_edges_is_array(self):
        """parse response: result.callEdges is a JSON array, not a string."""
        responses = _run_lsp(_build_session())
        parse_resp = next(r for r in responses if r.get("id") == 2)
        result = parse_resp["result"]
        assert isinstance(result["callEdges"], list), (
            f"callEdges should be list, got {type(result['callEdges']).__name__}: "
            f"{result['callEdges']!r}"
        )

    def test_declarations_contain_contracts(self):
        """With a contract-bearing fixture, each declaration has kind=='contract'."""
        responses = _run_lsp(_build_session())
        parse_resp = next(r for r in responses if r.get("id") == 2)
        decls = parse_resp["result"]["declarations"]
        assert len(decls) >= 1, "Expected at least one declaration from bounded-loop fixture"
        for d in decls:
            assert isinstance(d, dict), f"declaration is not a dict: {d!r}"
            assert d.get("kind") == "contract", (
                f"expected kind='contract', got {d.get('kind')!r}"
            )

    def test_declarations_have_name_field(self):
        """Each declaration is an object with a 'name' field."""
        responses = _run_lsp(_build_session())
        parse_resp = next(r for r in responses if r.get("id") == 2)
        for d in parse_resp["result"]["declarations"]:
            assert "name" in d, f"declaration missing 'name': {d!r}"

    def test_empty_source_returns_empty_arrays(self):
        """Empty source returns declarations=[] and callEdges=[]."""
        responses = _run_lsp(_build_session(source="# no contracts here\n"))
        parse_resp = next(r for r in responses if r.get("id") == 2)
        result = parse_resp["result"]
        assert result["declarations"] == []
        assert result["callEdges"] == []

    def test_byte_determinism(self):
        """Two independent runs on the same input produce identical output."""
        ndjson = _build_session()
        run1 = _run_lsp(ndjson)
        run2 = _run_lsp(ndjson)
        # Compare the parse response (id==2) from both runs.
        parse1 = next(r for r in run1 if r.get("id") == 2)
        parse2 = next(r for r in run2 if r.get("id") == 2)
        assert json.dumps(parse1, sort_keys=True) == json.dumps(parse2, sort_keys=True), (
            "parse response is not byte-deterministic across two runs"
        )

    def test_unknown_language_returns_error(self):
        """Requesting a non-python language returns a JSON-RPC error, not a crash."""
        msgs = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
            {"jsonrpc": "2.0", "id": 2, "method": "parse",
             "params": {"path": "f.rs", "source": "fn foo() {}", "language": "rust"}},
            {"jsonrpc": "2.0", "id": 3, "method": "shutdown"},
        ]
        ndjson = "\n".join(json.dumps(m) for m in msgs) + "\n"
        responses = _run_lsp(ndjson)
        parse_resp = next(r for r in responses if r.get("id") == 2)
        assert "error" in parse_resp, "Expected error for unsupported language"
        assert parse_resp["error"]["code"] == -32602
