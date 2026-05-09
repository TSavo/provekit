#!/usr/bin/env python3
"""Walk a kernel subtree, lift every .c file via the JSON-RPC C lifter, ingest
callEdges + functions into a SQLite substrate.db keyed by file path.

Usage: lift_kernel.py <kernel_root> <subdirs...> <substrate.db>
Example: lift_kernel.py /tmp/linux-source/linux net/ipv4 net/ipv6 net/rxrpc /tmp/substrate-barrage/substrate.db
"""
import json
import pathlib
import sqlite3
import subprocess
import sys
import time

LIFTER = "/tmp/provekit-substrate/implementations/c/provekit-lift-c-kernel-doc/provekit-lift-c-kernel-doc"


def init_db(db_path: pathlib.Path) -> sqlite3.Connection:
    db_path.unlink(missing_ok=True)
    db = sqlite3.connect(str(db_path))
    db.executescript(
        """
        CREATE TABLE call_edges (
            caller_function TEXT NOT NULL,
            callee_name     TEXT NOT NULL,
            args            TEXT,
            callsite_path   TEXT NOT NULL,
            callsite_line   INTEGER NOT NULL,
            callsite_column INTEGER NOT NULL
        );
        CREATE INDEX idx_call_edges_callee ON call_edges(callee_name);
        CREATE INDEX idx_call_edges_caller ON call_edges(caller_function);
        CREATE INDEX idx_call_edges_path   ON call_edges(callsite_path);

        CREATE TABLE lifted_files (
            path        TEXT PRIMARY KEY,
            edge_count  INTEGER NOT NULL,
            lift_ms     INTEGER NOT NULL
        );
        """
    )
    return db


def lift_file(rel_path: str, source: str, lifter: str = LIFTER, timeout: float = 30.0) -> dict:
    """Run a single parse RPC. Returns the result dict (declarations, callEdges, ...)."""
    init = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
    req = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "parse",
            "params": {
                "path": rel_path,
                "parse_backend": "clang_ast",
                "source": source,
            },
        }
    )
    proc = subprocess.run(
        [lifter, "--rpc"],
        input=f"{init}\n{req}\n",
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    out_lines = [ln for ln in proc.stdout.splitlines() if ln.strip()]
    for line in out_lines:
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        if obj.get("id") == 2:
            return obj.get("result", {})
    return {}


def main(argv: list[str]) -> int:
    if len(argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2
    kernel_root = pathlib.Path(argv[1]).resolve()
    db_path = pathlib.Path(argv[-1]).resolve()
    subdirs = argv[2:-1]

    targets: list[pathlib.Path] = []
    for sub in subdirs:
        for p in (kernel_root / sub).rglob("*.c"):
            targets.append(p)
    targets.sort()

    db = init_db(db_path)
    cur = db.cursor()
    started = time.time()

    for i, src_path in enumerate(targets, 1):
        rel = src_path.relative_to(kernel_root).as_posix()
        try:
            text = src_path.read_text(errors="replace")
        except OSError as exc:
            print(f"  skip {rel}: {exc}", file=sys.stderr)
            continue
        t0 = time.time()
        try:
            result = lift_file(rel, text)
        except subprocess.TimeoutExpired:
            print(f"  TIMEOUT {rel}", file=sys.stderr)
            continue
        elapsed_ms = int((time.time() - t0) * 1000)
        edges = result.get("callEdges", [])
        norm = []
        for e in edges:
            if isinstance(e, str):
                e = json.loads(e)
            norm.append(e)
        cur.executemany(
            "INSERT INTO call_edges (caller_function, callee_name, args, callsite_path, callsite_line, callsite_column) VALUES (?,?,?,?,?,?)",
            [
                (
                    e["caller_function"],
                    e["callee_name"],
                    json.dumps(e.get("args", [])),
                    e["callsite_path"],
                    e["callsite_line"],
                    e["callsite_column"],
                )
                for e in norm
            ],
        )
        cur.execute(
            "INSERT INTO lifted_files (path, edge_count, lift_ms) VALUES (?, ?, ?)",
            (rel, len(norm), elapsed_ms),
        )
        if i % 25 == 0 or i == len(targets):
            db.commit()
            print(f"  [{i:4}/{len(targets)}] {rel:60} +{len(norm):4} edges ({elapsed_ms}ms)")
    db.commit()
    elapsed = int(time.time() - started)
    total_edges = cur.execute("SELECT COUNT(*) FROM call_edges").fetchone()[0]
    total_files = cur.execute("SELECT COUNT(*) FROM lifted_files").fetchone()[0]
    print(f"\nLifted {total_files} files, {total_edges} call edges in {elapsed}s -> {db_path}")
    db.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
