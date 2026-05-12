#!/usr/bin/env python3
"""
mint_identity_python.py -- mint Python realization for concept:identity.

Extends the existing concept:identity abstraction (C realization on main, decbdea8)
with a Python realization. Python idiom for identity is lambda x: x or def identity(x): return x.

Mints:
  (A) RealizationDesugaringMemento: concept:identity -> python  (the N edge)

The Python realization is zero-loss on all five dimensions:
  - structural_divergence: empty (lambda x: x is the canonical no-op)
  - domain_narrowing: empty (all Python values are valid)
  - ub_introduction: empty (no undefined behavior)
  - effect_divergence: empty (lambda is pure)
  - value_divergence: empty (no value gap)

All CIDs are BLAKE3-512 via compute_fixture_cid.
Discharge receipt is null (PR1 form: no prior state to discharge against).
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
# RealizationDesugaringMemento: concept:identity -> python
# ---------------------------------------------------------------------------

def build_realization_identity_python():
    """
    N edge: concept:identity -> python (lambda x: x or def identity(x): return x).

    The Python realization encodes concept:identity as a simple lambda function.
    Two common idiomatic forms exist:
      - Lambda form: identity = lambda x: x
      - Function form: def identity(x): return x

    This is a zero-overhead, type-generic, trivially-correct realization.
    Python's lambda or def form is the canonical no-op function.

    Loss record: all dimensions are trivially true (zero-loss cell).

    concept:identity is the one cell whose projection distance is exactly 0:
      - No structural encoding gap (lambda x: x IS the argument itself)
      - No domain narrowing (valid for all Python objects)
      - No UB introduction (no undefined behavior, all values valid)
      - No effect divergence (lambda is pure, no side effects)
      - No value divergence (no value-representation gap)

    The realization choice between lambda vs def is a convention matter;
    both are byte-identical in behavior (the serialized bytecode is identical).
    The lambda form is the more idiomatic Python zero-loss cell.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:identity->python:lambda",
        "formals": ["x"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("concept:identity", [var("x")]),
            "rhs": op(
                "python:lambda",
                [
                    {
                        "kind": "const",
                        "value": "lambda x: x",
                        "sort": ctor("LambdaExpr"),
                    },
                    var("x"),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": True,
            "domain_narrowing": True,
            "ub_introduction": True,
            "effect_divergence": True,
            "value_divergence": True,
        },
        "discharge_receipt": None,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# Main mint flow
# ---------------------------------------------------------------------------

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

    print("[1] Minting concept:identity->python:lambda realization (N edge)...")
    real_memento = build_realization_identity_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:identity->python:lambda.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:identity->python:lambda: {real_cid[:40]}...")
    cid_rows.append({
        "kind": "realization",
        "name": "concept:identity->python:lambda",
        "cid": real_cid,
        "path": str(real_path),
    })

    # Stability check: mint artifact a second time and compare.
    print("\n[STABILITY] Re-minting artifact for byte-stability check...")
    stable = True
    check_cid = compute_cid(real_memento)
    if check_cid != real_cid:
        print(f"  UNSTABLE: realization: {check_cid} != {real_cid}")
        stable = False
    else:
        print(f"  STABLE: realization: ok")
    if not stable:
        raise SystemExit("ABORTING: CID instability detected. Fix canonical key order.")

    append_cids_tsv(cid_rows)

    print("\n[DONE] Minted CIDs:")
    print(f"  realize (N edge) CID: {real_cid}")

    return real_cid


if __name__ == "__main__":
    mint_all()
