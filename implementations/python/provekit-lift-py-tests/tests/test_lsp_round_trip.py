# SPDX-License-Identifier: Apache-2.0
#
# LSP plugin round-trip test (#221).
#
# Spawns the Python LSP plugin (`python -m provekit_lift_py_tests.lsp`) as a
# subprocess, drives the NDJSON-over-stdio protocol end to end, and asserts
# the response shape per `protocol/specs/2026-04-30-lsp-protocol.md` and the
# plugin handshake described in `provekit-lsp/src/plugin.rs`:
#
#   1. initialize -> name/version/capabilities
#   2. parse      -> declarations + warnings
#   3. shutdown   -> null result, clean exit
#
# This is the single test that proves the binary actually speaks the protocol;
# unit tests on individual handlers do not.

from __future__ import annotations

import json
import os
import subprocess
import sys

import pytest


HERE = os.path.dirname(os.path.abspath(__file__))
PKG_ROOT = os.path.abspath(os.path.join(HERE, "..", "src"))


def _spawn_plugin() -> subprocess.Popen:
    env = os.environ.copy()
    env["PYTHONPATH"] = PKG_ROOT + os.pathsep + env.get("PYTHONPATH", "")
    return subprocess.Popen(
        [sys.executable, "-m", "provekit_lift_py_tests.lsp"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        text=True,
        bufsize=1,
    )


def _exchange(proc: subprocess.Popen, payload: dict) -> dict:
    line = json.dumps(payload, separators=(",", ":"))
    assert proc.stdin is not None and proc.stdout is not None
    proc.stdin.write(line + "\n")
    proc.stdin.flush()
    resp_line = proc.stdout.readline()
    assert resp_line, f"plugin closed stdout; stderr={proc.stderr.read() if proc.stderr else ''}"
    return json.loads(resp_line)


def test_lsp_plugin_round_trip():
    proc = _spawn_plugin()
    try:
        # 1. initialize ----------------------------------------------------
        init = _exchange(
            proc,
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        )
        assert init["jsonrpc"] == "2.0"
        assert init["id"] == 1
        assert "result" in init, f"initialize returned error: {init}"
        result = init["result"]
        assert result["name"] == "provekit-lsp-python"
        assert isinstance(result.get("version"), str) and result["version"]
        assert "parse" in result.get("capabilities", [])

        # 2. parse ---------------------------------------------------------
        sample = (
            "# //provekit:contract\n"
            "def add(a, b):\n"
            "    return a + b\n"
        )
        parse = _exchange(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "parse",
                "params": {"path": "sample.py", "source": sample},
            },
        )
        assert parse["jsonrpc"] == "2.0"
        assert parse["id"] == 2
        assert "result" in parse, f"parse returned error: {parse}"
        parse_result = parse["result"]
        # Per protocol: `declarations` + `warnings` keys.
        assert "declarations" in parse_result
        assert "warnings" in parse_result
        assert isinstance(parse_result["warnings"], list)

        # 3. shutdown ------------------------------------------------------
        shut = _exchange(
            proc, {"jsonrpc": "2.0", "id": 3, "method": "shutdown"}
        )
        assert shut["id"] == 3
        assert shut["result"] is None

        # Plugin must exit cleanly within a reasonable window.
        try:
            rc = proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            pytest.fail("plugin did not exit after shutdown")
        assert rc == 0, f"plugin exited with {rc}"
    finally:
        if proc.poll() is None:
            proc.kill()


def test_lsp_plugin_unknown_method_returns_jsonrpc_error():
    proc = _spawn_plugin()
    try:
        # initialize so we are past the handshake
        _exchange(
            proc,
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        )
        bad = _exchange(
            proc,
            {"jsonrpc": "2.0", "id": 2, "method": "no_such_method"},
        )
        assert "error" in bad
        # JSON-RPC 2.0 method-not-found code.
        assert bad["error"]["code"] == -32601

        # Plugin still responds to shutdown after an error.
        shut = _exchange(
            proc, {"jsonrpc": "2.0", "id": 3, "method": "shutdown"}
        )
        assert shut["result"] is None
        proc.wait(timeout=5)
    finally:
        if proc.poll() is None:
            proc.kill()
