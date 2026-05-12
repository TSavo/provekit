#!/usr/bin/env python3
"""
mint_result_bind_python.py -- mint concept:result-bind -> python realization (N edge).

Pattern mirrors mint_result_python.py (PR #693).
This is N-edge-only: the concept:result-bind abstraction is already on main (PR #683).

Mints:
  (A) RealizationDesugaringMemento: concept:result-bind -> python:result-bind-if-ok  (the N edge)
  (B) MorphismDischargeReceipt for the realization attempt

The Python realization of concept:result-bind uses a conditional ok-check pattern:
  def result_bind(result, f):
      if result.is_ok:
          return f(result.value)
      return result

This pairs with the Python concept:result realization (dataclass-tagged-union)
from PR #693. The bind function follows the monadic laws:
  bind(Ok(v), f) == f(v)
  bind(Err(e), f) == Err(e)

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


def build_realization_result_bind_python():
    """
    N edge: concept:result-bind -> python:result-bind-if-ok.

    Python idiom: conditional ok-check serving as monadic bind.
      def result_bind(result, f):
          if result.is_ok:
              return f(result.value)
          return result

    Pairs with concept:result -> python:dataclass-tagged-union (PR #693).
    Monadic bind laws:
      bind(Ok(v), f) == f(v)    # if result.is_ok: return f(result.value)
      bind(Err(e), f) == Err(e) # return result (unchanged)

    Loss record (5-dimension):

    structural_divergence:
      Python has no native Result monad; the bind function is user-defined;
      there is no >>= operator in Python; the is_ok discriminator field
      is specific to the Python dataclass-tagged-union realization (PR #693);
      the bind pattern depends on the structural shape of that realization.

    domain_narrowing:
      the realization domain is narrowed to programs using the
      dataclass-tagged-union Result from concept:result->python (PR #693);
      programs using other Result representations (e.g., exceptions, tuples)
      cannot use this bind directly without adaptation; the domain is narrowed
      to the specific dataclass shape with is_ok/value/error fields.

    ub_introduction:
      none: Python has no undefined behavior; accessing result.value when
      is_ok is True is safe given the invariant of the dataclass-tagged-union
      representation; no memory corruption.

    effect_divergence:
      none: the bind function is a pure conditional with no effects beyond
      calling f once (at most); f is called only when result.is_ok; no
      resource acquisition or I/O introduced by bind itself.

    value_divergence:
      none: bind(Ok(v), f) == f(v) and bind(Err(e), f) == Err(e) hold exactly
      under the is_ok pattern; error type E is preserved on the Err branch;
      monadic associativity law holds for well-formed results.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:result-bind->python:result-bind-if-ok",
        "formals": ["result", "f"],
        "formal_sorts": [
            ctor("T"),
            ctor("E"),
            ctor("U"),
        ],
        "post": {
            "lhs": op("concept:result-bind", [var("result"), var("f")]),
            "rhs": op(
                "python:result-bind-if-ok",
                [
                    op("python:if-ok-branch", [
                        {"kind": "const", "value": "if result.is_ok: return f(result.value)", "sort": ctor("OkBranch")},
                        {"kind": "const", "value": "return result", "sort": ctor("ErrBranch")},
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": (
                "Python has no native Result monad; bind is user-defined; "
                "no >>= operator; is_ok discriminator is specific to the "
                "dataclass-tagged-union realization (PR #693); "
                "bind depends on that realization's structural shape"
            ),
            "domain_narrowing": (
                "realization domain narrowed to programs using the dataclass-tagged-union "
                "Result from concept:result->python (PR #693); "
                "other Result representations (exceptions, tuples) require adaptation; "
                "domain is the specific dataclass shape with is_ok/value/error fields"
            ),
            "ub_introduction": (
                "none: Python has no undefined behavior; "
                "accessing result.value when is_ok is True is safe given the invariant; "
                "no memory corruption"
            ),
            "effect_divergence": (
                "none: bind is a pure conditional; f is called at most once; "
                "no resource acquisition or I/O introduced by bind itself"
            ),
            "value_divergence": (
                "none: bind(Ok(v), f) == f(v) and bind(Err(e), f) == Err(e) hold exactly; "
                "error type E preserved on Err branch; "
                "monadic associativity holds for well-formed results"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


def build_discharge_receipt_result_bind_python(real_cid):
    return {
        "kind": "morphism-discharge-attempt",
        "morphism_cid": real_cid,
        "attempt_status": "pending",
        "attempt_date": "2026-05-12",
        "notes": (
            "Python realization of concept:result-bind via if-ok conditional pattern. "
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

    print("[1] Minting concept:result-bind->python:result-bind-if-ok realization (N edge)...")
    real_memento = build_realization_result_bind_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:result-bind->python:result-bind-if-ok.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:result-bind->python:result-bind-if-ok: {real_cid[:40]}...")

    print("[2] Minting MorphismDischargeReceipt...")
    receipt_memento = build_discharge_receipt_result_bind_python(real_cid)
    receipt_cid = compute_cid(receipt_memento)
    receipt_path = RECEIPT_DIR / f"morphism_python_result_bind_attempt.receipt.json"
    write_json(receipt_path, {"memento": receipt_memento, "cid": receipt_cid, "signature": UNSIGNED_SIG})
    print(f"  receipt: {receipt_cid[:40]}...")

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(build_realization_result_bind_python())
    if check_cid != real_cid:
        print(f"ERROR: CID mismatch!")
        return False
    print(f"  stable: ok")

    print("\n[4] Updating cids.tsv...")
    append_cid_row("realization", "concept:result-bind->python:result-bind-if-ok", real_cid, str(real_path))
    append_cid_row("receipt", "morphism_python_result_bind_attempt", receipt_cid, str(receipt_path))
    print("  cids.tsv updated")

    print(f"\n[DONE] realization CID: {real_cid}")
    return True


if __name__ == "__main__":
    success = mint_all()
    sys.exit(0 if success else 1)
