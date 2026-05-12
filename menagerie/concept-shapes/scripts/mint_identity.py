#!/usr/bin/env python3
"""
mint_identity.py -- mint concept:identity abstraction + rust lift edge + C realization.

This is the zero-loss cell scout for the abstraction layer.
Purpose: land the M+N proof empirically with receipts for concept:identity.

Mints:
  (A) ConceptAbstractionMemento for concept:identity
  (B) Lift equation: rust:identity -> concept:identity  (the M edge)
  (C) RealizationDesugaringMemento: concept:identity -> c  (the N edge)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

Loss-record shape: follows PR #636's empirical wire format (string values per dimension).
PR #634 (BTreeMap<String,IrFormula>) is not yet merged; this PR documents the dependency
and will require a successor mint when #634 lands and the IrFormula encoding is standardised.

concept:identity is the zero-loss cell: projection-distance is exactly 0.
Contract: forall x: T. concept:identity(x) == x.
C realization: #define IDENTITY(x) (x) -- trivial macro, type-generic, zero overhead.
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
# (A) ConceptAbstractionMemento: concept:identity
# ---------------------------------------------------------------------------

def build_abstraction_identity():
    """
    concept:identity is the identity function for any type T.

    Contract: op("eq", [op("concept:identity", [var("x")]), var("x")])
    That is: concept:identity(x) == x, for all x of sort T.

    This is the zero-loss cell: projection distance is exactly 0.
    The identity function has exactly one slot (the input value) and
    returns it unchanged. result_sort is T (same sort as input).

    Slots:
      - x: the input value (sort T); returned unchanged.

    result_sort: T -- the identity function preserves sort.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:identity",
        "tier": "abstraction",
        "slots": [
            {"name": "x"},
        ],
        "formal_sorts": [
            "T",
        ],
        "result_sort": "T",
        "contract": {
            "kind": "wp-rule",
            "formals": ["x"],
            "body": op(
                "eq",
                [
                    op("concept:identity", [{"kind": "var", "name": "x"}]),
                    {"kind": "var", "name": "x"},
                ],
            ),
        },
        "contract_note": (
            "concept:identity(x) == x holds for all x of sort T. "
            "The identity function introduces no structural transformation, "
            "no domain narrowing, and no undefined behaviour. "
            "This is the zero-loss cell: the projection distance from concept:identity "
            "to any faithful realization is exactly 0."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) Lift equation: rust:identity -> concept:identity  (M edge)
# ---------------------------------------------------------------------------

def build_lift_rust_identity():
    """
    M edge: rust:identity -> concept:identity.

    This is an abstraction-lift equation going from language to hub.
    Role: "abstraction-lift".

    Rust's std::convert::identity::<T>(x: T) -> T is a stable no-op function
    that returns its argument unchanged. It is zero-overhead (inlined by LLVM),
    type-safe, and has no side effects.

    The lift is zero-loss:
      - structural_divergence: empty (Rust's identity IS the canonical model)
      - domain_narrowing: empty (all well-typed T values lift exactly)
      - ub_introduction: empty (identity is unconditionally safe)
      - effect_divergence: empty (identity is pure)

    loss_record is intentionally empty: this is the zero-loss M edge.
    """
    return {
        "kind": "equation",
        "fn_name": "rust:identity->concept:identity",
        "formals": ["x"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("rust:identity", [var("x")]),
            "rhs": op("concept:identity", [var("x")]),
        },
        "role": "abstraction-lift",
        "direction": "left-to-right",
        "source_lang": "rust",
        "loss_record": {},
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:identity -> c  (N edge)
# ---------------------------------------------------------------------------

def build_realization_identity_c():
    """
    N edge: concept:identity -> c (identity macro).

    The C realization encodes concept:identity as a type-generic macro:
      #define IDENTITY(x) (x)

    This is a zero-overhead, type-generic, trivially-correct realization.
    The macro expands to its argument with no transformation.

    Loss record: empty for all dimensions.

    concept:identity is the one cell whose projection distance is exactly 0:
      - No structural encoding gap (macro expands to the argument itself)
      - No domain narrowing (valid for all T, including non-pointer types)
      - No UB introduction (no memory access, no undefined operation)
      - No effect divergence (macro is purely syntactic)

    The only caveats are macro hygiene (double-evaluation if x has side
    effects in the expansion context) and the absence of type-checking
    enforcement. These are properties of C macros in general and are
    documented here as notes, not as loss dimensions, because they do not
    constitute projection distance from the abstraction -- the abstraction
    itself is also silent on evaluation count (it specifies extensional
    equality, not operational semantics).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:identity->c:identity-macro",
        "formals": ["x"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("concept:identity", [var("x")]),
            "rhs": op(
                "c:identity-macro",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "IDENTITY(x)", "sort": ctor("MacroName")},
                        var("x"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {},
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
    print("[B] Minting rust:identity->concept:identity lift equation (M edge)...")
    lift_memento = build_lift_rust_identity()
    lift_entry, lift_cid = catalog_entry(lift_memento)
    lift_path = REAL_DIR / f"rust:identity->concept:identity.{lift_cid}.json"
    write_json(lift_path, lift_entry)
    print(f"  rust:identity->concept:identity: {lift_cid[:40]}...")
    cid_rows.append({"kind": "lift-equation", "name": "rust:identity->concept:identity", "cid": lift_cid, "path": str(lift_path)})

    print("[C] Minting concept:identity->c:identity-macro realization (N edge)...")
    real_memento = build_realization_identity_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:identity->c:identity-macro.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:identity->c:identity-macro: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:identity->c:identity-macro", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:identity abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_identity()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:identity.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:identity: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:identity", "cid": abst_cid, "path": str(abst_path)})

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
