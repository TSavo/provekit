#!/usr/bin/env python3
"""
mint_assert_python.py -- mint concept:assert -> python realization (N edge).

Pattern mirrors mint_result_python.py (PR #693) and mint_result_java.py (PR #694).
This is N-edge-only: the concept:assert abstraction is already on main (PR #685).

Mints:
  (A) RealizationDesugaringMemento: concept:assert -> python:assert-statement  (the N edge)
  (B) MorphismDischargeReceipt for the realization attempt

The Python realization of concept:assert uses the built-in assert statement:
  assert pred, "message"

Python's assert statement maps directly to concept:assert's held(p) contract.
Key constraint: python -O (optimized mode) strips assert statements entirely,
which means the contract no longer holds in optimized builds.

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).

Loss record (5-dimension):
  structural_divergence: assert is a statement, not a function call; no custom abort handler
  domain_narrowing: python -O disables all asserts; realization domain excludes -O builds
  ub_introduction: none; Python has no UB; AssertionError is a structured exception
  effect_divergence: Python raises AssertionError (catchable) vs C abort() (uncatchable)
  value_divergence: none; predicate slot maps directly to assert condition
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
# (A) RealizationDesugaringMemento: concept:assert -> python:assert-statement
# ---------------------------------------------------------------------------

def build_realization_assert_python():
    """
    N edge: concept:assert -> python:assert-statement.

    Python realization: the built-in assert statement.
      assert pred, "assertion failed"

    The assert statement is the natural Python idiom for the held(p) contract.
    It raises AssertionError (a subclass of Exception) if pred is False.

    Loss record (5-dimension):

    structural_divergence:
      assert is a statement syntactic form, not a function call; it cannot be
      stored in a variable or passed as a first-class value; the assert keyword
      takes an expression directly rather than a callable; there is no hook to
      customize the abort handler (short of replacing the whole AssertionError
      handler at the interpreter level).

    domain_narrowing:
      python -O (optimized mode, __debug__ == False) strips all assert statements
      at compile time; in -O builds the held(p) postcondition is never checked;
      the realization domain is narrowed to programs run without -O; production
      deployments using -O are excluded.

    ub_introduction:
      none: Python has no undefined behavior; the assert statement either
      passes silently or raises AssertionError, a structured catchable exception;
      no memory corruption or undefined state is introduced.

    effect_divergence:
      Python assert raises AssertionError, a catchable exception, unlike the
      concept:assert specification which implies program termination (abort
      semantics); a caller could catch AssertionError and continue execution,
      violating the held(p) postcondition; the effect is divergent when
      AssertionError is caught in a surrounding except clause.

    value_divergence:
      none: the predicate slot maps directly to the assert condition; no
      value-representation gap between concept:assert(pred) and assert pred.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:assert->python:assert-statement",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("concept:assert", []),
            "rhs": op(
                "python:assert-statement",
                [
                    {
                        "kind": "const",
                        "value": "assert pred, \"assertion failed\"",
                        "sort": ctor("AssertStmt"),
                    }
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": (
                "assert is a statement syntactic form, not a function call; "
                "it cannot be stored in a variable or passed as a first-class value; "
                "assert takes an expression directly rather than a callable; "
                "no hook to customize the abort handler"
            ),
            "domain_narrowing": (
                "python -O strips all assert statements at compile time; "
                "in optimized builds the held(p) postcondition is never checked; "
                "realization domain is narrowed to programs run without -O; "
                "production deployments using -O are excluded from the domain"
            ),
            "ub_introduction": (
                "none: Python has no undefined behavior; assert raises AssertionError, "
                "a structured catchable exception; no memory corruption or undefined state"
            ),
            "effect_divergence": (
                "Python assert raises AssertionError (catchable exception) instead of "
                "terminating the program; a surrounding except clause could catch "
                "AssertionError and continue execution, violating the held(p) postcondition; "
                "effect diverges when AssertionError is caught"
            ),
            "value_divergence": (
                "none: predicate slot maps directly to assert condition; "
                "no value-representation gap between concept:assert(pred) and assert pred"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (B) MorphismDischargeReceipt
# ---------------------------------------------------------------------------

def build_discharge_receipt_assert_python(real_cid):
    return {
        "kind": "morphism-discharge-attempt",
        "morphism_cid": real_cid,
        "attempt_status": "pending",
        "attempt_date": "2026-05-12",
        "notes": (
            "Python realization of concept:assert via built-in assert statement. "
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

    print("[1] Minting concept:assert->python:assert-statement realization (N edge)...")
    real_memento = build_realization_assert_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:assert->python:assert-statement.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:assert->python:assert-statement: {real_cid[:40]}...")

    print("[2] Minting MorphismDischargeReceipt...")
    receipt_memento = build_discharge_receipt_assert_python(real_cid)
    receipt_cid = compute_cid(receipt_memento)
    receipt_path = RECEIPT_DIR / f"morphism_python_assert_attempt.receipt.json"
    write_json(receipt_path, {"memento": receipt_memento, "cid": receipt_cid, "signature": UNSIGNED_SIG})
    print(f"  receipt: {receipt_cid[:40]}...")

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(build_realization_assert_python())
    if check_cid != real_cid:
        print(f"ERROR: CID mismatch! First: {real_cid}, second: {check_cid}")
        return False
    print(f"  stable: ok")

    print("\n[4] Updating cids.tsv...")
    append_cid_row("realization", "concept:assert->python:assert-statement", real_cid, str(real_path))
    append_cid_row("receipt", "morphism_python_assert_attempt", receipt_cid, str(receipt_path))
    print("  cids.tsv updated")

    print(f"\n[DONE] realization CID: {real_cid}")
    return True


if __name__ == "__main__":
    success = mint_all()
    sys.exit(0 if success else 1)
