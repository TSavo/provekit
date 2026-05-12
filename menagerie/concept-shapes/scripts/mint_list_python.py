#!/usr/bin/env python3
"""
mint_list_python.py -- mint concept:list<T> Python realization.

Purpose: land the N edge empirically with receipt for concept:list<T> -> python:list.

Mints:
  (A) RealizationDesugaringMemento: concept:list<T> -> python (native list builtin)

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt: null (PR1 form, as per #672 template)

Loss-record shape: full 5-dimensional per #636 empirical wire format.

Python idiom: native list builtin (dynamic array). The realization encodes
concept:list<T> as the Python list type, a resizable homogeneous sequence.
List operations: append, indexing, slicing, iteration. Nil case handled by
empty list; Cons case by non-empty list with elements accessible via [0..len-1].
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

def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


# ---------------------------------------------------------------------------
# (A) RealizationDesugaringMemento: concept:list<T> -> python
# ---------------------------------------------------------------------------

def build_realization_list_python():
    """
    N edge: concept:list<T> -> python (native list builtin).

    The Python realization encodes concept:list<T> as the language's native
    list type: a dynamic array with O(1) amortized append and O(1) indexed access.

    List operations:
      - empty: []
      - cons (prepend): [head] + tail (or tail.insert(0, head))
      - head: lst[0] (raises IndexError if empty)
      - tail: lst[1:] or lst.pop(0)
      - len: len(lst)
      - iterate: for elem in lst

    Loss record (concrete, full 5-dimensional):

    structural_divergence:
      The abstraction encodes a singly-linked list as head+tail (Cons/Nil).
      Python realization uses a contiguous dynamic array (list). This changes
      the structural encoding: no explicit Cons/Nil constructors; instead,
      presence/absence is implicit in list length. Head access is O(1) array
      indexing ([0]); tail access requires slicing ([1:]) and is O(n). In the
      abstraction, both head and tail have symmetric structure; in the realization,
      tail production is linear rather than constant-time.

    domain_narrowing:
      Python has no type-level distinction between a list and other sequences.
      The realization narrows the domain to programs that treat the list instance
      as a homogeneous sequence of type T (maintaining invariant: all elements
      of the same sort). Programs that mix element types or treat list as a
      mutable map are in the narrowed-out domain. Additionally, the realization
      presumes the dynamic array implementation; programs that depend on
      pointer-based linked-list behavior (e.g., structural sharing of tails)
      are in the narrowed-out domain.

    ub_introduction:
      Nil case (empty list) introduces no undefined behavior; indexing an empty
      list raises IndexError, which is a defined exception in Python (not UB).
      List mutations (append, pop, insert, delete) are bounds-checked by the
      runtime. No buffer overflows or use-after-free. The realization is safer
      than raw C array handling in this respect. However, programs that assume
      tail sharing (structural sharing in functional style) will observe copy
      semantics instead, which could change program meaning (mutation of a tail
      slice will not affect the original list).

    effect_divergence:
      List operations are observable: mutations have side effects on the object
      identity. Slicing (tail operation [1:]) creates a new list, not a view;
      this is a visible side effect distinct from the abstraction's pure
      head/tail operations. Append/pop operations mutate the list in place
      (side effects). The abstraction assumes pure functional semantics; the
      realization introduces imperative mutation effects.

    value_divergence:
      Python has no native algebraic sum for Nil/Cons; the realization introduces
      a library shape via the list type. The absence of a Cons arm in an empty
      list is implicit (len == 0); in the abstraction, Nil is an explicit arm.
      Programs that rely on exhaustive pattern matching (Rust, Haskell, OCaml)
      require explicit case analysis; Python programs rely on conditional
      checking (if len(lst) > 0, etc.), which is more error-prone. Value
      representation is also different: abstraction has head:T and tail:list<T>
      as explicit fields; Python stores all elements in a flat array, so
      accessing the "head" and "tail" requires computed access patterns.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:list->python:list",
        "formals": ["elem"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("concept:list", [var("elem")]),
            "rhs": op("python:list", [var("elem")]),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
        "loss_record": {
            "structural_divergence": (
                "dynamic array replaces singly-linked-list structure: "
                "the abstraction is a recursive Cons/Nil tree; the realization is a flat contiguous array. "
                "Head access ([0]) is O(1) in both; tail access (lst[1:]) is O(n) in the realization (copy of rest of array) "
                "vs O(1) in the abstraction (pointer dereference). "
                "No explicit Cons/Nil constructors; presence/absence implicit in list length. "
                "Slicing and iteration patterns differ substantially."
            ),
            "domain_narrowing": (
                "Python has no type-level list distinct from other sequences; "
                "the realization narrows the domain to programs that treat the list as a homogeneous sequence of type T "
                "(maintaining the invariant that all elements are of the same sort). "
                "Programs that mix element types are in the narrowed-out domain. "
                "Programs that depend on pointer-based structural sharing (functional tail reuse) are in the narrowed-out domain; "
                "slicing creates a new list (copy), not a view."
            ),
            "ub_introduction": (
                "none: indexing an empty list raises IndexError (a defined exception, not UB). "
                "List mutations are bounds-checked by the runtime. "
                "No buffer overflows or memory safety violations. "
                "However, programs that assume pure functional tail sharing will observe copy semantics instead, "
                "which may change program meaning (mutations are not visible to the original list after a slice)."
            ),
            "effect_divergence": (
                "list operations are observable and mutating: append, pop, insert, delete modify the list in place. "
                "The abstraction assumes pure functional head/tail operations; the realization introduces imperative mutation effects. "
                "Slicing (lst[1:]) creates a new list (observable allocation and copy); not a pure operation. "
                "Iteration has side effects if the list is modified during iteration (raises RuntimeError in CPython 3.7+)."
            ),
            "value_divergence": (
                "Python has no native algebraic sum type for Nil/Cons; the realization uses implicit representation: "
                "empty list (len == 0) corresponds to Nil; non-empty list corresponds to Cons. "
                "There is no explicit pattern match or constructor call; access patterns are conditional (if len > 0). "
                "In the abstraction, head and tail are first-class slots; in the realization, they are computed via indexing and slicing. "
                "Abstraction: Nil and Cons are disjoint arms with disjoint slot sets; realization: all elements live in one array, "
                "no arm distinction. Programs that rely on exhaustive static pattern matching are in the narrowed-out domain."
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
    ABST_DIR.mkdir(parents=True, exist_ok=True)
    REAL_DIR.mkdir(parents=True, exist_ok=True)

    cid_rows = []

    print("[A] Minting concept:list->python:list realization (N edge)...")
    real_memento = build_realization_list_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:list->python:list.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:list->python:list: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:list->python:list", "cid": real_cid, "path": str(real_path)})

    # Stability check: mint artifact a second time and compare.
    print("\n[STABILITY] Re-minting artifact for byte-stability check...")
    check_cid = compute_cid(real_memento)
    if check_cid != real_cid:
        print(f"  UNSTABLE: realization: {check_cid} != {real_cid}")
        raise SystemExit("ABORTING: CID instability detected. Fix canonical key order.")
    else:
        print(f"  STABLE: realization: ok")

    append_cids_tsv(cid_rows)

    print("\n[DONE] Minted CID:")
    print(f"  realization (N edge) CID: {real_cid}")

    return real_cid


if __name__ == "__main__":
    mint_all()
