#!/usr/bin/env python3
"""
mint_option.py -- mint concept:option<T> abstraction + rust lift edge + C realization.

This is the first per-cell catalog scout for the abstraction layer.
Purpose: land the M+N proof empirically with receipts for concept:option<T>.

Mints:
  (A) ConceptAbstractionMemento for concept:option<T>
  (B) Lift equation: rust:Option<T> -> concept:option<T>  (the M edge)
  (C) RealizationDesugaringMemento: concept:option<T> -> c  (the N edge)

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

sys.path.insert(0, str(Path(__file__).resolve().parent))
import discharge as _discharge

BASE = Path(__file__).resolve().parents[1]
CATALOG_REAL = BASE / "catalog"
ABST_DIR = CATALOG_REAL / "abstractions"
REAL_DIR = CATALOG_REAL / "realizations"
CID_FILE = BASE / "cids.tsv"

ROOT = BASE.parents[1]
RUST_DIR = ROOT / "implementations" / "rust"


def rel_path(path):
    """Return *path* relative to ROOT if it is absolute and under ROOT; else str(path)."""
    try:
        return str(Path(path).resolve().relative_to(ROOT.resolve()))
    except ValueError:
        return str(path)

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
    _discharge.write_json(path, value)


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
# (A) ConceptAbstractionMemento: concept:option<T>
# ---------------------------------------------------------------------------

def build_abstraction_option():
    """
    concept:option<T> is a value-or-nothing of type T.

    Contract: Skolem predicate option_inhabitant(self, T) characterizes the
    two-arm structure:
      - Some arm: the value is present and has sort T
      - None arm: the term denotes absence (no value of sort T is present)

    The contract says: self is inhabited by exactly one arm; when the Some arm
    holds, the carried value has sort T; when the None arm holds, no T-value
    is extractable without UB.

    Slots:
      - value: the carried value (sort T); meaningless when None arm is active.

    result_sort: OptionOfT -- the sum type itself.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:option",
        "tier": "abstraction",
        "slots": [
            {"name": "value"},
        ],
        "formal_sorts": [
            "T",
        ],
        "result_sort": "OptionOfT",
        "contract": {
            "kind": "wp-rule",
            "formals": ["value"],
            "body": skolem(
                "option_inhabitant",
                [
                    {"kind": "var", "name": "self"},
                    {"kind": "var", "name": "T"},
                ],
            ),
        },
        "contract_note": (
            "option_inhabitant(self, T) holds iff self is either Some(v) with v : T, "
            "or None; the two arms are disjoint and exhaustive. "
            "When Some, the value slot carries a term of sort T. "
            "When None, accessing the value slot is undefined."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) Lift equation: rust:Option<T> -> concept:option<T>  (M edge)
# ---------------------------------------------------------------------------

def build_lift_rust_option():
    """
    M edge: rust:Option<T> -> concept:option<T>.

    This is an abstraction-lift equation, symmetric in structure to an
    abstraction-realization but going from language to hub rather than hub to
    language. Role: "abstraction-lift".

    Rust's Option<T> is a native sum type: enum Option<T> { Some(T), None }.
    It is a FIRST-CLASS ADT with statically enforced exhaustiveness via match.
    The lift is near-zero-loss:
      - structural_divergence: empty (Rust's Option IS the canonical model)
      - domain_narrowing: empty (all well-typed Option<T> values lift exactly)
      - ub_introduction: empty (match on None without .unwrap() is safe)
      - effect_divergence: empty (Option itself is pure)

    The lift is dischargeable via canonicalizer-alpha-equivalence: the
    Rust-side IR for Option<T> maps directly to option_inhabitant(self, T)
    under the representation map { Some(_) |-> Some arm, None |-> None arm }.
    Discharge: structural-wp-abstraction (the op's wp differs only in
    the wp_note documentation field from the concept's wp_rule).

    This provides the empirical M edge: the lift CID is the content-addressed
    proof that rust:Option<T> is a valid source-side instantiation of the hub.
    """
    return {
        "kind": "equation",
        "fn_name": "rust:Option->concept:option",
        "formals": ["value"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("rust:Option", [var("value")]),
            "rhs": op("concept:option", [var("value")]),
        },
        "role": "abstraction-lift",
        "direction": "left-to-right",
        "source_lang": "rust",
        "loss_record": {
            "structural_divergence": (
                "empty: Rust Option<T> is the canonical two-arm sum type; "
                "no structural encoding gap between the source and the hub concept"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:option<T> -> c  (N edge)
# ---------------------------------------------------------------------------

def build_realization_option_c():
    """
    N edge: concept:option<T> -> c (tagged-union macro family).

    The C realization encodes concept:option<T> as a tagged-union struct:
      typedef struct {
        enum { name_NONE, name_SOME } tag;
        T some;
      } name_option_t;

    Loss record (concrete, not placeholder):

    structural_divergence:
      The abstraction is a single ADT term with two disjoint arms.
      The C realization requires three declarations: a discriminator enum,
      a value-carrying union field, and a wrapper struct. The arms are encoded
      as integer tag values (name_NONE=0, name_SOME=1); the concept's arms
      are named constructors. Calling convention: the caller must pass the
      type name and the macro family; no single polymorphic function exists.
      Surface form is macros expanding to struct literals, not a language
      primitive. Pattern matching becomes switch(opt.tag) with manual
      exhaustiveness.

    domain_narrowing:
      C cannot statically enforce that all match arms are handled. The
      realization narrows the abstraction's domain to programs where the
      caller manually exhausts both arms (SOME and NONE). Programs that
      read opt.some without checking opt.tag are in the narrowed-out domain;
      the abstraction forbids them; the C realization cannot enforce the
      exclusion statically.

    ub_introduction:
      Accessing opt.some when opt.tag == name_NONE reads an uninitialised
      union field; in C this is undefined behaviour. The concept abstraction
      makes this branch inaccessible by construction; the C realization
      introduces UB on precisely the states the abstraction statically
      excludes (the None arm with value-field access).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:option->c:tagged-union-macro",
        "formals": ["value"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("concept:option", [var("value")]),
            "rhs": op(
                "c:tagged-union-macro",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "OPTION_DECL(T, name)", "sort": ctor("MacroName")},
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
                "(name_NONE=0, name_SOME=1) instead of named constructors; "
                "pattern matching becomes switch(opt.tag) with manual exhaustiveness; "
                "instantiation requires caller-supplied type and macro-name identifiers"
            ),
            "domain_narrowing": (
                "C cannot statically enforce exhaustiveness over the two arms; "
                "the realization narrows the abstraction domain to programs where "
                "the caller manually checks opt.tag before accessing opt.some; "
                "programs that omit the check are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "accessing opt.some when opt.tag == name_NONE reads an uninitialised "
                "union field; this is undefined behaviour in C; "
                "the abstraction statically excludes this state; "
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
    print("[B] Minting rust:Option->concept:option lift equation (M edge)...")
    lift_memento = build_lift_rust_option()
    lift_entry, lift_cid = catalog_entry(lift_memento)
    lift_path = REAL_DIR / f"rust:Option->concept:option.{lift_cid}.json"
    write_json(lift_path, lift_entry)
    print(f"  rust:Option->concept:option: {lift_cid[:40]}...")
    cid_rows.append({"kind": "lift-equation", "name": "rust:Option->concept:option", "cid": lift_cid, "path": rel_path(lift_path)})

    print("[C] Minting concept:option->c:tagged-union-macro realization (N edge)...")
    real_memento = build_realization_option_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:option->c:tagged-union-macro.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:option->c:tagged-union-macro: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:option->c:tagged-union-macro", "cid": real_cid, "path": rel_path(real_path)})

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:option<T> abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_option()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:option.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:option: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:option", "cid": abst_cid, "path": rel_path(abst_path)})

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
