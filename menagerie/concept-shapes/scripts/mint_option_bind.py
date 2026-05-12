#!/usr/bin/env python3
"""
mint_option_bind.py -- mint concept:option-bind monad operator + C macro-composition realization.

Purpose: land monadic bind for option, composing on the concept:option cell from PR #641.

Mints:
  (A) ConceptAbstractionMemento for concept:option-bind<T,U>
  (B) Lift equation: rust:Option::and_then -> concept:option-bind (the M edge)
  (C) RealizationDesugaringMemento: concept:option-bind -> c:macro-composition (the N edge)

This cell DEPENDS on concept:option being in the catalog (from PR #641).
The C realization composes on the OPTION_DECL + OPTION_NONE + OPTION_SOME macros
from the option-c cell, implementing bind as:
  #define OPTION_BIND(name, opt, var, body) \
    ((opt).tag == name##_SOME ? ({ T var = (opt).some; (body); }) : OPTION_NONE(name))

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-642-PR5"

Loss-record shape: follows PR #636's empirical wire format (string values per dimension).
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

DEFERRED_RECEIPT = "deferred:pending-642-PR5"
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


def ctor(name):
    return {"kind": "constructor", "name": name}


def op(name, args):
    return {"kind": "operation", "name": name, "args": args}


def fn_type(formal_sorts, return_sort):
    """Build (T1, T2, ..., Tn) -> R"""
    return {
        "kind": "function",
        "formal_sorts": formal_sorts,
        "return_sort": return_sort,
    }


# ---------------------------------------------------------------------------
# (A) ConceptAbstractionMemento for concept:option-bind<T,U>
# ---------------------------------------------------------------------------

def build_abstraction_option_bind():
    """
    The monadic bind abstraction. Signature:
      fn bind<T, U>(opt: Option<T>, f: T -> Option<U>) -> Option<U>

    Laws (discharged via receiver, not represented in IR):
      bind(Some(v), f) == f(v)
      bind(None, f) == None

    This abstraction depends on concept:option being catalogued.
    """
    return {
        "kind": "abstraction",
        "name": "concept:option-bind",
        "formals": ["T", "U"],
        "formal_sorts": [ctor("T"), ctor("U")],
        "type": fn_type(
            [
                op("concept:option", [var("T")]),
                op("fn", [var("T"), op("concept:option", [var("U")])]),
            ],
            op("concept:option", [var("U")]),
        ),
        "description": (
            "Monadic bind for option type. "
            "Applies a function that returns an Option to the value inside an Option, "
            "flattening the nested option structure. "
            "Pattern-mirrors the monad law m >>= f."
        ),
        "role": "hub-abstraction",
        "semantics": "monadic-bind",
        "dependencies": ["concept:option"],
        "realizations": [],
        "discharge_receipt": DEFERRED_RECEIPT,
    }


# ---------------------------------------------------------------------------
# (B) Lift equation: rust:Option::and_then -> concept:option-bind (M edge)
# ---------------------------------------------------------------------------

def build_lift_rust_option_bind():
    """
    M edge: rust:Option<T>::and_then -> concept:option-bind<T,U>

    Rust's Option::and_then is the canonical monadic bind for Option<T>.
    Loss record: no structural divergence.
    """
    return {
        "kind": "equation",
        "fn_name": "rust:Option::and_then->concept:option-bind",
        "formals": ["T", "U"],
        "formal_sorts": [ctor("T"), ctor("U")],
        "post": {
            "lhs": op(
                "rust:Option::and_then",
                [var("T"), var("U")],
            ),
            "rhs": op("concept:option-bind", [var("T"), var("U")]),
        },
        "role": "abstraction-lift",
        "direction": "left-to-right",
        "source_lang": "rust",
        "loss_record": {
            "structural_divergence": (
                "none; Rust Option<T>::and_then is the canonical monadic bind, "
                "no encoding gap between source and hub concept"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:option-bind -> c:macro-composition
# ---------------------------------------------------------------------------

def build_realization_option_bind_c():
    """
    N edge: concept:option-bind<T,U> -> c:macro-composition

    The C realization composes on the macros from the option-c cell (PR #641).
    It implements bind as a statement-expression that:
      1. Checks if the input option's tag is name_SOME
      2. If yes: binds the inner value to a variable and evaluates the closure
      3. If no: returns OPTION_NONE(name)

    The macro is:
      #define OPTION_BIND(name, opt, var, body) \
        ((opt).tag == name##_SOME ? ({ T var = (opt).some; (body); }) : OPTION_NONE(name))

    This uses GNU C statement-expressions ({ ... }) which is a non-portable extension
    not available in MSVC or strict C99.

    Loss record (concrete):
      - structural_divergence: macro-composition replaces native monad operator
      - domain_narrowing: T must be block-expression-compatible
      - non_portability: requires GNU C statement-expressions
    """
    return {
        "kind": "equation",
        "fn_name": "concept:option-bind->c:macro-composition",
        "formals": ["T", "U"],
        "formal_sorts": [ctor("T"), ctor("U")],
        "post": {
            "lhs": op("concept:option-bind", [var("T"), var("U")]),
            "rhs": op(
                "c:macro-composition",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "OPTION_BIND(name, opt, var, body)", "sort": ctor("MacroName")},
                        var("T"),
                        var("U"),
                    ]),
                    op("c:compose-on-cell", [
                        {"kind": "const", "value": "concept:option", "sort": ctor("CellRef")},
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "macro-composition replaces native monad operator: "
                "bind is implemented as a ternary conditional operator with embedded "
                "GNU statement-expression rather than a native function call or operator"
            ),
            "domain_narrowing": (
                "T must be compatible with block-expression binding semantics; "
                "the closure body must be a valid C expression (no multiple statements without wrapping); "
                "the macro composition assumes the closure evaluates to type Option<U>"
            ),
            "non_portability": (
                "requires GNU C statement-expressions ({ ... }) which is not portable "
                "to MSVC or strict C99; requires -std=gnu99 or gnu11 flag"
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

    # Step 1: compute lift and realization CIDs first so abstraction can reference realization.
    print("[B] Minting rust:Option::and_then->concept:option-bind lift equation (M edge)...")
    lift_memento = build_lift_rust_option_bind()
    lift_entry, lift_cid = catalog_entry(lift_memento)
    lift_path = REAL_DIR / f"rust:Option::and_then->concept:option-bind.{lift_cid}.json"
    write_json(lift_path, lift_entry)
    print(f"  rust:Option::and_then->concept:option-bind: {lift_cid[:40]}...")
    cid_rows.append({"kind": "lift-equation", "name": "rust:Option::and_then->concept:option-bind", "cid": lift_cid, "path": str(lift_path)})

    print("[C] Minting concept:option-bind->c:macro-composition realization (N edge)...")
    real_memento = build_realization_option_bind_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:option-bind->c:macro-composition.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:option-bind->c:macro-composition: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:option-bind->c:macro-composition", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction once, with realization CID populated.
    print("[A] Minting concept:option-bind<T,U> abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_option_bind()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:option-bind.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:option-bind: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:option-bind", "cid": abst_cid, "path": str(abst_path)})

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
