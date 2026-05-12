#!/usr/bin/env python3
"""
mint_option_bind_python.py -- mint concept:option-bind -> python realization (N edge).

Pattern mirrors mint_result_python.py (PR #693).
This is N-edge-only: the concept:option-bind abstraction is already on main (PR #682).

Mints:
  (A) RealizationDesugaringMemento: concept:option-bind -> python:optional-and-then  (the N edge)
  (B) MorphismDischargeReceipt for the realization attempt

The Python realization of concept:option-bind is a manual None-check pattern:
  def and_then(opt, f):
      if opt is None:
          return None
      return f(opt)

Python has no native Option/Optional monad with >>=. The standard library's
typing.Optional[T] is just T | None (a type annotation, not a runtime container).
The monadic bind is expressed as a conditional None check.

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


def build_realization_option_bind_python():
    """
    N edge: concept:option-bind -> python:optional-and-then.

    Python idiom: None-check pattern serving as monadic bind.
      def and_then(opt, f):
          if opt is None:
              return None
          return f(opt)

    Loss record (5-dimension):

    structural_divergence:
      Python has no native Option monad; typing.Optional[T] is T | None
      (a type annotation without runtime behavior); the monadic bind must
      be expressed as a user-defined function or inline if-check; the
      >>= operator does not exist in Python; bind is not a method on
      any standard type.

    domain_narrowing:
      the Python None-check conflates option:None with null/absent; programs
      where None appears as a valid T value (f could return None for a valid
      result) break the monadic abstraction; the realization domain is
      narrowed to programs where None is used exclusively as the absent marker
      and not as a valid domain value.

    ub_introduction:
      none: Python has no undefined behavior; None checks are safe; no
      memory corruption or undefined state.

    effect_divergence:
      none: the None-check pattern has no effects beyond returning a value;
      f is called at most once; no resource acquisition or I/O.

    value_divergence:
      none: bind(Some(v), f) == f(v) and bind(None, f) == None hold exactly
      under the None-check pattern; the monadic laws are satisfied for
      programs in the non-narrowed domain.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:option-bind->python:optional-and-then",
        "formals": ["T", "U"],
        "formal_sorts": [
            ctor("T"),
            ctor("U"),
        ],
        "post": {
            "lhs": op("concept:option-bind", [var("T"), var("U")]),
            "rhs": op(
                "python:optional-and-then",
                [
                    op("python:none-check-chain", [
                        {"kind": "const", "value": "if opt is None: return None", "sort": ctor("NoneGuard")},
                        {"kind": "const", "value": "return f(opt)", "sort": ctor("BindBody")},
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": (
                "Python has no native Option monad; typing.Optional[T] is T | None (type annotation only); "
                "monadic bind must be a user-defined function or inline if-check; "
                ">>= operator does not exist in Python; bind is not a method on any standard type"
            ),
            "domain_narrowing": (
                "the None-check conflates option:None with null/absent sentinel; "
                "programs where None is a valid T value break the monadic abstraction; "
                "realization domain narrowed to programs where None is used exclusively "
                "as the absent marker and not as a valid domain value"
            ),
            "ub_introduction": (
                "none: Python has no undefined behavior; "
                "None checks are always safe; no memory corruption"
            ),
            "effect_divergence": (
                "none: the None-check pattern has no effects beyond returning a value; "
                "f is called at most once; no resource acquisition or I/O"
            ),
            "value_divergence": (
                "none: bind(Some(v), f) == f(v) and bind(None, f) == None hold exactly "
                "under the None-check pattern; monadic laws satisfied in the non-narrowed domain"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


def build_discharge_receipt_option_bind_python(real_cid):
    return {
        "kind": "morphism-discharge-attempt",
        "morphism_cid": real_cid,
        "attempt_status": "pending",
        "attempt_date": "2026-05-12",
        "notes": (
            "Python realization of concept:option-bind via None-check chain pattern. "
            "Loss record fully characterized (5-dimensional). Discharge deferred to "
            "integration phase once Python lifter infrastructure is in place."
        ),
    }


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

    print("[1] Minting concept:option-bind->python:optional-and-then realization (N edge)...")
    real_memento = build_realization_option_bind_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:option-bind->python:optional-and-then.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:option-bind->python:optional-and-then: {real_cid[:40]}...")

    print("[2] Minting MorphismDischargeReceipt...")
    receipt_memento = build_discharge_receipt_option_bind_python(real_cid)
    receipt_cid = compute_cid(receipt_memento)
    receipt_path = RECEIPT_DIR / f"morphism_python_option_bind_attempt.receipt.json"
    write_json(receipt_path, {"memento": receipt_memento, "cid": receipt_cid, "signature": UNSIGNED_SIG})
    print(f"  receipt: {receipt_cid[:40]}...")

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(build_realization_option_bind_python())
    if check_cid != real_cid:
        print(f"ERROR: CID mismatch!")
        return False
    print(f"  stable: ok")

    print("\n[4] Updating cids.tsv...")
    append_cid_row("realization", "concept:option-bind->python:optional-and-then", real_cid, str(real_path))
    append_cid_row("receipt", "morphism_python_option_bind_attempt", receipt_cid, str(receipt_path))
    print("  cids.tsv updated")

    print(f"\n[DONE] realization CID: {real_cid}")
    return True


if __name__ == "__main__":
    success = mint_all()
    sys.exit(0 if success else 1)
