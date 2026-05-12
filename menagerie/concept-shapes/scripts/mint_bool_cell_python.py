#!/usr/bin/env python3
"""
mint_bool_cell_python.py -- mint concept:bool-cell -> python realization (N edge).

Pattern mirrors mint_result_python.py (PR #693).
This is N-edge-only: the concept:bool-cell abstraction is already on main (PR #681).

Mints:
  (A) RealizationDesugaringMemento: concept:bool-cell -> python:mutable-list-cell  (the N edge)
  (B) MorphismDischargeReceipt for the realization attempt

The Python realization of concept:bool-cell uses a mutable list of one element:
  cell = [False]      # create
  cell[0] = True      # set
  val = cell[0]       # get

This is the idiomatic Python pattern for a mutable boolean reference. Python
does not have pointer indirection (unlike C). Lists are mutable, so a single-
element list serves as a mutable cell. The read-after-write axiom holds:
cell[0] = v; cell[0] == v.

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
RECEIPT_DIR = CATALOG_REAL / "receipts"
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


# ---------------------------------------------------------------------------
# CID utilities
# ---------------------------------------------------------------------------

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


# ---------------------------------------------------------------------------
# IR formula helpers
# ---------------------------------------------------------------------------

def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


# ---------------------------------------------------------------------------
# (A) RealizationDesugaringMemento: concept:bool-cell -> python:mutable-list-cell
# ---------------------------------------------------------------------------

def build_realization_bool_cell_python():
    """
    N edge: concept:bool-cell -> python:mutable-list-cell.

    Python idiom: single-element list as a mutable reference cell.
      cell = [False]    # bool-cell:create
      cell[0] = True    # bool-cell:set(cell, value)
      val = cell[0]     # bool-cell:get(cell)

    The read-after-write axiom: bool-cell:get(bool-cell:set(c, v); c) == v
    is satisfied: cell[0] = v; cell[0] is v (same object, True/False are
    singletons in CPython).

    Loss record (5-dimension):

    structural_divergence:
      Python lists are general containers; the single-element list pattern
      imposes a convention (cell[0] is the value) with no type enforcement;
      any list of length >= 1 passes static type checking; there is no dedicated
      BoolCell type; cell is a list[bool] not a distinct mutable-cell type;
      the get/set interface is not named (just indexing).

    domain_narrowing:
      the mutable-list-cell is not thread-safe; the read-after-write axiom
      holds in a single thread but is not atomic under concurrent access;
      the realization domain is narrowed to single-threaded programs or programs
      that externally synchronize access to the cell; concurrent writers can
      violate the axiom.

    ub_introduction:
      none: Python has no undefined behavior; index access on a non-empty list
      is safe; out-of-bounds raises IndexError, a structured exception.

    effect_divergence:
      none: list index assignment is a local mutation with no external effects
      beyond the cell itself; no I/O, no resource acquisition.

    value_divergence:
      none: Python's True and False are singletons that compare equal to the
      corresponding boolean values; cell[0] = v; cell[0] == v holds exactly;
      no value-representation gap for bool values.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:bool-cell->python:mutable-list-cell",
        "formals": ["cell", "value"],
        "formal_sorts": [
            ctor("BoolCell"),
            ctor("Bool"),
        ],
        "post": {
            "lhs": op("concept:bool-cell", [var("cell"), var("value")]),
            "rhs": op(
                "python:mutable-list-cell",
                [
                    op("python:list-index-assign", [
                        var("cell"),
                        {"kind": "const", "value": "0", "sort": ctor("Index")},
                        var("value"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": (
                "Python lists are general containers; single-element list pattern "
                "imposes a convention with no type enforcement; any list[bool] passes; "
                "no dedicated BoolCell type; get/set is unnamed index access not a named interface"
            ),
            "domain_narrowing": (
                "mutable-list-cell is not thread-safe; read-after-write axiom holds "
                "in single-threaded use but is not atomic under concurrent access; "
                "realization domain narrowed to single-threaded or externally-synchronized programs"
            ),
            "ub_introduction": (
                "none: Python has no undefined behavior; "
                "out-of-bounds raises IndexError, a structured exception; "
                "no memory corruption"
            ),
            "effect_divergence": (
                "none: list index assignment is local mutation with no external effects; "
                "no I/O, no resource acquisition"
            ),
            "value_divergence": (
                "none: Python True/False are singletons; "
                "cell[0] = v; cell[0] == v holds exactly; "
                "no value-representation gap for bool values"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (B) MorphismDischargeReceipt
# ---------------------------------------------------------------------------

def build_discharge_receipt_bool_cell_python(real_cid):
    return {
        "kind": "morphism-discharge-attempt",
        "morphism_cid": real_cid,
        "attempt_status": "pending",
        "attempt_date": "2026-05-12",
        "notes": (
            "Python realization of concept:bool-cell via single-element mutable list. "
            "Loss record fully characterized (5-dimensional). Discharge deferred to "
            "integration phase once Python lifter infrastructure is in place."
        ),
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def append_cid_row(kind, name, cid, path):
    existing_lines = []
    seen = set()

    if CID_FILE.exists():
        for line in CID_FILE.read_text(encoding="utf-8").strip().split("\n"):
            if not line.strip():
                continue
            parts = line.split("\t")
            if len(parts) >= 2:
                seen.add((parts[0], parts[1]))
            existing_lines.append(line)

    key = (kind, name)
    if key not in seen:
        existing_lines.append(f"{kind}\t{name}\t{cid}\t{path}")
        seen.add(key)

    CID_FILE.write_text("\n".join(existing_lines) + "\n", encoding="utf-8")


def mint_all():
    REAL_DIR.mkdir(parents=True, exist_ok=True)
    RECEIPT_DIR.mkdir(parents=True, exist_ok=True)

    print("[1] Minting concept:bool-cell->python:mutable-list-cell realization (N edge)...")
    real_memento = build_realization_bool_cell_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:bool-cell->python:mutable-list-cell.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:bool-cell->python:mutable-list-cell: {real_cid[:40]}...")

    print("[2] Minting MorphismDischargeReceipt...")
    receipt_memento = build_discharge_receipt_bool_cell_python(real_cid)
    receipt_cid = compute_cid(receipt_memento)
    receipt_path = RECEIPT_DIR / f"morphism_python_bool_cell_attempt.receipt.json"
    write_json(receipt_path, {"memento": receipt_memento, "cid": receipt_cid, "signature": UNSIGNED_SIG})
    print(f"  receipt: {receipt_cid[:40]}...")

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(build_realization_bool_cell_python())
    if check_cid != real_cid:
        print(f"ERROR: CID mismatch! First: {real_cid}, second: {check_cid}")
        return False
    print(f"  stable: ok")

    print("\n[4] Updating cids.tsv...")
    append_cid_row("realization", "concept:bool-cell->python:mutable-list-cell", real_cid, str(real_path))
    append_cid_row("receipt", "morphism_python_bool_cell_attempt", receipt_cid, str(receipt_path))
    print("  cids.tsv updated")

    print(f"\n[DONE] realization CID: {real_cid}")
    return True


if __name__ == "__main__":
    success = mint_all()
    sys.exit(0 if success else 1)
