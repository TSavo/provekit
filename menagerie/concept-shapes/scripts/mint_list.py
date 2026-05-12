#!/usr/bin/env python3
"""
mint_list.py -- mint concept:list<T> abstraction + rust lift edge + C realization.

This is the second per-cell catalog scout for the abstraction layer.
Purpose: mint concept:list<T> with linked-struct C realization, matching the pattern from #641.

Mints:
  (A) ConceptAbstractionMemento for concept:list<T>
  (B) Lift equation: rust:Vec<T> -> concept:list<T>  (the M edge)
  (C) RealizationDesugaringMemento: concept:list<T> -> c  (the N edge)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

Loss-record shape: follows PR #636's empirical wire format (string values per dimension).
Pattern: mirrors #641 structure with linked-struct realization.
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
# (A) ConceptAbstractionMemento: concept:list<T>
# ---------------------------------------------------------------------------

def build_abstraction_list():
    """
    concept:list<T> is a singly-linked list of T values.

    Contract: Skolem predicate list_inhabitant(self, T) characterizes the
    two-arm recursive structure:
      - Nil arm: the list is empty
      - Cons arm: the list has a head (sort T) and a tail (also a list of T)

    The contract says: self is either Nil (empty) or Cons(h, t) where h : T
    and t : list<T>. Pattern matching by induction on the list structure.

    Slots:
      - head: the carried value (sort T); meaningless when Nil arm is active.
      - tail: the list tail (sort list<T>); recursive.

    result_sort: ListOfT -- the sum type itself.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:list",
        "tier": "abstraction",
        "slots": [
            {"name": "head"},
            {"name": "tail"},
        ],
        "formal_sorts": [
            "T",
        ],
        "result_sort": "ListOfT",
        "contract": {
            "kind": "wp-rule",
            "formals": ["head", "tail"],
            "body": skolem(
                "list_inhabitant",
                [
                    {"kind": "var", "name": "self"},
                    {"kind": "var", "name": "T"},
                ],
            ),
        },
        "contract_note": (
            "list_inhabitant(self, T) holds iff self is either Nil (empty) "
            "or Cons(h, t) with h : T and t : list<T>; the arms are disjoint and exhaustive. "
            "When Cons, the head slot carries a term of sort T and the tail slot carries a list term. "
            "When Nil, accessing the head or tail slots is undefined."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) Lift equation: rust:Vec<T> -> concept:list<T>  (M edge)
# ---------------------------------------------------------------------------

def build_lift_rust_vec():
    """
    M edge: rust:Vec<T> -> concept:list<T>.

    This is an abstraction-lift equation.

    Rust's Vec<T> is a heap-allocated growable array: a concrete sequence type
    with O(1) amortized append and O(1) indexed access.
    The lift to concept:list<T> (a singly-linked Cons/Nil structure) introduces
    fundamental divergence:
      - structural_divergence: Vec is indexed access; list is sequential traversal.
        Arrays have static length proofs; lists have structural induction proofs.
      - domain_narrowing: Vec operations assume random access; list offers no indexing.
      - effect_divergence: Vec append may reallocate (heap effect); list Cons is pure.

    The lift is dischargeable via abstraction-lift proof, documenting the gap
    between Vec's O(1) indexing guarantee and list's O(n) head-access pattern.

    This provides the empirical M edge: the lift CID documents that rust:Vec<T>
    can be lifted to concept:list<T> with explicit loss accounting.
    """
    return {
        "kind": "equation",
        "fn_name": "rust:Vec->concept:list",
        "formals": ["elements"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("rust:Vec", [var("elements")]),
            "rhs": op("concept:list", [var("elements")]),
        },
        "role": "abstraction-lift",
        "direction": "left-to-right",
        "source_lang": "rust",
        "loss_record": {
            "structural_divergence": (
                "Vec is indexed sequence; list is singly-linked Cons/Nil structure; "
                "Vec offers O(1) random access; list offers O(n) sequential traversal; "
                "no structural encoding bridges the indexing gap"
            ),
            "domain_narrowing": (
                "Vec operations assume indexed access is available; "
                "list forbids indexing; programs relying on Vec[i] are in the narrowed-out domain"
            ),
            "effect_divergence": (
                "Vec append may reallocate memory (heap effect); "
                "list Cons is pure (functional); reallocation semantics diverge"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:list<T> -> c  (N edge)
# ---------------------------------------------------------------------------

def build_realization_list_c():
    """
    N edge: concept:list<T> -> c (linked-struct realization).

    The C realization encodes concept:list<T> as a singly-linked list of
    head-pointer + tail-pointer style nodes:

      typedef struct LIST_##T##_node {
        T head;
        struct LIST_##T##_node *tail;
      } LIST_##T##_node_t;

      typedef LIST_##T##_node_t *LIST_##T##_t;

    Macros:
      - LIST_NIL: NULL pointer representing empty list
      - LIST_CONS(head, tail): create a new node and cons it onto tail
      - LIST_HEAD(list): extract head value from node
      - LIST_TAIL(list): extract tail pointer from node
      - LIST_FOLD(list, init, var, acc, body): fold over the list

    Loss record (concrete, not placeholder):

    structural_divergence:
      The abstraction is a recursive sum type with Nil and Cons arms.
      The C realization requires pointer arithmetic and explicit node allocation.
      Nil is represented as a NULL pointer; Cons is represented as a
      dynamically allocated struct. Macros hide the allocation, but callers
      must manage memory (malloc/free).

    domain_narrowing:
      C cannot statically enforce proper list termination (Nil must be reached).
      The realization narrows the abstraction domain to programs where list
      traversal always terminates. Programs with circular list structures are
      in the narrowed-out domain; the abstraction forbids them.

    ub_introduction:
      Following a NULL tail pointer (dereferencing nil_pointer) is undefined
      behaviour. The concept abstraction forbids this by construction (structural
      exhaustiveness). The C realization introduces UB on dereferences of NULL.

    effect_divergence:
      Node allocation requires malloc (heap effect), and deallocation requires
      explicit free or garbage collection. The concept abstraction is pure;
      the C realization introduces memory-management effects.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:list->c:linked-struct",
        "formals": ["elements"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("concept:list", [var("elements")]),
            "rhs": op(
                "c:linked-struct",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "LIST_DECL(T, name)", "sort": ctor("MacroName")},
                        var("elements"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "linked-struct realization replaces native list type: "
                "struct with pointer field + macro family replace a single ADT constructor; "
                "Nil encoded as NULL; Cons encoded as heap-allocated node; "
                "pattern matching becomes pointer arithmetic with manual exhaustiveness; "
                "instantiation requires caller-supplied type and macro-name identifiers"
            ),
            "domain_narrowing": (
                "C cannot statically enforce list termination (reachability of Nil); "
                "the realization narrows the abstraction domain to programs where "
                "list traversal always terminates at NULL; "
                "programs with circular references are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "dereferencing a NULL tail pointer is undefined behaviour in C; "
                "the abstraction statically excludes this state via structural exhaustiveness; "
                "the C realization introduces UB on dereferences of NULL"
            ),
            "effect_divergence": (
                "node allocation requires malloc (heap effect); "
                "deallocation requires free or GC; "
                "the abstraction is pure; the C realization introduces memory-management effects"
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

    # Step 1: compute lift CID first so abstraction can reference it.
    print("[B] Minting rust:Vec->concept:list lift equation (M edge)...")
    lift_memento = build_lift_rust_vec()
    lift_entry, lift_cid = catalog_entry(lift_memento)
    lift_path = REAL_DIR / f"rust:Vec->concept:list.{lift_cid}.json"
    write_json(lift_path, lift_entry)
    print(f"  rust:Vec->concept:list: {lift_cid[:40]}...")
    cid_rows.append({"kind": "lift-equation", "name": "rust:Vec->concept:list", "cid": lift_cid, "path": str(lift_path)})

    print("[C] Minting concept:list->c:linked-struct realization (N edge)...")
    real_memento = build_realization_list_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:list->c:linked-struct.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:list->c:linked-struct: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:list->c:linked-struct", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:list<T> abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_list()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:list.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:list: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:list", "cid": abst_cid, "path": str(abst_path)})

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
