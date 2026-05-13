#!/usr/bin/env python3
"""
mint_tagged_union_python.py -- mint concept:tagged-union<T1,T2> realization for Python.

This script mints the Python realization of concept:tagged-union, which was
abstracted in feat/cell-tagged-union-c. The Python idiom is a dataclass with
a discriminator field (tag: Literal[0, 1]) and variant value fields
(value_left, value_right), mirroring the result-python pattern.

Mints:
  (B) RealizationDesugaringMemento: concept:tagged-union<T1,T2> -> python (the N edge)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

Loss-record shape: follows PR #636's empirical wire format (string values per dimension).
Python realization encodes via dataclass with typed discriminator and variant value fields.
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
# (B) RealizationDesugaringMemento: concept:tagged-union<T1,T2> -> python
# ---------------------------------------------------------------------------

def build_realization_tagged_union_python():
    """
    Python realization of concept:tagged-union<T1,T2>.

    The Python idiom is a dataclass with a discriminator field (tag) and
    two variant value fields (value_left, value_right). Only one field
    holds a meaningful value at any time, determined by the tag.

    Pattern mirrors result-python: a union is encoded as explicit fields
    with a runtime discriminator. Pattern matching becomes if/elif on the
    tag value.

    Encoding convention:
      - tag == 0 => left arm is active; value_left is set, value_right is None
      - tag == 1 => right arm is active; value_right is set, value_left is None

    Surface form:
      @dataclass
      class TaggedUnion:
          tag: Literal[0, 1]
          value_left: T1 | None = None
          value_right: T2 | None = None

    Instantiation: caller constructs the dataclass directly.
    Pattern matching: caller checks tu.tag and extracts the corresponding field.
    """
    return {
        "kind": "realization-desugaring-memento",
        "operator": "concept:tagged-union",
        "target_lang": "python",
        "target_form": "python:dataclass-discriminated-union",
        "formal_sorts": [
            ctor("T1"),
            ctor("T2"),
        ],
        "morphism": equation_tagged_union_python(),
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "loss_record": {
            "structural_divergence": (
                "tagged-union dataclass has three fields (tag, value_left, value_right) "
                "instead of a native union type or pattern-matching primitive; "
                "arms are encoded as integer tag values (LEFT=0, RIGHT=1) "
                "with oneOf semantics enforced by convention, not by the type system; "
                "instantiation requires caller to supply all three fields; "
                "pattern matching becomes if/elif on tu.tag with manual field access"
            ),
            "domain_narrowing": (
                "Python's type system cannot statically enforce that only one of "
                "value_left or value_right is non-None at any time; "
                "the realization narrows the abstraction domain to programs where "
                "the caller maintains the invariant that exactly one field is active; "
                "programs that set both fields or read the inactive field are in the "
                "narrowed-out domain"
            ),
            "ub_introduction": (
                "accessing tu.value_left when tu.tag == 1 (right active) yields None, "
                "not undefined behaviour as in C, but a runtime type error if the code "
                "assumes a non-None value; the abstraction statically excludes these reads; "
                "the Python realization introduces a runtime invariant violation on "
                "exactly those states"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


def equation_tagged_union_python():
    """
    Morphism equation: concept:tagged-union<T1,T2> -> python:dataclass-discriminated-union

    The equation asserts that a concept:tagged-union value desugars to a Python dataclass
    with tag (Literal[0, 1]), value_left (T1 | None), and value_right (T2 | None) fields.

    Loss dimensions:
      - structural_divergence: dataclass replaces native ADT
      - domain_narrowing: Python's type system cannot enforce oneOf
      - ub_introduction: runtime invariant violation instead of static exclusion
    """
    return {
        "kind": "equation",
        "fn_name": "concept:tagged-union->python:dataclass-discriminated-union",
        "formals": ["value"],
        "formal_sorts": [
            ctor("T1"),
            ctor("T2"),
        ],
        "post": {
            "lhs": op("concept:tagged-union", [var("value")]),
            "rhs": op(
                "python:dataclass-discriminated-union",
                [
                    op("python:dataclass-decl", [
                        {"kind": "const", "value": "TaggedUnion", "sort": ctor("ClassName")},
                        var("value"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "python",
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

    print("[B] Minting concept:tagged-union->python:dataclass-discriminated-union realization (N edge)...")
    real_memento = build_realization_tagged_union_python()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:tagged-union->python:dataclass-discriminated-union.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:tagged-union->python:dataclass-discriminated-union: {real_cid[:40]}...")
    cid_rows.append({
        "kind": "realization",
        "name": "concept:tagged-union->python:dataclass-discriminated-union",
        "cid": real_cid,
        "path": str(real_path),
    })

    # Stability check: mint artifact a second time and compare.
    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    stable = True
    check_cid = compute_cid(real_memento)
    if check_cid != real_cid:
        print(f"  UNSTABLE: {check_cid} != {real_cid}")
        stable = False
    else:
        print(f"  STABLE: ok")
    if not stable:
        raise SystemExit("ABORTING: CID instability detected. Fix canonical key order.")

    append_cids_tsv(cid_rows)

    print("\n[DONE] Minted CIDs:")
    print(f"  realize (N edge) CID: {real_cid}")

    return real_cid


if __name__ == "__main__":
    mint_all()
