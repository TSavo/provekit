#!/usr/bin/env python3
"""Fire every .sql predicate in predicates/ against the lifted substrate,
collect the result sets, print a tight summary.

Usage: run_predicates.py <substrate.db> [<predicates_dir>]
"""
import hashlib
import pathlib
import sqlite3
import sys


def cid(b: bytes) -> str:
    return "blake3-512:" + hashlib.blake2b(b, digest_size=64).hexdigest()


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    db_path = pathlib.Path(argv[1]).resolve()
    pred_dir = pathlib.Path(argv[2] if len(argv) > 2 else "/tmp/substrate-barrage/predicates").resolve()

    db = sqlite3.connect(str(db_path))
    db.row_factory = sqlite3.Row

    total_edges = db.execute("SELECT COUNT(*) FROM call_edges").fetchone()[0]
    total_files = db.execute("SELECT COUNT(*) FROM lifted_files").fetchone()[0]
    print(f"Substrate: {total_files} files, {total_edges} call edges, db={db_path}\n")

    sqls = sorted(pred_dir.glob("*.sql"))
    if not sqls:
        print(f"No .sql files in {pred_dir}", file=sys.stderr)
        return 1

    for sql_path in sqls:
        text = sql_path.read_text()
        pred_cid = cid(text.encode())
        try:
            rows = db.execute(text).fetchall()
        except sqlite3.OperationalError as exc:
            print(f"=== {sql_path.name} ===")
            print(f"  ERROR: {exc}\n")
            continue
        print(f"=== {sql_path.name} ===")
        print(f"  predicate CID: {pred_cid[:48]}...")
        print(f"  matches: {len(rows)}")
        if rows:
            keys = list(rows[0].keys())
            for r in rows[:25]:
                vals = [str(r[k]) for k in keys]
                print(f"    " + "  ".join(f"{k}={v}" for k, v in zip(keys, vals)))
            if len(rows) > 25:
                print(f"    ... and {len(rows) - 25} more")
        print()
    db.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
