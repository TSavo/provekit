#!/usr/bin/env python3
"""
mint_unit.py -- mint concept:unit abstraction + rust lift edge + C realization.

This is the per-cell catalog scout for the abstraction layer.
Purpose: land the M+N proof empirically with receipts for concept:unit.

Mints:
  (A) ConceptAbstractionMemento for concept:unit
  (B) Lift equation: rust:() -> concept:unit  (the M edge)
  (C) RealizationDesugaringMemento: concept:unit -> c  (the N edge)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

Loss-record shape: follows PR #636's empirical wire format (string values per dimension).
PR #634 (BTreeMap<String,IrFormula>) is not yet merged; this PR documents the dependency
and will require a successor mint when #634 lands and the IrFormula encoding is standardised.
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
    concept:unit is the unit type: a type with exactly one inhabitant.

    Contract: Skolem predicate unit_singleton(self) characterizes the single
    inhabitant: there is exactly one value of type unit, and all values of
    type unit are equal.

    Formally: forall x y: concept:unit. x == y

    The contract says: self is the unique term of sort Unit; any two terms
    of sort Unit are provably equal; no information is carried.

    Slots: none (unit carries no data).

    result_sort: Unit -- the singleton type itself.
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
            "body": skolem(
                "unit_singleton",
                [
                    {"kind": "var", "name": "self"},
                ],
            ),
        },
        "contract_note": (
            "unit_singleton(self) holds iff self is the unique inhabitant of the Unit sort. "
            "All values of sort Unit are provably equal: forall x y: concept:unit. x == y. "
            "No information is carried; the type exists solely for its structural role "
            "as the identity element of product types and the return type of void-like functions."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) Lift equation: rust:() -> concept:unit  (M edge)
# ---------------------------------------------------------------------------

def build_lift_rust_unit():
    """
    M edge: rust:() -> concept:unit.

    This is an abstraction-lift equation, symmetric in structure to an
    abstraction-realization but going from language to hub rather than hub to
    language. Role: "abstraction-lift".

    Rust's () (unit type) is a native zero-sized type with exactly one value,
    also written (). It is a first-class type: it can appear as a function
    return type, in generics, and as a struct field. The lift is zero-loss:
      - structural_divergence: empty (Rust's () IS the canonical model)
      - domain_narrowing: empty (all well-typed () values lift exactly)
      - ub_introduction: empty (unit values are always safe to construct)
      - effect_divergence: empty (() itself is pure)

    The lift is dischargeable via canonicalizer-alpha-equivalence: the
    Rust-side IR for () maps directly to unit_singleton(self) under the
    trivial representation map { () |-> Unit }.

    This provides the empirical M edge: the lift CID is the content-addressed
    proof that rust:() is a valid source-side instantiation of the hub.
    """
    return {
        "kind": "equation",
        "fn_name": "rust:()->concept:unit",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("rust:()", []),
            "rhs": op("concept:unit", []),
        },
        "role": "abstraction-lift",
        "direction": "left-to-right",
        "source_lang": "rust",
        "loss_record": {
            "structural_divergence": (
                "empty: Rust () is the canonical zero-sized unit type; "
                "no structural encoding gap between the source and the hub concept"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:unit -> c  (N edge)
# ---------------------------------------------------------------------------

def build_realization_unit_c():
    """
    N edge: concept:unit -> c (empty struct typedef).

    The C realization encodes concept:unit as an empty struct typedef:
      typedef struct {} unit_t;
      #define UNIT ((unit_t){})

    Pre-C23 fallback (for compilers that reject empty structs as an extension):
      typedef int unit_t;
      #define UNIT 0

    This file documents the C23-compliant primary form.

    Loss record (concrete, not placeholder):

    structural_divergence:
      C (pre-C23) does not natively support empty structs as a standard
      feature; empty struct is a GCC/Clang extension. The C23 standard
      permits empty structs. The realization uses a typedef to name the
      type and a macro to construct the single value; the concept's
      singleton is encoded as a zero-sized aggregate, not a language
      primitive. The type is not truly zero-sized in all C ABIs (some
      compilers give it size 1); this is a representational gap absent
      from the hub concept.

    domain_narrowing: none -- the singleton property is preserved; there
      is exactly one value of type unit_t under any reasonable ABI.
      Unlike concept:option, no conditional check is required and no
      arm can be missed.

    ub_introduction: none -- constructing UNIT is always safe; the
      empty struct compound literal has well-defined behaviour under C99+
      with the GCC empty-struct extension and under C23 without any
      extension.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:unit->c:empty-struct-typedef",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("concept:unit", []),
            "rhs": op(
                "c:empty-struct-typedef",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "typedef struct {} unit_t; #define UNIT ((unit_t){})", "sort": ctor("MacroName")},
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "empty_struct_replaces_native_unit_type: "
                "C has no native unit/void-as-value type; the realization uses "
                "typedef struct {} unit_t with a UNIT macro for the single value; "
                "pre-C23 compilers treat empty structs as a GCC/Clang extension; "
                "some ABIs assign size 1 to empty structs rather than size 0"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
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
    ABST_DIR.mkdir(parents=True, exist_ok=True)
    REAL_DIR.mkdir(parents=True, exist_ok=True)

    cid_rows = []

    # Step 1: compute realization CID first so abstraction can reference it.
    print("[B] Minting rust:()->concept:unit lift equation (M edge)...")
    lift_memento = build_lift_rust_unit()
    lift_entry, lift_cid = catalog_entry(lift_memento)
    lift_path = REAL_DIR / f"rust:()->concept:unit.{lift_cid}.json"
    write_json(lift_path, lift_entry)
    print(f"  rust:()->concept:unit: {lift_cid[:40]}...")
    cid_rows.append({"kind": "lift-equation", "name": "rust:()->concept:unit", "cid": lift_cid, "path": str(lift_path)})

    print("[C] Minting concept:unit->c:empty-struct-typedef realization (N edge)...")
    real_memento = build_realization_unit_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:unit->c:empty-struct-typedef.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:unit->c:empty-struct-typedef: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:unit->c:empty-struct-typedef", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:unit abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_unit()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:unit.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:unit: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:unit", "cid": abst_cid, "path": str(abst_path)})

    # Stability check: mint each artifact a second time and compare.
    print("\n[STABILITY] Re-minting all artifacts for byte-stability check...")
    stable = True
    for check_name, memento, expected_cid in [
        ("abstraction", abst_memento, abst_cid),
        ("lift-equation", lift_memento, lift_cid),
        ("realization", real_memento, real_cid),
    ]:
        check_cid = compute_cid(memento)
        if check_cid != expected_cid:
            print(f"  UNSTABLE: {check_name}: {check_cid} != {expected_cid}")
            stable = False
        else:
            print(f"  STABLE: {check_name}: ok")
    if not stable:
        raise SystemExit("ABORTING: CID instability detected. Fix canonical key order.")

    append_cids_tsv(cid_rows)

    print("\n[DONE] Minted CIDs:")
    print(f"  abstraction CID:     {abst_cid}")
    print(f"  lift (M edge) CID:   {lift_cid}")
    print(f"  realize (N edge) CID:{real_cid}")

    return abst_cid, lift_cid, real_cid


if __name__ == "__main__":
    mint_all()
