#!/usr/bin/env python3
"""
mint_result.py -- mint concept:result<T,E> abstraction + rust lift edge + C realization.

Pattern mirrors mint_option.py (PR #641) with two type parameters (T, E).

Mints:
  (A) ConceptAbstractionMemento for concept:result<T,E>
  (B) Lift equation: rust:Result<T,E> -> concept:result<T,E>  (the M edge)
  (C) RealizationDesugaringMemento: concept:result<T,E> -> c  (the N edge)

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
# (A) ConceptAbstractionMemento: concept:result<T,E>
# ---------------------------------------------------------------------------

def build_abstraction_result():
    """
    concept:result<T,E> is a value-or-error of type T or E.

    Contract: Skolem predicate result_inhabitant(self, T, E) characterizes the
    two-arm structure:
      - Ok arm: the value is present and has sort T
      - Err arm: the error is present and has sort E

    The contract says: self is inhabited by exactly one arm; when the Ok arm
    holds, the carried value has sort T; when the Err arm holds, the carried
    error has sort E.

    Slots:
      - value: the carried value (sort T); meaningless when Err arm is active.
      - error: the carried error (sort E); meaningless when Ok arm is active.

    result_sort: ResultOfT_E -- the sum type itself.
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:result",
        "tier": "abstraction",
        "slots": [
            {"name": "value"},
            {"name": "error"},
        ],
        "formal_sorts": [
            "T",
            "E",
        ],
        "result_sort": "ResultOfT_E",
        "contract": {
            "kind": "wp-rule",
            "formals": ["value", "error"],
            "body": skolem(
                "result_inhabitant",
                [
                    {"kind": "var", "name": "self"},
                    {"kind": "var", "name": "T"},
                    {"kind": "var", "name": "E"},
                ],
            ),
        },
        "contract_note": (
            "result_inhabitant(self, T, E) holds iff self is either Ok(v) with v : T, "
            "or Err(e) with e : E; the two arms are disjoint and exhaustive. "
            "When Ok, the value slot carries a term of sort T. "
            "When Err, the error slot carries a term of sort E. "
            "Accessing the value slot when Err is active is undefined; "
            "accessing the error slot when Ok is active is undefined."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) Lift equation: rust:Result<T,E> -> concept:result<T,E>  (M edge)
# ---------------------------------------------------------------------------

def build_lift_rust_result():
    """
    M edge: rust:Result<T,E> -> concept:result<T,E>.

    This is an abstraction-lift equation, symmetric in structure to an
    abstraction-realization but going from language to hub rather than hub to
    language. Role: "abstraction-lift".

    Rust's Result<T,E> is a native sum type: enum Result<T,E> { Ok(T), Err(E) }.
    It is a FIRST-CLASS ADT with statically enforced exhaustiveness via match.
    The lift is near-zero-loss:
      - structural_divergence: empty (Rust's Result IS the canonical model)
      - domain_narrowing: empty (all well-typed Result<T,E> values lift exactly)
      - ub_introduction: empty (match on Err without .unwrap() is safe)
      - effect_divergence: empty (Result itself is pure)

    The lift is dischargeable via canonicalizer-alpha-equivalence: the
    Rust-side IR for Result<T,E> maps directly to result_inhabitant(self, T, E)
    under the representation map { Ok(_) |-> Ok arm, Err(_) |-> Err arm }.
    Discharge: structural-wp-abstraction (the op's wp differs only in
    the wp_note documentation field from the concept's wp_rule).

    This provides the empirical M edge: the lift CID is the content-addressed
    proof that rust:Result<T,E> is a valid source-side instantiation of the hub.
    """
    return {
        "kind": "equation",
        "fn_name": "rust:Result->concept:result",
        "formals": ["value", "error"],
        "formal_sorts": [
            ctor("T"),
            ctor("E"),
        ],
        "post": {
            "lhs": op("rust:Result", [var("value"), var("error")]),
            "rhs": op("concept:result", [var("value"), var("error")]),
        },
        "role": "abstraction-lift",
        "direction": "left-to-right",
        "source_lang": "rust",
        "loss_record": {
            "structural_divergence": (
                "empty: Rust Result<T,E> is the canonical two-arm sum type; "
                "no structural encoding gap between the source and the hub concept"
            ),
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# (C) RealizationDesugaringMemento: concept:result<T,E> -> c  (N edge)
# ---------------------------------------------------------------------------

def build_realization_result_c():
    """
    N edge: concept:result<T,E> -> c (tagged-union macro family).

    The C realization encodes concept:result<T,E> as a tagged-union struct:
      typedef struct {
        enum { name_OK, name_ERR } tag;
        union { T ok; E err; } v;
      } name_result_t;

    Loss record (concrete, not placeholder):

    structural_divergence:
      The abstraction is a single ADT term with two disjoint arms.
      The C realization requires multiple declarations: a discriminator enum,
      a value-carrying union field with two arms, and a wrapper struct. The arms
      are encoded as integer tag values (name_OK, name_ERR); the concept's arms
      are named constructors. Calling convention: the caller must pass the
      type names and the macro family; no single polymorphic function exists.
      Surface form is macros expanding to struct literals, not a language
      primitive. Pattern matching becomes switch(res.tag) with manual
      exhaustiveness.

    domain_narrowing:
      C cannot statically enforce that all match arms are handled. The
      realization narrows the abstraction's domain to programs where the
      caller manually exhausts both arms (OK and ERR). Programs that
      read res.v.ok without checking res.tag are in the narrowed-out domain;
      the abstraction forbids them; the C realization cannot enforce the
      exclusion statically.

    ub_introduction:
      Accessing res.v.ok when res.tag == name_ERR reads an uninitialised
      union field; in C this is undefined behaviour. The concept abstraction
      makes this branch inaccessible by construction; the C realization
      introduces UB on precisely the states the abstraction statically
      excludes (the Err arm with ok-field access). Similarly for reading
      res.v.err when res.tag == name_OK.

    domain_narrowing (checked-exception discipline):
      Unlike Java/Haskell checked exceptions, Rust's Result type provides
      static exhaustiveness checking at compile time. C's tagged union
      cannot provide this guarantee; Result's distinct loss-dimension is
      the absence of static checked-exception discipline: the domain of
      valid C programs is narrowed to those where the programmer manually
      enforces exhaustiveness on every match site.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:result->c:tagged-union-macro",
        "formals": ["value", "error"],
        "formal_sorts": [
            ctor("T"),
            ctor("E"),
        ],
        "post": {
            "lhs": op("concept:result", [var("value"), var("error")]),
            "rhs": op(
                "c:tagged-union-macro",
                [
                    op("c:macro-expand", [
                        {"kind": "const", "value": "RESULT_DECL(T, E, name)", "sort": ctor("MacroName")},
                        var("value"),
                        var("error"),
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
                "multiple declarations (enum discriminator + union field with two arms + wrapper struct) "
                "replace a single ADT constructor; arms encoded as integer tag values "
                "(name_OK, name_ERR) instead of named constructors; "
                "pattern matching becomes switch(res.tag) with manual exhaustiveness; "
                "instantiation requires caller-supplied type and macro-name identifiers"
            ),
            "domain_narrowing": (
                "C cannot statically enforce exhaustiveness over the two arms; "
                "the realization narrows the abstraction domain to programs where "
                "the caller manually checks res.tag before accessing res.v; "
                "programs that omit the check are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "accessing res.v.ok when res.tag == name_ERR or res.v.err when res.tag == name_OK "
                "reads an uninitialised union field; this is undefined behaviour in C; "
                "the abstraction statically excludes these states; "
                "the C realization introduces UB on exactly those states"
            ),
            "domain_narrowing_no_static_checked_exception_discipline": (
                "Rust Result<T,E> provides static exhaustiveness checking via match expressions; "
                "C tagged unions cannot enforce this; the realization domain is narrowed to "
                "C programs where the programmer manually enforces exhaustiveness at every match site"
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
    print("[B] Minting rust:Result->concept:result lift equation (M edge)...")
    lift_memento = build_lift_rust_result()
    lift_entry, lift_cid = catalog_entry(lift_memento)
    lift_path = REAL_DIR / f"rust:Result->concept:result.{lift_cid}.json"
    write_json(lift_path, lift_entry)
    print(f"  rust:Result->concept:result: {lift_cid[:40]}...")
    cid_rows.append({"kind": "lift-equation", "name": "rust:Result->concept:result", "cid": lift_cid, "path": str(lift_path)})

    print("[C] Minting concept:result->c:tagged-union-macro realization (N edge)...")
    real_memento = build_realization_result_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:result->c:tagged-union-macro.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:result->c:tagged-union-macro: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:result->c:tagged-union-macro", "cid": real_cid, "path": str(real_path)})

    # Step 2: build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:result<T,E> abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_result()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:result.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:result: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:result", "cid": abst_cid, "path": str(abst_path)})

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
