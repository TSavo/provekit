#!/usr/bin/env python3
"""
mint_pair.py -- mint concept:pair<T1,T2> abstraction + rust lift edge + C realization.

Pattern-mirrors mint_option.py (#641), substituting two type parameters.

Purpose: land the M+N proof empirically with receipts for concept:pair<T1,T2>.

Mints:
  (A) ConceptAbstractionMemento for concept:pair<T1,T2>
  (B) Lift equation: rust:(T1,T2) -> concept:pair<T1,T2>  (the M edge)
  (C) RealizationDesugaringMemento: concept:pair<T1,T2> -> c  (the N edge)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-641-PR8"

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

DEFERRED_RECEIPT = "deferred:pending-641-PR8"
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
# (A) ConceptAbstractionMemento: concept:pair<T1,T2>
# ---------------------------------------------------------------------------

def build_abstraction_pair():
    """
    concept:pair<T1,T2> is a tuple of two values of type T1 and type T2.

    Contract: Skolem predicate pair_inhabitant(self, T1, T2) characterizes
    the two-slot structure:
      - first slot: carries a value of sort T1
      - second slot: carries a value of sort T2

    The contract says: self is inhabited by exactly one pair; the first slot
    holds a term of sort T1; the second slot holds a term of sort T2.

    Slots:
      - first: the first element (sort T1)
      - second: the second element (sort T2)

    result_sort: PairOfT1T2 -- the product type itself.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:pair",
        "tier": "abstraction",
        "slots": [
            {"name": "first"},
            {"name": "second"},
        ],
        "formal_sorts": [
            "T1",
            "T2",
        ],
        "result_sort": "PairOfT1T2",
        "contract": {
            "kind": "wp-rule",
            "formals": ["first", "second"],
            "body": skolem(
                "pair_inhabitant",
                [
                    {"kind": "var", "name": "self"},
                    {"kind": "var", "name": "T1"},
                    {"kind": "var", "name": "T2"},
                ],
            ),
        },
        "contract_note": (
            "pair_inhabitant(self, T1, T2) holds iff self is a pair with "
            "first slot carrying a term of sort T1 and second slot carrying a term of sort T2. "
            "Both slots are always occupied; there is no absence case. "
            "The pair structure is always fully inhabited."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) Lift equation: rust:(T1,T2) -> concept:pair<T1,T2>  (M edge)
# ---------------------------------------------------------------------------

def build_lift_rust_pair():
    """
    M edge: rust:(T1,T2) -> concept:pair<T1,T2>.

    This is an abstraction-lift equation, symmetric in structure to an
    abstraction-realization but going from language to hub rather than hub to
    language. Role: "abstraction-lift".

    Rust's tuple (T1, T2) is a native product type with two positional slots.
    It is a FIRST-CLASS value type with statically enforced tuple-access via
    pattern matching or field indexing (.0, .1).
    The lift is near-zero-loss:
      - structural_divergence: empty (Rust's tuples are the canonical model)
      - domain_narrowing: empty (all well-typed (T1,T2) values lift exactly)
      - ub_introduction: empty (tuple field access is safe)
      - effect_divergence: empty (tuples themselves are pure)

    The lift is dischargeable via canonicalizer-alpha-equivalence: the
    Rust-side IR for (T1, T2) maps directly to pair_inhabitant(self, T1, T2)
    under the representation map { (a, b) |-> (first: a, second: b) }.
    Discharge: structural-wp-abstraction (the op's wp differs only in
    the wp_note documentation field from the concept's wp_rule).

    This provides the empirical M edge: the lift CID is the content-addressed
    proof that rust tuples are a valid source-side instantiation of the hub.
    """
    return {
        "kind": "equation",
        "fn_name": "rust:tuple->concept:pair",
        "formals": ["first", "second"],
        "formal_sorts": [
            ctor("T1"),
            ctor("T2"),
        ],
        "post": {
            "lhs": op("rust:tuple", [var("first"), var("second")]),
            "rhs": op("concept:pair", [var("first"), var("second")]),
        },
        "role": "abstraction-lift",
        "direction": "left-to-right",
        "source_lang": "rust",
        "loss_record": {
            "structural_divergence": (
                "empty: Rust tuple (T1, T2) is the canonical two-slot product type; "
                "no structural encoding gap between the source and the hub concept"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:pair<T1,T2> -> c  (N edge)
# ---------------------------------------------------------------------------

def build_realization_pair_c():
    """
    N edge: concept:pair<T1,T2> -> c (struct with two fields).

    The C realization encodes concept:pair<T1,T2> as a simple struct:
      typedef struct {
        T1 first;
        T2 second;
      } PAIR_T1_T2_t;

    Plus constructor and accessor macros:
      #define PAIR(f, s) { .first = (f), .second = (s) }
      #define FIRST(p) ((p).first)
      #define SECOND(p) ((p).second)

    Loss record (concrete, not placeholder):

    structural_divergence:
      The abstraction is a single pair term with two slots.
      The C realization requires a struct declaration and caller-supplied
      type names to instantiate the macro family. Slot access becomes field
      selection (p.first, p.second) rather than a primitive operation.
      No pattern matching in C; access is direct or guarded by client code.

    domain_narrowing:
      C has no type-level pair distinct from struct. The realization narrows
      the domain to programs that treat the struct instance as a pair (not
      mutating it in ways that break the two-slot invariant, though C has no
      mechanism to statically enforce this). Programs that use the struct for
      other purposes are in the narrowed-out domain.

    ub_introduction:
      No UB compared to the abstraction. Both slots are always initialized.
      Field access is always valid. The C struct is safer than option in this
      respect (no absent slot to trap on).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:pair->c:struct",
        "formals": ["first", "second"],
        "formal_sorts": [
            ctor("T1"),
            ctor("T2"),
        ],
        "post": {
            "lhs": op("concept:pair", [var("first"), var("second")]),
            "rhs": op(
                "c:struct",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "PAIR(T1, T2, name)", "sort": ctor("MacroName")},
                        var("first"),
                        var("second"),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "struct with two fields replaces native product type: "
                "one declaration with explicit T1 and T2 field types "
                "replaces a language primitive; field access via .first and .second "
                "instead of pattern matching or positional indexing; "
                "instantiation requires caller-supplied type and macro-name identifiers"
            ),
            "domain_narrowing": (
                "C has no type-level distinction between a pair struct and any struct; "
                "the realization narrows the domain to programs that treat the struct instance "
                "as a pair (maintaining the two-slot invariant); "
                "programs that use the struct for other purposes are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "none: both slots are always occupied at initialization and remain valid; "
                "field access is always safe; the C struct is safer than option "
                "in that there is no absent slot"
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
    print("[B] Minting rust:tuple->concept:pair lift equation (M edge)...")
    lift_memento = build_lift_rust_pair()
    lift_entry, lift_cid = catalog_entry(lift_memento)
    lift_path = REAL_DIR / f"rust:tuple->concept:pair.{lift_cid}.json"
    write_json(lift_path, lift_entry)
    print(f"  rust:tuple->concept:pair: {lift_cid[:40]}...")
    cid_rows.append({"kind": "lift-equation", "name": "rust:tuple->concept:pair", "cid": lift_cid, "path": str(lift_path)})

    print("[C] Minting concept:pair->c:struct realization (N edge)...")
    real_memento = build_realization_pair_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:pair->c:struct.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:pair->c:struct: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:pair->c:struct", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:pair<T1,T2> abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_pair()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:pair.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:pair: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:pair", "cid": abst_cid, "path": str(abst_path)})

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
