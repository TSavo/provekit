#!/usr/bin/env python3
"""
mint_unit_python.py -- mint concept:unit -> python realization (N edge).

Pattern mirrors mint_result_python.py (PR #693).
This is N-edge-only: the concept:unit abstraction is already on main (PR #684).

Mints:
  (A) RealizationDesugaringMemento: concept:unit -> python:none-singleton  (the N edge)
  (B) MorphismDischargeReceipt for the realization attempt

The Python realization of concept:unit uses None (the NoneType singleton):
  None  # the unique inhabitant of NoneType

Python's None is the canonical zero-information value and serves the same
structural role as concept:unit: identity of product types, return value of
void-like functions.

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).

Loss record (5-dimension):
  structural_divergence: None is a named constant, not a type constructor; NoneType is not a user-definable type
  domain_narrowing: None can be used as a null sentinel (is None checks), mixing unit and optional semantics
  ub_introduction: none; Python has no UB
  effect_divergence: none; None carries no effects
  value_divergence: none; None is the unique inhabitant, matching the unit_singleton contract
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
# (A) RealizationDesugaringMemento: concept:unit -> python:none-singleton
# ---------------------------------------------------------------------------

def build_realization_unit_python():
    """
    N edge: concept:unit -> python:none-singleton.

    Python realization: None, the unique value of NoneType.

    Python's None satisfies the unit_singleton contract: it is the unique
    inhabitant of NoneType, carries no information, and serves as the
    identity element of product types. Functions with no meaningful return
    value return None implicitly.

    Loss record (5-dimension):

    structural_divergence:
      None is a named constant (a singleton object), not a type constructor;
      NoneType is not user-definable or instantiable; Python does not have
      a distinct unit type separate from None; the realization conflates
      the type (NoneType) with its unique value (None); there is no explicit
      syntax for "the Unit type" distinct from the value None.

    domain_narrowing:
      None is overloaded in Python as both a unit value and a null/absent
      sentinel; code using None to mean "no value" (Optional semantics) and
      code using it to mean "unit return" are syntactically indistinguishable;
      the realization domain is narrowed to programs where None is used
      strictly in the unit role, not the null-sentinel role; mixing both
      uses in the same context violates the unit_singleton contract.

    ub_introduction:
      none: Python has no undefined behavior; None is a safe, well-defined
      singleton; no memory corruption or undefined state.

    effect_divergence:
      none: None carries no effects; returning None from a function is
      side-effect-free with respect to the unit contract; no divergence.

    value_divergence:
      none: None is the unique inhabitant of NoneType, directly satisfying
      unit_singleton(None); the value representation gap is zero; all
      values of NoneType are provably equal (None == None always True).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:unit->python:none-singleton",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("concept:unit", []),
            "rhs": op(
                "python:none-singleton",
                [
                    {
                        "kind": "const",
                        "value": "None",
                        "sort": ctor("NoneType"),
                    }
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": (
                "None is a named constant (singleton object), not a type constructor; "
                "NoneType is not user-definable or instantiable; "
                "Python conflates the unit type with its unique value; "
                "no explicit 'Unit type' syntax distinct from the value None"
            ),
            "domain_narrowing": (
                "None is overloaded as both unit value and null/absent sentinel; "
                "unit use and null-sentinel use are syntactically indistinguishable; "
                "realization domain narrowed to programs where None is used in "
                "the unit role only; mixing both uses violates unit_singleton contract"
            ),
            "ub_introduction": (
                "none: Python has no undefined behavior; "
                "None is a safe, well-defined singleton; "
                "no memory corruption or undefined state"
            ),
            "effect_divergence": (
                "none: None carries no effects; "
                "returning None is side-effect-free with respect to the unit contract"
            ),
            "value_divergence": (
                "none: None is the unique inhabitant of NoneType, "
                "satisfying unit_singleton(None); "
                "all values of NoneType are provably equal; "
                "no value-representation gap"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (B) MorphismDischargeReceipt
# ---------------------------------------------------------------------------

def build_discharge_receipt_unit_python(real_cid):
    return {
        "kind": "morphism-discharge-attempt",
        "morphism_cid": real_cid,
        "attempt_status": "pending",
        "attempt_date": "2026-05-12",
        "notes": (
            "Python realization of concept:unit via None singleton. "
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

    print("[1] Minting concept:unit->python:none-singleton realization (N edge)...")
    real_memento = build_realization_unit_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:unit->python:none-singleton.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:unit->python:none-singleton: {real_cid[:40]}...")

    print("[2] Minting MorphismDischargeReceipt...")
    receipt_memento = build_discharge_receipt_unit_python(real_cid)
    receipt_cid = compute_cid(receipt_memento)
    receipt_path = RECEIPT_DIR / f"morphism_python_unit_attempt.receipt.json"
    write_json(receipt_path, {"memento": receipt_memento, "cid": receipt_cid, "signature": UNSIGNED_SIG})
    print(f"  receipt: {receipt_cid[:40]}...")

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(build_realization_unit_python())
    if check_cid != real_cid:
        print(f"ERROR: CID mismatch! First: {real_cid}, second: {check_cid}")
        return False
    print(f"  stable: ok")

    print("\n[4] Updating cids.tsv...")
    append_cid_row("realization", "concept:unit->python:none-singleton", real_cid, str(real_path))
    append_cid_row("receipt", "morphism_python_unit_attempt", receipt_cid, str(receipt_path))
    print("  cids.tsv updated")

    print(f"\n[DONE] realization CID: {real_cid}")
    return True


if __name__ == "__main__":
    success = mint_all()
    sys.exit(0 if success else 1)
