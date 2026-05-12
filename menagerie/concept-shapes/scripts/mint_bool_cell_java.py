#!/usr/bin/env python3
"""
mint_bool_cell_java.py -- mint concept:bool-cell -> java realization (N edge).

Pattern mirrors mint_result_java.py (PR #694).
This is N-edge-only: the concept:bool-cell abstraction is already on main (PR #681).

Mints:
  (A) RealizationDesugaringMemento: concept:bool-cell -> java:atomic-boolean  (the N edge)

The Java realization of concept:bool-cell uses java.util.concurrent.atomic.AtomicBoolean:
  AtomicBoolean cell = new AtomicBoolean(false);
  cell.set(true);       // bool-cell:set(cell, value)
  boolean val = cell.get(); // bool-cell:get(cell)

AtomicBoolean is the canonical Java mutable boolean reference with thread-safe
semantics. It satisfies the read-after-write axiom: cell.set(v); cell.get() == v.

A simpler (non-thread-safe) alternative is a one-element boolean[]:
  boolean[] cell = {false};
  cell[0] = true;
  boolean val = cell[0];

We use AtomicBoolean as the primary realization (it is more widely idiomatic
for a mutable reference cell in Java), noting the boolean[] as an alternative.

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
CATALOG_REAL = BASE / "catalog"
REAL_DIR = CATALOG_REAL / "realizations"
CID_FILE = BASE / "cids.tsv"

ROOT = BASE.parents[1]
RUST_DIR = ROOT / "implementations" / "rust"

BINARY_CANDIDATES = [
    RUST_DIR / "target" / "debug" / "compute_fixture_cid",
    Path("/Users/tsavo/provekit/implementations/rust/target/debug/compute_fixture_cid"),
]

BINARY = None
for candidate in BINARY_CANDIDATES:
    if candidate.exists():
        BINARY = candidate
        break

if BINARY is None:
    sys.exit(
        "compute_fixture_cid binary not found; "
        "run: cargo build --manifest-path implementations/rust/Cargo.toml "
        "-p provekit-canonicalizer"
    )

UNSIGNED_SIG = {
    "alg": "ed25519",
    "key_id": "UNSIGNED_DEV_ONLY",
    "sig_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
}


def compute_cid(memento):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(memento, f, ensure_ascii=True)
        f.write("\n")
        tmp = f.name
    try:
        result = subprocess.run([str(BINARY), tmp], capture_output=True, text=True)
        if result.returncode != 0:
            raise SystemExit(f"compute_fixture_cid failed: {result.stderr}")
        return result.stdout.strip()
    finally:
        os.unlink(tmp)


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(value, f, indent=2, ensure_ascii=True)
        f.write("\n")


def catalog_entry(memento):
    cid = compute_cid(memento)
    return {"memento": memento, "cid": cid, "signature": UNSIGNED_SIG}, cid


def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


def build_realization_bool_cell_java():
    return {
        "kind": "equation",
        "fn_name": "concept:bool-cell->java:atomic-boolean",
        "formals": ["cell", "value"],
        "formal_sorts": [
            ctor("BoolCell"),
            ctor("Bool"),
        ],
        "post": {
            "lhs": op("concept:bool-cell", [var("cell"), var("value")]),
            "rhs": op(
                "java:atomic-boolean",
                [
                    op("java:method-call", [
                        {"kind": "const", "value": "AtomicBoolean", "sort": ctor("ClassName")},
                        {"kind": "const", "value": "set", "sort": ctor("MethodName")},
                        var("value"),
                    ]),
                    op("java:method-call", [
                        {"kind": "const", "value": "AtomicBoolean", "sort": ctor("ClassName")},
                        {"kind": "const", "value": "get", "sort": ctor("MethodName")},
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "java",
        "loss_record": {
            "structural_divergence": (
                "AtomicBoolean is a class wrapper around a boolean primitive; "
                "set/get are method calls not direct assignment/read; "
                "the cell is heap-allocated (object reference) not a stack variable; "
                "alternative boolean[] array idiom exists for non-thread-safe use"
            ),
            "domain_narrowing": (
                "AtomicBoolean provides stronger (thread-safe) semantics than the "
                "concept:bool-cell contract requires; the realization is correct in "
                "single-threaded and concurrent programs; no domain narrowing for correctness, "
                "but AtomicBoolean has overhead vs boolean[] for single-threaded use"
            ),
            "ub_introduction": (
                "none: Java has no undefined behavior; AtomicBoolean operations are "
                "always safe; NullPointerException only if cell reference itself is null; "
                "no memory corruption"
            ),
            "effect_divergence": (
                "none: AtomicBoolean.set and get have no side effects beyond the cell itself; "
                "thread-safety is an implementation detail, not an effect in the contract sense"
            ),
            "value_divergence": (
                "none: AtomicBoolean.get() returns the exact value last set by set(v); "
                "read-after-write axiom holds: cell.set(v); cell.get() == v; "
                "no value-representation gap for boolean values"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


def append_cids_tsv(rows):
    existing_lines = []
    if CID_FILE.exists():
        existing_lines = CID_FILE.read_text(encoding="utf-8").splitlines()
    if not existing_lines:
        existing_lines = ["kind\tname\tcid\tpath"]
    seen = set()
    for line in existing_lines[1:]:
        parts = line.split("\t")
        if len(parts) >= 2:
            seen.add((parts[0], parts[1]))
    for row in rows:
        key = (row["kind"], row["name"])
        if key not in seen:
            existing_lines.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
            seen.add(key)
    CID_FILE.write_text("\n".join(existing_lines) + "\n", encoding="utf-8")


def mint_all():
    REAL_DIR.mkdir(parents=True, exist_ok=True)
    cid_rows = []

    print("[N] Minting concept:bool-cell->java:atomic-boolean realization (N edge)...")
    real_memento = build_realization_bool_cell_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:bool-cell->java:atomic-boolean.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:bool-cell->java:atomic-boolean: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:bool-cell->java:atomic-boolean", "cid": real_cid, "path": str(real_path)})

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(real_memento)
    if check_cid != real_cid:
        raise SystemExit(f"ABORTING: CID instability detected: {check_cid} != {real_cid}")
    print(f"  STABLE: realization: ok")

    append_cids_tsv(cid_rows)
    print(f"\n[DONE] realization (N edge) CID: {real_cid}")
    return real_cid


if __name__ == "__main__":
    mint_all()
