#!/usr/bin/env python3
"""
mint_unit_java.py -- mint concept:unit -> java realization (N edge).

Pattern mirrors mint_result_java.py (PR #694).
This is N-edge-only: the concept:unit abstraction is already on main (PR #684).

Mints:
  (A) RealizationDesugaringMemento: concept:unit -> java:void-return  (the N edge)

The Java realization of concept:unit uses void / Void.

Java's void is the natural unit type for methods: a void method returns no
meaningful value, just as concept:unit carries no information. The boxed
type java.lang.Void (capital V) is the unit type as a first-class object:
it has exactly one assignable value, null (Void cannot be instantiated).

Realization name: java:void-return (canonical idiom: void return type)

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).

Loss record (5-dimension):
  structural_divergence: void is a keyword not a type; Void is a class but non-instantiable (only null)
  domain_narrowing: Void can hold null which is NOT the unit singleton; requires discipline
  ub_introduction: none; no memory UB in Java
  effect_divergence: none; returning void/Void carries no effects
  value_divergence: Void's unique "value" is null (not a real instance), diverging from unit_singleton
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
# (A) RealizationDesugaringMemento: concept:unit -> java:void-return
# ---------------------------------------------------------------------------

def build_realization_unit_java():
    """
    N edge: concept:unit -> java:void-return.

    Java realization: void (primitive return type) and java.lang.Void (boxed).

    In Java, void is the return type of methods with no meaningful result.
    The boxed java.lang.Void (capital V) is the unit type as a first-class
    object: it cannot be instantiated (private constructor), so its only
    "value" in variable bindings is null.

    For the concept:unit realization, void is the structural match:
    - A void method returns concept:unit (no information carried)
    - Void is used in generic contexts (Callable<Void>, Future<Void>)

    Loss record (5-dimension):

    structural_divergence:
      void is a keyword (not a type), so it cannot be used as a type parameter;
      java.lang.Void bridges this gap but is non-instantiable; the split
      between void (return position) and Void (type position) is a structural
      divergence from concept:unit which is a single consistent type;
      the realization requires choosing void or Void depending on syntactic context.

    domain_narrowing:
      java.lang.Void can hold null, which does not satisfy the unit_singleton
      contract (the contract requires a real instantiated value, not null);
      programs passing null as the "unit value" in Void positions are in the
      narrowed-out domain; the realization is correct only when null is treated
      as the unit value by discipline, not by type enforcement.

    ub_introduction:
      none: Java has no undefined behavior; NullPointerException from misusing
      Void is a structured exception; no memory corruption or undefined state.

    effect_divergence:
      none: returning void or Void carries no effects; the unit contract does
      not prescribe effects; no divergence.

    value_divergence:
      Void's unique "value" is null (the only assignable value), which is not
      a real instance of Void; unit_singleton requires a proper inhabitant;
      the use of null as the unit value is a value representation divergence
      in generic contexts (where null is not the same as an actual unit instance).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:unit->java:void-return",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("concept:unit", []),
            "rhs": op(
                "java:void-return",
                [
                    {
                        "kind": "const",
                        "value": "void",
                        "sort": ctor("VoidKeyword"),
                    },
                    {
                        "kind": "const",
                        "value": "java.lang.Void",
                        "sort": ctor("BoxedVoid"),
                    },
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "java",
        "loss_record": {
            "structural_divergence": (
                "void is a keyword (not a type), cannot be used as a type parameter; "
                "java.lang.Void bridges this but is non-instantiable; "
                "the void/Void split diverges from concept:unit as a single consistent type; "
                "realization requires choosing void or Void based on syntactic context"
            ),
            "domain_narrowing": (
                "java.lang.Void can hold null, which does not satisfy unit_singleton; "
                "programs passing null as the unit value in Void positions are narrowed-out; "
                "correct only when null is treated as the unit value by discipline, "
                "not by type enforcement"
            ),
            "ub_introduction": (
                "none: Java has no undefined behavior; "
                "NullPointerException from Void misuse is a structured exception; "
                "no memory corruption or undefined state"
            ),
            "effect_divergence": (
                "none: returning void or Void carries no effects; "
                "unit contract does not prescribe effects; no divergence"
            ),
            "value_divergence": (
                "Void's unique assignable value is null (not a real Void instance); "
                "unit_singleton requires a proper inhabited value; "
                "using null as the unit value is a value-representation divergence "
                "in generic contexts where null is not the same as an actual unit instance"
            ),
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

    print("[N] Minting concept:unit->java:void-return realization (N edge)...")
    real_memento = build_realization_unit_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:unit->java:void-return.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:unit->java:void-return: {real_cid[:40]}...")
    cid_rows.append(
        {
            "kind": "realization",
            "name": "concept:unit->java:void-return",
            "cid": real_cid,
            "path": str(real_path),
        }
    )

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(real_memento)
    if check_cid != real_cid:
        print(f"  UNSTABLE: {check_cid} != {real_cid}")
        raise SystemExit("ABORTING: CID instability detected.")
    print(f"  STABLE: realization: ok")

    append_cids_tsv(cid_rows)

    print(f"\n[DONE] Minted CID:")
    print(f"  realization (N edge) CID: {real_cid}")

    return real_cid


if __name__ == "__main__":
    mint_all()
