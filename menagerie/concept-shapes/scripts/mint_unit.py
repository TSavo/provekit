#!/usr/bin/env python3
"""
mint_unit.py -- mint concept:unit abstraction + C realization.

Purpose: mint the smallest type-cell — the unit type with exactly one inhabitant.

Mints:
  (A) ConceptAbstractionMemento for concept:unit (no type params)
  (B) RealizationDesugaringMemento: concept:unit -> c  (the N edge)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

Loss-record shape: follows PR #636's empirical wire format (string values per dimension).

Contract: forall x y: concept:unit. x == y (the unit type has exactly one inhabitant).
"""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
CATALOG_REAL = BASE / "catalog"
ABST_DIR = CATALOG_REAL / "abstractions"
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

DEFERRED_RECEIPT = "deferred:pending-61-PR5"
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
# IR formula helpers (matching discharge.py / mint_trinity.py conventions)
# ---------------------------------------------------------------------------

def true_formula():
    return {"kind": "atomic", "name": "true", "args": []}


def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def skolem(predicate, args):
    return {"kind": "skolem", "predicate": predicate, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


# ---------------------------------------------------------------------------
# (A) ConceptAbstractionMemento: concept:unit
# ---------------------------------------------------------------------------

def build_abstraction_unit():
    """
    concept:unit is the type with exactly one inhabitant.

    Contract: the single-value theorem — forall x y: concept:unit. x == y.
    There is only one value of type unit; all inhabitants are identical.

    Slots: (empty — unit carries no data)

    result_sort: Unit -- the unit type itself.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:unit",
        "tier": "abstraction",
        "slots": [],
        "formal_sorts": [],
        "result_sort": "Unit",
        "contract": {
            "kind": "wp-rule",
            "formals": [],
            "body": true_formula(),
        },
        "contract_note": (
            "unit_inhabitant holds unconditionally; the unit type has exactly one inhabitant. "
            "All values of type unit are identical by construction."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (N) RealizationDesugaringMemento: concept:unit -> c
# ---------------------------------------------------------------------------

def build_realization_unit_c():
    """
    N edge: concept:unit -> c (void or empty struct).

    The C realization uses the most conservative approach: void as the realization.
    Alternatively, typedef struct {} unit_t; with #define UNIT ((unit_t){}) is the
    struct-of-no-fields approach, but requires C23 support. void is maximally portable.

    Loss record (concrete):

    structural_divergence:
      The abstraction is a simple singleton value.
      The C realization uses void (the no-type), which is distinct from
      any expressible value in C. The "value" of unit becomes a no-op in C.
      The caller cannot manipulate, pass, or return void in normal contexts;
      void is a language-level fiction used only in function return types and
      generic pointer args. This is a significant structural gap: the abstraction
      allows unit as a first-class value; C forbids it. Workaround: wrapper
      in a struct or function with no-arg convention. Fallback: typedef int unit_t;
      #define UNIT 0 trades the structural gap for a distinct-but-valid int value.

    domain_narrowing:
      C cannot pass void by value. Programs that instantiate unit as a value
      (e.g., fn() -> unit) must wrap or use a sentinel. The abstraction permits
      unit as a genuine type; C forbids bare void values. The realization narrows
      to programs that respect this C limitation.

    ub_introduction:
      None; void is not UB, but it is semantically empty. No field access,
      no value manipulation. The abstraction's single inhabitant maps to a
      language-level no-op in C.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:unit->c:void",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("concept:unit", []),
            "rhs": op("c:void", []),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "void replaces native unit type: "
                "C void is a no-type fiction that cannot be instantiated as a value; "
                "the abstraction permits unit as a first-class type; "
                "C forbids void by-value semantics; "
                "workaround: wrapper struct or sentinel-based convention"
            ),
            "domain_narrowing": (
                "C cannot pass or return void by value; "
                "programs must wrap the realization in struct or use function-call convention; "
                "the abstraction's direct unit values narrow to wrapped or indirect patterns in C"
            ),
            "ub_introduction": "none; void is not UB, only semantically empty",
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# Main: mint and catalog
# ---------------------------------------------------------------------------

def main():
    # Clean and recreate catalog dirs
    import shutil
    if CATALOG_REAL.exists():
        shutil.rmtree(CATALOG_REAL)
    ABST_DIR.mkdir(parents=True, exist_ok=True)
    REAL_DIR.mkdir(parents=True, exist_ok=True)

    # (A) Mint abstraction
    print("Minting concept:unit abstraction...")
    abst_memento = build_abstraction_unit()
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  CID: {abst_cid}")
    print(f"  Path: {abst_path}")

    # (N) Mint realization
    print("Minting concept:unit -> c:void realization...")
    real_memento = build_realization_unit_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  CID: {real_cid}")
    print(f"  Path: {real_path}")

    # (A+N) Update abstraction to include realization CID
    print("Updating abstraction with realization CID...")
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid_updated = catalog_entry(abst_memento)
    if abst_cid_updated != abst_cid:
        # Remove old file, write new one
        abst_path.unlink()
        abst_path = ABST_DIR / f"{abst_cid_updated}.json"
        write_json(abst_path, abst_entry)
        abst_cid = abst_cid_updated
        print(f"  Updated CID: {abst_cid}")
    else:
        write_json(abst_path, abst_entry)
        print(f"  CID unchanged: {abst_cid}")

    # Report CIDs
    print("\n=== MINT SUMMARY ===")
    print(f"Abstraction CID: {abst_cid}")
    print(f"Realization CID: {real_cid}")

    # Write cids.tsv (similar to mint_option.py)
    with open(CID_FILE, "a", encoding="utf-8") as f:
        f.write(f"concept\tunits\t{abst_cid}\t{abst_path}\n")
        f.write(f"equation\tunits_c\t{real_cid}\t{real_path}\n")

    print(f"\nCIDs appended to: {CID_FILE}")
    return [abst_cid, real_cid]


if __name__ == "__main__":
    cids = main()
    print("\nMint complete. Byte-stable CIDs:")
    for cid in cids:
        print(f"  {cid}")
