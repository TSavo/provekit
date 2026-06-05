# SPDX-License-Identifier: Apache-2.0
#
# provekit editor LSP: a REAL editor language server for Python.
#
# This is the "red squiggle" gap, closed. The batch lift plugin (`lsp.py`) speaks
# the provekit-lift/1 NDJSON protocol the CLI spawns per invocation; it is NOT a
# server an editor holds open. THIS module is: a persistent stdio server speaking
# the Language Server Protocol base wire format (Content-Length-framed JSON-RPC),
# so an editor that opens a Python file gets diagnostics back.
#
# The diagnostic IS the `provekit prove` verdict. On didOpen/didSave the server
# evaluates the document's project AS IT IS ON DISK and turns every unsatisfied
# obligation into an LSP Diagnostic with severity Error -- the red squiggle. The
# headline case: a consumer that asserts `np.add(2,3) == 6` while inheriting
# `== 5` from a vendor `.proof` in `.provekit/imports/`. `prove` reports that
# callsite UNSAT (the conjoined contracts contradict); the squiggle lands on the
# offending call, and CLEARS when the source is fixed to `== 5`.
#
# Because `prove` reads minted `.proof` artifacts (not source), the server mints
# the current source into a FRESH isolated workspace and proves that -- so the
# squiggle tracks the buffer on save, not stale mint output, and never writes to
# the user's tree. See `run_prove_report`.
#
# TRUST BOUNDARY: verification lives in the rust CLI, exactly as everywhere else.
# This server is a thin client: it spawns `provekit prove`, reads its machine
# report, and renders it. It never decides correctness itself. The prove-runner
# is injectable so the wire protocol is testable without the toolchain.
#
# This is DISTINCT from the rust `provekit-lsp` crate, which is the cross-language
# linker-daemon-routed editor server (roadmap). This one is the Python-kit-native
# server that renders `provekit prove` directly.

from __future__ import annotations

import ast
import json
import os
import re
import shutil
import subprocess
import sys
from typing import Any, BinaryIO, Dict, List, Optional, Tuple

# LSP DiagnosticSeverity. 1 = Error (the red squiggle).
SEVERITY_ERROR = 1
SEVERITY_WARNING = 2

SERVER_NAME = "provekit-editor-lsp-python"
SERVER_VERSION = "0.1.0"
DIAGNOSTIC_SOURCE = "provekit"


# ---------------------------------------------------------------------------
# LSP base wire protocol: Content-Length-framed JSON-RPC over stdio.
# ---------------------------------------------------------------------------


def read_message(stream: BinaryIO) -> Optional[dict]:
    """Read one LSP message: `Content-Length: N\\r\\n` headers, blank line, N
    bytes of JSON. Returns None at EOF (the editor closed the pipe)."""
    headers: Dict[str, str] = {}
    while True:
        line = stream.readline()
        if not line:
            return None
        line = line.rstrip(b"\r\n")
        if line == b"":
            break  # end of headers
        if b":" in line:
            key, _, val = line.partition(b":")
            headers[key.strip().decode("ascii").lower()] = val.strip().decode("ascii")
    length = int(headers.get("content-length", "0"))
    if length <= 0:
        return None
    body = stream.read(length)
    if len(body) < length:
        return None
    try:
        return json.loads(body.decode("utf-8"))
    except json.JSONDecodeError:
        return None


def write_message(stream: BinaryIO, obj: dict) -> None:
    """Frame and write one LSP message."""
    body = json.dumps(obj, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    stream.write(b"Content-Length: " + str(len(body)).encode("ascii") + b"\r\n\r\n")
    stream.write(body)
    stream.flush()


# ---------------------------------------------------------------------------
# Anchoring: a prove violation -> a source range in THIS document.
#
# The verifier's consistency rows carry `file: null, line: null` (a conjoined
# contradiction is a property of a callsite term, not a single source line). The
# anchor lives in the `property` string: `consistency:<callee>#euf#...(<args>)...`.
# We decode the callee + literal int args and find the matching call node in the
# document's AST -- that call's line is where the squiggle belongs.
# ---------------------------------------------------------------------------

_PROPERTY_CALLEE = re.compile(r"^(?:consistency:)?(?P<callee>[A-Za-z_][\w.]*)#euf#")
_PROPERTY_ARGS = re.compile(r"\(([^)]*)\)")
_INT_ARG = re.compile(r"i:(-?\d+)")


def _decode_property(property_str: str) -> Optional[Tuple[str, List[int]]]:
    """Decode `consistency:numpy.add#euf#c:callresult_numpy_add_a2(i:2,i:3)::assertion`
    into ("numpy.add", [2, 3]). Returns None when the shape is not a callsite EUF
    term (e.g. a whole-test property we cannot anchor to a specific call)."""
    m = _PROPERTY_CALLEE.match(property_str)
    if not m:
        return None
    callee = m.group("callee")
    args: List[int] = []
    arg_match = _PROPERTY_ARGS.search(property_str)
    if arg_match:
        args = [int(x) for x in _INT_ARG.findall(arg_match.group(1))]
    return callee, args


def _module_aliases(tree: ast.Module) -> Dict[str, str]:
    """Map the names bound by imports to their module: `import numpy as np` ->
    {np: numpy}; `import numpy` -> {numpy: numpy}; `import a.b` binds top-level
    `a` -> a (Python binds the top package, not the submodule)."""
    aliases: Dict[str, str] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.asname:
                    aliases[alias.asname] = alias.name
                else:
                    top = alias.name.split(".")[0]
                    aliases[top] = top
        elif isinstance(node, ast.ImportFrom) and node.module:
            for alias in node.names:
                bound = alias.asname or alias.name
                aliases[bound] = f"{node.module}.{alias.name}"
    return aliases


def _qualified_callee(func: ast.expr, aliases: Dict[str, str]) -> Optional[str]:
    """Resolve a call's func expression to a dotted callee, applying import
    aliases at the head: `np.add` -> `numpy.add`."""
    parts: List[str] = []
    cur: Optional[ast.expr] = func
    while isinstance(cur, ast.Attribute):
        parts.append(cur.attr)
        cur = cur.value
    if isinstance(cur, ast.Name):
        head = aliases.get(cur.id, cur.id)
        parts.append(head)
    else:
        return None
    parts.reverse()
    return ".".join(parts)


def _int_args(call: ast.Call) -> Optional[List[int]]:
    """The call's positional args IF they are all integer literals (matching the
    EUF arg signature `(i:2,i:3)`). Returns None when any arg isn't an int."""
    out: List[int] = []
    for arg in call.args:
        if isinstance(arg, ast.Constant) and isinstance(arg.value, int) and not isinstance(arg.value, bool):
            out.append(arg.value)
        elif (
            isinstance(arg, ast.UnaryOp)
            and isinstance(arg.op, ast.USub)
            and isinstance(arg.operand, ast.Constant)
            and isinstance(arg.operand.value, int)
        ):
            out.append(-arg.operand.value)
        else:
            return None
    return out


def _find_callsites(tree: ast.Module, callee: str, args: List[int]) -> List[ast.Call]:
    """Every call in the document whose resolved callee + integer args match."""
    aliases = _module_aliases(tree)
    hits: List[ast.Call] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if _qualified_callee(node.func, aliases) != callee:
            continue
        if args and _int_args(node) != args:
            continue
        hits.append(node)
    return hits


def _range_of(node: ast.AST) -> Dict[str, Dict[str, int]]:
    """LSP Range (0-based lines/characters) covering an AST node."""
    start_line = max(getattr(node, "lineno", 1) - 1, 0)
    start_char = getattr(node, "col_offset", 0)
    end_line = max(getattr(node, "end_lineno", getattr(node, "lineno", 1)) - 1, 0)
    end_char = getattr(node, "end_col_offset", start_char)
    return {
        "start": {"line": start_line, "character": start_char},
        "end": {"line": end_line, "character": end_char},
    }


def _whole_line_range(line_1based: int) -> Dict[str, Dict[str, int]]:
    line = max(line_1based - 1, 0)
    return {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 200}}


# ---------------------------------------------------------------------------
# The analysis: prove report + document text -> LSP diagnostics.
# ---------------------------------------------------------------------------


def syntax_diagnostics(text: str, path: str) -> List[Dict[str, Any]]:
    try:
        ast.parse(text, filename=path)
        return []
    except SyntaxError as e:
        line = e.lineno or 1
        char = max((e.offset or 1) - 1, 0)
        return [
            {
                "range": {
                    "start": {"line": max(line - 1, 0), "character": char},
                    "end": {"line": max(line - 1, 0), "character": char + 1},
                },
                "severity": SEVERITY_ERROR,
                "source": DIAGNOSTIC_SOURCE,
                "code": "provekit.parse_error",
                "message": str(e),
            }
        ]


def _row_is_violation(row: dict) -> bool:
    status = str(row.get("status", ""))
    return status not in ("discharged", "ok", "vacuous", "")


def prove_diagnostics(text: str, path: str, report: dict) -> List[Dict[str, Any]]:
    """Turn a `provekit prove --json` report into diagnostics for THIS document.

    A violation row's callsite is decoded from its `property` and matched against
    calls in this file. Only rows whose call lives here produce a squiggle (prove
    is project-wide; a diagnostic belongs in the file holding the offending call).
    """
    try:
        tree = ast.parse(text, filename=path)
    except SyntaxError:
        return []  # syntax errors are reported separately; can't anchor on a broken tree

    diagnostics: List[Dict[str, Any]] = []
    for row in report.get("rows", []):
        if not _row_is_violation(row):
            continue
        message = str(row.get("reason") or row.get("property") or "obligation unsatisfied")
        code = str(row.get("property") or "provekit.unsatisfied")
        decoded = _decode_property(str(row.get("property", "")))

        ranges: List[Dict[str, Dict[str, int]]] = []
        # 1. The producer told us a concrete line: trust it.
        if row.get("file") and row.get("line"):
            same_file = os.path.basename(str(row["file"])) == os.path.basename(path)
            if same_file:
                ranges.append(_whole_line_range(int(row["line"])))
        # 2. Otherwise recover the callsite from the document AST.
        if not ranges and decoded is not None:
            callee, args = decoded
            ranges = [_range_of(c) for c in _find_callsites(tree, callee, args)]

        for rng in ranges:
            diagnostics.append(
                {
                    "range": rng,
                    "severity": SEVERITY_ERROR,
                    "source": DIAGNOSTIC_SOURCE,
                    "code": code,
                    "message": message,
                }
            )
    return diagnostics


def diagnostics_for(
    text: str,
    path: str,
    *,
    prove_report: Optional[dict] = None,
) -> List[Dict[str, Any]]:
    """All diagnostics for a document. Syntax errors always; prove-backed
    squiggles when a report is supplied (syntax errors suppress prove, since a
    broken parse has no trustworthy callsites)."""
    syn = syntax_diagnostics(text, path)
    if syn:
        return syn
    if prove_report is None:
        return []
    return prove_diagnostics(text, path, prove_report)


# ---------------------------------------------------------------------------
# The prove runner (the injectable trust boundary).
# ---------------------------------------------------------------------------


def find_project_root(path: str) -> Optional[str]:
    """Walk up from a file to the nearest directory containing `.provekit/`."""
    cur = os.path.dirname(os.path.abspath(path))
    while True:
        if os.path.isdir(os.path.join(cur, ".provekit")):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            return None
        cur = parent


def _provekit_binary() -> Optional[str]:
    env = os.environ.get("PROVEKIT_CLI")
    if env and os.path.exists(env):
        return env
    return shutil.which("provekit")


def _parse_report(stdout: str) -> Optional[dict]:
    for chunk in (stdout, stdout.strip()):
        try:
            return json.loads(chunk)
        except (json.JSONDecodeError, TypeError):
            continue
    start = stdout.find("{")  # a pretty header may precede the JSON
    if start >= 0:
        try:
            return json.loads(stdout[start:])
        except json.JSONDecodeError:
            return None
    return None


def run_prove_report(path: str) -> Optional[dict]:
    """Evaluate the document's project AS IT IS ON DISK NOW and return the
    `provekit prove --json` report.

    `prove` reads MINTED `.proof` artifacts, not source -- so a bare `prove` would
    report the last mint, and the squiggle would lag the edit (and re-minting in
    place accumulates stale CID-named proofs that re-introduce the contradiction).
    Instead we mint the current source into a FRESH isolated workspace and prove
    THAT: the project's config + inherited `.provekit/imports/` proofs are copied
    in, `mint` lifts the live source into the temp dir, and `prove` runs against
    only those. The user's tree is never written to, and there is no stale
    accumulation -- the report reflects exactly what is on disk.

    Returns None when there is no project, no CLI, or mint/prove cannot run -- the
    editor shows no provekit diagnostics rather than a wrong one.
    """
    root = find_project_root(path)
    if root is None:
        return None
    binary = _provekit_binary()
    if binary is None:
        return None
    config = os.path.join(root, ".provekit", "config.toml")
    if not os.path.isfile(config):
        return None

    import tempfile

    workspace = tempfile.mkdtemp(prefix="provekit-lsp-")
    try:
        ws_provekit = os.path.join(workspace, ".provekit")
        ws_imports = os.path.join(ws_provekit, "imports")
        os.makedirs(ws_imports, exist_ok=True)
        shutil.copy(config, os.path.join(ws_provekit, "config.toml"))
        src_imports = os.path.join(root, ".provekit", "imports")
        if os.path.isdir(src_imports):
            for name in os.listdir(src_imports):
                if name.endswith(".proof"):
                    shutil.copy(os.path.join(src_imports, name), os.path.join(ws_imports, name))
        try:
            # mint reads the project's source + manifests in place (cwd=root) and
            # writes the freshly-lifted proof into the isolated workspace.
            mint = subprocess.run(
                [binary, "mint", "--out", workspace, "--quiet"],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=120,
            )
            if mint.returncode != 0:
                return None
            prove = subprocess.run(
                [binary, "prove", "--json", workspace],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=120,
            )
        except (OSError, subprocess.TimeoutExpired):
            return None
        return _parse_report(prove.stdout)
    finally:
        shutil.rmtree(workspace, ignore_errors=True)


# ---------------------------------------------------------------------------
# The server.
# ---------------------------------------------------------------------------


def _uri_to_path(uri: str) -> str:
    if uri.startswith("file://"):
        rest = uri[len("file://"):]
        # file:///abs/path -> /abs/path
        return rest if rest.startswith("/") else rest
    return uri


class Server:
    """A persistent LSP server. `prove_runner` is injected so the wire protocol
    is testable without the rust toolchain (the default runs the real CLI)."""

    def __init__(self, instream: BinaryIO, outstream: BinaryIO, prove_runner=run_prove_report):
        self._in = instream
        self._out = outstream
        self._prove = prove_runner
        self._docs: Dict[str, str] = {}
        self._shutdown = False

    # -- transport ---------------------------------------------------------

    def _notify(self, method: str, params: dict) -> None:
        write_message(self._out, {"jsonrpc": "2.0", "method": method, "params": params})

    def _reply(self, msg_id: Any, result: Any) -> None:
        write_message(self._out, {"jsonrpc": "2.0", "id": msg_id, "result": result})

    # -- diagnostics -------------------------------------------------------

    def _publish(self, uri: str, *, run_prove: bool) -> None:
        text = self._docs.get(uri, "")
        path = _uri_to_path(uri)
        report = self._prove(path) if run_prove else None
        diags = diagnostics_for(text, path, prove_report=report)
        self._notify("textDocument/publishDiagnostics", {"uri": uri, "diagnostics": diags})

    # -- lifecycle ---------------------------------------------------------

    def _on_initialize(self, msg_id: Any) -> None:
        self._reply(
            msg_id,
            {
                "capabilities": {
                    # 1 = full document sync (the editor sends the whole text).
                    "textDocumentSync": {"openClose": True, "change": 1, "save": True},
                },
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            },
        )

    def handle(self, msg: dict) -> bool:
        """Dispatch one message. Returns False when the server should stop."""
        method = msg.get("method")
        msg_id = msg.get("id")
        params = msg.get("params") or {}

        if method == "initialize":
            self._on_initialize(msg_id)
        elif method == "initialized":
            pass  # notification, no reply
        elif method == "textDocument/didOpen":
            doc = params.get("textDocument", {})
            uri = doc.get("uri", "")
            self._docs[uri] = doc.get("text", "")
            self._publish(uri, run_prove=True)
        elif method == "textDocument/didChange":
            uri = params.get("textDocument", {}).get("uri", "")
            changes = params.get("contentChanges", [])
            if changes:
                # Full-sync: the last change carries the whole document.
                self._docs[uri] = changes[-1].get("text", self._docs.get(uri, ""))
            # On edit, refresh syntax only (cheap, live); prove-backed squiggles
            # refresh on save, against the on-disk project.
            self._publish(uri, run_prove=False)
        elif method == "textDocument/didSave":
            uri = params.get("textDocument", {}).get("uri", "")
            if "text" in params:
                self._docs[uri] = params["text"]
            self._publish(uri, run_prove=True)
        elif method == "textDocument/didClose":
            uri = params.get("textDocument", {}).get("uri", "")
            self._docs.pop(uri, None)
            self._notify("textDocument/publishDiagnostics", {"uri": uri, "diagnostics": []})
        elif method == "shutdown":
            self._shutdown = True
            self._reply(msg_id, None)
        elif method == "exit":
            return False
        elif msg_id is not None:
            # Unknown request: reply with a MethodNotFound error so the client
            # isn't left waiting.
            write_message(
                self._out,
                {
                    "jsonrpc": "2.0",
                    "id": msg_id,
                    "error": {"code": -32601, "message": f"method not found: {method}"},
                },
            )
        return True

    def serve_forever(self) -> int:
        while True:
            msg = read_message(self._in)
            if msg is None:
                break
            if not self.handle(msg):
                break
        return 0 if self._shutdown else 1


def main() -> None:
    server = Server(sys.stdin.buffer, sys.stdout.buffer)
    sys.exit(server.serve_forever())


if __name__ == "__main__":
    main()
