#!/usr/bin/env python3
"""
mint_bool_cell.py -- mint concept:bool-cell abstraction + C realization.

Mints:
  (A) ConceptAbstractionMemento for concept:bool-cell
  (C) RealizationDesugaringMemento: concept:bool-cell -> c:pointer-indirection  (N edge)

The C realization is:
  typedef bool *bool_cell_t;
  #define BOOL_CELL_GET(c)    (*(c))
  #define BOOL_CELL_SET(c, v) ((*(c)) = (v))
  #define BOOL_CELL_NEW()     ((bool_cell_t)malloc(sizeof(bool)))

Contract: BOOL_CELL_GET(BOOL_CELL_SET(c, v); c) == v

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

Loss-record shape: BTreeMap<String, IrFormula> per PR #634.
Values are IrFormula objects (kind: "atomic") encoding the loss dimension description.
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


def atomic(name):
    """Loss-record IrFormula: atomic predicate with no args, name encodes the description."""
    return {"kind": "atomic", "name": name, "args": []}


# ---------------------------------------------------------------------------
# (A) ConceptAbstractionMemento: concept:bool-cell
# ---------------------------------------------------------------------------

def build_abstraction_bool_cell():
    """
    concept:bool-cell is a mutable cell holding a boolean value.

    Contract: BOOL_CELL_GET(BOOL_CELL_SET(c, v); c) == v

    The contract says: writing value v into cell c and then reading c yields v.
    This is the standard cell read-after-write axiom.

    Slots:
      - cell: the mutable reference to the boolean storage.
      - value: the boolean value to store or retrieve.

    result_sort: BoolCell -- the cell type itself.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:bool-cell",
        "tier": "abstraction",
        "slots": [
            {"name": "cell"},
            {"name": "value"},
        ],
        "formal_sorts": [
            "Bool",
        ],
        "result_sort": "BoolCell",
        "contract": {
            "kind": "wp-rule",
            "formals": ["cell", "value"],
            "body": op(
                "eq",
                [
                    op("bool-cell:get", [
                        op("seq", [
                            op("bool-cell:set", [var("cell"), var("value")]),
                            var("cell"),
                        ]),
                    ]),
                    var("value"),
                ],
            ),
        },
        "contract_note": (
            "Read-after-write axiom: BOOL_CELL_GET(BOOL_CELL_SET(c, v); c) == v. "
            "Writing v to cell c and immediately reading c returns v. "
            "The cell is a mutable reference; aliasing and concurrency are out of scope."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:bool-cell -> c:pointer-indirection  (N edge)
# ---------------------------------------------------------------------------

def build_realization_bool_cell_c():
    """
    N edge: concept:bool-cell -> c:pointer-indirection.

    The C realization encodes concept:bool-cell as a heap-allocated pointer:
      typedef bool *bool_cell_t;
      #define BOOL_CELL_GET(c)    (*(c))
      #define BOOL_CELL_SET(c, v) ((*(c)) = (v))
      #define BOOL_CELL_NEW()     ((bool_cell_t)malloc(sizeof(bool)))

    Loss record (BTreeMap<String, IrFormula> per PR #634):

    structural_divergence:
      The abstraction is a native mutable cell with direct read/write semantics.
      The C realization uses pointer indirection: the cell is a heap-allocated
      bool accessed via dereference macros. There is no native mutable cell
      primitive in C; the pointer-plus-macro pattern replaces it.

    effect_divergence:
      The abstraction's BOOL_CELL_NEW() requires heap allocation (malloc).
      The concept abstraction does not specify an allocation model; the C
      realization introduces a mandatory heap dependency that the concept
      does not require.

    ub_introduction:
      Dereferencing a bool_cell_t after the backing memory has been freed
      is undefined behaviour in C. The concept abstraction has no notion
      of memory lifetime; the C realization introduces use-after-free UB
      on any access through a freed pointer.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:bool-cell->c:pointer-indirection",
        "formals": ["cell", "value"],
        "formal_sorts": [
            "Bool",
        ],
        "post": {
            "lhs": op("concept:bool-cell", [var("cell"), var("value")]),
            "rhs": op(
                "c:pointer-indirection",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "bool_cell_t", "sort": ctor("MacroName")},
                        var("cell"),
                        var("value"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "effect_divergence": atomic(
                "requires_heap_allocation: "
                "BOOL_CELL_NEW() introduces a mandatory malloc dependency; "
                "the concept abstraction does not specify an allocation model"
            ),
            "structural_divergence": atomic(
                "pointer_indirection_replaces_native_mutable_cell: "
                "the abstraction is a direct mutable cell; "
                "the C realization uses heap-allocated bool accessed via pointer dereference macros; "
                "no native mutable cell primitive exists in C"
            ),
            "ub_introduction": atomic(
                "use_after_free: "
                "dereferencing bool_cell_t after the backing memory is freed is undefined behaviour in C; "
                "the concept abstraction has no memory-lifetime model; "
                "the C realization introduces UB on any access through a freed pointer"
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

    # Step 1: mint realization first so abstraction can reference its CID.
    print("[C] Minting concept:bool-cell->c:pointer-indirection realization (N edge)...")
    real_memento = build_realization_bool_cell_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:bool-cell:c:pointer-indirection.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:bool-cell->c:pointer-indirection: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:bool-cell->c:pointer-indirection", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction with realization CID populated.
    print("[A] Minting concept:bool-cell abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_bool_cell()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:bool-cell.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:bool-cell: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:bool-cell", "cid": abst_cid, "path": str(abst_path)})

    # Stability check: mint each artifact a second time and compare.
    print("\n[STABILITY] Re-minting all artifacts for byte-stability check...")
    stable = True
    for check_name, memento, expected_cid in [
        ("abstraction", abst_memento, abst_cid),
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
    print(f"  realization CID:     {real_cid}")

    return abst_cid, real_cid


if __name__ == "__main__":
    mint_all()
