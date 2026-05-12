#!/usr/bin/env python3
"""
mint_list_java.py -- mint concept:list<T> Java realization.

Purpose: mint RealizationDesugaringMemento for (concept:list, java) via ArrayList<T>.

This mints:
  (A) RealizationDesugaringMemento: concept:list<T> -> java:array-backed-list

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt: null (PR1 form, no deferred marker).

Loss-record shape: 5-dim empirical profile matching C's linked-struct realization.
Java idiom: java.util.ArrayList<T> for direct realization.
  - O(1) amortized append (array growth factor ~1.5x)
  - O(1) indexed random access (array backing)
  - O(n) traversal for pure-functional list head operations
  - Full generics support (compile-time erasure)
  - Heap allocation (GC-managed, not explicit malloc/free)

Realization name: java:array-backed-list (canonical form: ArrayList<T>)
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

def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


# ---------------------------------------------------------------------------
# RealizationDesugaringMemento: concept:list<T> -> java:array-backed-list (N edge)
# ---------------------------------------------------------------------------

def build_realization_list_java():
    """
    N edge: concept:list<T> -> java:array-backed-list (ArrayList<T> realization).

    The Java realization encodes concept:list<T> as a heap-allocated, dynamically
    resizing array of T values, wrapped in java.util.ArrayList<T>.

    Signature:
      List<T> list = new ArrayList<T>();
      list.add(element);      // O(1) amortized
      list.get(i);            // O(1) indexed access
      list.remove(i);         // O(n) for deletions after index i

    Loss record (5-dim empirical profile):

    structural_divergence:
      The abstraction is a recursive sum type (Nil | Cons(head, tail)).
      Java ArrayList is an indexed sequence type backed by an Object[] array.
      Nil is represented implicitly (empty ArrayList); Cons is represented
      as a position in the array (index i, T at position i).
      No ADT constructors; access pattern is array indexing, not pattern matching.

    domain_narrowing:
      ArrayList<T> assumes all elements fit in heap memory and the size
      stays within Integer.MAX_VALUE. The realization narrows the abstraction
      domain to programs where list size <= 2^31-1. Programs with unbounded
      infinite lists are in the narrowed-out domain.

    ub_introduction:
      none: Java ArrayList performs bounds checks on all index accesses
      (IndexOutOfBoundsException on invalid index); the realization introduces
      no undefined behaviour; exceptions replace UB at all access sites.

    effect_divergence:
      ArrayList.add(T) is O(1) amortized (1.5x resize factor);
      reallocation happens at power-of-1.5 thresholds (empirical: [10, 15, 22, 33, ...]);
      concept:list Cons is structurally pure with no cost model;
      append introduces heap-allocation effects and GC pressure.

    value_divergence:
      ArrayList backing is contiguous Object[] array; sequential traversal has
      excellent L1/L2 cache locality (~64 bytes/line); the concept abstraction
      makes no locality claims; linked-struct (C) has poor spatial locality by
      contrast; iteration value semantics differ (index-based vs pointer-chain).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:list->java:array-backed-list",
        "formals": ["elements"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("concept:list", [var("elements")]),
            "rhs": op(
                "java:array-backed-list",
                [
                    op("java:generics", [
                        {"kind": "const", "value": "ArrayList<T>", "sort": ctor("JavaType")},
                        var("elements"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "java",
        "loss_record": {
            "structural_divergence": (
                "array-backed-list realization replaces native list sum type: "
                "Nil|Cons(T, list<T>) becomes ArrayList index space; "
                "Nil is empty ArrayList; Cons is implicit via size > 0; "
                "pattern matching becomes index range checks; "
                "recursive structure becomes array indexing; no ADT constructors"
            ),
            "domain_narrowing": (
                "ArrayList<T> size is bounded by Integer.MAX_VALUE; "
                "the realization narrows the abstraction domain to finite lists "
                "with size <= 2^31-1; programs with unbounded infinite lists "
                "are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "none: Java ArrayList performs bounds checks on all index accesses; "
                "IndexOutOfBoundsException is thrown on invalid index; "
                "the realization introduces no undefined behaviour; "
                "exceptions replace UB at all access sites"
            ),
            "effect_divergence": (
                "ArrayList.add(T) is O(1) amortized (1.5x resize factor); "
                "reallocation happens at power-of-1.5 thresholds; "
                "concept:list Cons is structurally pure with no cost model; "
                "empirical resize points: [10, 15, 22, 33, ...] elements; "
                "GC-managed allocation introduces heap and non-deterministic pause effects"
            ),
            "value_divergence": (
                "ArrayList backing is contiguous Object[] array; "
                "sequential traversal has excellent L1/L2 cache locality (~64 bytes/line); "
                "the concept abstraction makes no locality claims; "
                "linked-struct (C) has poor spatial locality by contrast; "
                "iteration value semantics differ: index-based vs pointer-chain traversal"
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

    print("[MINT] Minting concept:list->java:array-backed-list realization (N edge)...")
    real_memento = build_realization_list_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:list->java:array-backed-list.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:list->java:array-backed-list: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:list->java:array-backed-list", "cid": real_cid, "path": str(real_path)})

    # Stability check: mint the artifact a second time and compare.
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

    print("\n[DONE] Minted CID:")
    print(f"  realization (N edge) CID: {real_cid}")

    return real_cid


if __name__ == "__main__":
    mint_all()
