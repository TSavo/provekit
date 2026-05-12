#!/usr/bin/env python3
"""
mint_tagged_union.py -- mint concept:tagged-union<T1,T2> abstraction + C realization.

This cell generalizes concept:option<T> to a two-variant sum type.
Pattern: option is tagged-union<T, unit>.

Mints:
  (A) ConceptAbstractionMemento for concept:tagged-union<T1,T2>
  (B) RealizationDesugaringMemento: concept:tagged-union<T1,T2> -> c  (the N edge)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

Loss-record shape: follows PR #636's empirical wire format (string values per dimension).
C realization encodes via tagged-struct-union macros (mirrors option-c structure).
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
# (A) ConceptAbstractionMemento: concept:tagged-union<T1,T2>
# ---------------------------------------------------------------------------

def build_abstraction_tagged_union():
    """
    concept:tagged-union<T1,T2> is a two-variant sum type.

    Contract: Skolem predicate tagged_union_inhabitant(self, T1, T2) characterizes
    the two-arm structure:
      - LEFT arm: the value is present and has sort T1
      - RIGHT arm: the value is present and has sort T2

    The contract says: self is inhabited by exactly one arm; when the LEFT arm
    holds, the carried value has sort T1; when the RIGHT arm holds, the carried
    value has sort T2; the arms are disjoint and exhaustive.

    Slots:
      - value: the carried value (sort T1 when LEFT, sort T2 when RIGHT)

    result_sort: TaggedUnionOfT1T2 -- the sum type itself.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:tagged-union",
        "tier": "abstraction",
        "slots": [
            {"name": "value"},
        ],
        "formal_sorts": [
            "T1",
            "T2",
        ],
        "result_sort": "TaggedUnionOfT1T2",
        "contract": {
            "kind": "wp-rule",
            "formals": ["value"],
            "body": skolem(
                "tagged_union_inhabitant",
                [
                    {"kind": "var", "name": "self"},
                    {"kind": "var", "name": "T1"},
                    {"kind": "var", "name": "T2"},
                ],
            ),
        },
        "contract_note": (
            "tagged_union_inhabitant(self, T1, T2) holds iff self is either LEFT(v) with v : T1, "
            "or RIGHT(v) with v : T2; the two arms are disjoint and exhaustive. "
            "When LEFT, the value slot carries a term of sort T1. "
            "When RIGHT, the value slot carries a term of sort T2."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) RealizationDesugaringMemento: concept:tagged-union<T1,T2> -> c  (N edge)
# ---------------------------------------------------------------------------

def build_realization_tagged_union_c():
    """
    N edge: concept:tagged-union<T1,T2> -> c (tagged-union macro family).

    The C realization encodes concept:tagged-union<T1,T2> as a tagged-union struct:
      typedef struct {
        enum { name_LEFT, name_RIGHT } tag;
        union { T1 left; T2 right; } v;
      } name_tu_t;

    Loss record (concrete, not placeholder):

    structural_divergence:
      The abstraction is a single ADT term with two disjoint arms.
      The C realization requires three declarations: a discriminator enum,
      a value-carrying union field, and a wrapper struct. The arms are encoded
      as integer tag values (name_LEFT=0, name_RIGHT=1); the concept's arms
      are named constructors. Calling convention: the caller must pass the
      type name and the macro family; no single polymorphic function exists.
      Surface form is macros expanding to struct literals, not a language
      primitive. Pattern matching becomes switch(tu.tag) with manual
      exhaustiveness.

    domain_narrowing:
      C cannot statically enforce that all match arms are handled. The
      realization narrows the abstraction's domain to programs where the
      caller manually exhausts both arms (LEFT and RIGHT). Programs that
      read tu.v without checking tu.tag are in the narrowed-out domain;
      the abstraction forbids them; the C realization cannot enforce the
      exclusion statically.

    ub_introduction:
      Accessing tu.v.left when tu.tag == name_RIGHT reads an uninitialised
      union field; in C this is undefined behaviour. Similarly for accessing
      tu.v.right when tu.tag == name_LEFT. The concept abstraction makes
      these branches inaccessible by construction; the C realization introduces
      UB on precisely the states the abstraction statically excludes.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:tagged-union->c:tagged-union-macro",
        "formals": ["value"],
        "formal_sorts": [
            ctor("T1"),
            ctor("T2"),
        ],
        "post": {
            "lhs": op("concept:tagged-union", [var("value")]),
            "rhs": op(
                "c:tagged-union-macro",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "TAGGED_UNION_DECL(T1, T2, name)", "sort": ctor("MacroName")},
                        var("value"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "tagged-union macro family replaces native sum type: "
                "three declarations (enum discriminator + union field + wrapper struct) "
                "replace a single ADT constructor; arms encoded as integer tag values "
                "(name_LEFT=0, name_RIGHT=1) instead of named constructors; "
                "pattern matching becomes switch(tu.tag) with manual exhaustiveness; "
                "instantiation requires caller-supplied type names and macro-name identifier"
            ),
            "domain_narrowing": (
                "C cannot statically enforce exhaustiveness over the two arms; "
                "the realization narrows the abstraction domain to programs where "
                "the caller manually checks tu.tag before accessing tu.v; "
                "programs that omit the check are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "accessing tu.v.left when tu.tag == name_RIGHT, or tu.v.right when "
                "tu.tag == name_LEFT, reads an uninitialised union field; "
                "this is undefined behaviour in C; "
                "the abstraction statically excludes these states; "
                "the C realization introduces UB on exactly those states"
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
    print("[B] Minting concept:tagged-union->c:tagged-union-macro realization (N edge)...")
    real_memento = build_realization_tagged_union_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:tagged-union->c:tagged-union-macro.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:tagged-union->c:tagged-union-macro: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:tagged-union->c:tagged-union-macro", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:tagged-union<T1,T2> abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_tagged_union()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:tagged-union.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:tagged-union: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:tagged-union", "cid": abst_cid, "path": str(abst_path)})

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
    print(f"  realize (N edge) CID:{real_cid}")

    return abst_cid, real_cid


if __name__ == "__main__":
    mint_all()
