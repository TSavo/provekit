#!/usr/bin/env python3
"""
mint_assert.py -- mint concept:assert abstraction + C realization.

This is the simplest cell: a boolean predicate assertion concept.
Purpose: demonstrate the catalog-walker mechanism on a trivially-small cell.

Mints:
  (A) ConceptAbstractionMemento for concept:assert
  (B) RealizationDesugaringMemento: concept:assert -> c (one-line macro)

All CIDs are BLAKE3-512 via compute_fixture_cid.
All discharge_receipts are deferred: "deferred:pending-61-PR5"

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
# (A) ConceptAbstractionMemento: concept:assert
# ---------------------------------------------------------------------------

def build_abstraction_assert():
    """
    concept:assert is a boolean predicate assertion.

    Contract: Skolem predicate held(p) characterizes the assertion:
      - Pre: true (any predicate p may be asserted)
      - Post: the predicate p held (Skolem held(p))

    Slots: none (assertion is stateless).

    formal_sorts: none (assertion takes a Bool predicate as implicit argument).

    result_sort: Unit (assertion returns nothing; side effect is abort on failure).
    """
    return {
        "kind": "concept-abstraction",
        "operator": "concept:assert",
        "tier": "abstraction",
        "slots": [],
        "formal_sorts": [],
        "result_sort": "Unit",
        "contract": {
            "kind": "wp-rule",
            "formals": [],
            "body": skolem(
                "held",
                [
                    {"kind": "var", "name": "p"},
                ],
            ),
        },
        "contract_note": (
            "held(p) holds iff the boolean predicate p evaluated to true. "
            "If the predicate is false, the assertion aborts (abort on failure). "
            "The assertion is state-independent and returns Unit."
        ),
        "realizations": [],
    }


# ---------------------------------------------------------------------------
# (B) RealizationDesugaringMemento: concept:assert -> c  (one-line macro)
# ---------------------------------------------------------------------------

def build_realization_assert_c():
    """
    N edge: concept:assert -> c (one-line macro).

    The C realization encodes concept:assert as a one-line macro:
      #define ASSERT(p) do { if (!(p)) { abort(); } } while(0)

    Loss record (concrete, not placeholder):

    structural_divergence:
      The abstraction is a stateless assertion concept.
      The C realization requires a macro definition using an if statement and
      do-while loop (to allow use in all contexts). Macro instantiation is
      syntactic expansion, not a function call. The semantics differ in that
      the predicate p is evaluated eagerly at the call site, not deferred.

    effect_divergence:
      The concept asserts held(p) non-locally; if false, the program must abort.
      The C realization implements this via abort() which is a blocking system call,
      not a structured exception or return code. The effect is identical (termination
      on failure) but the mechanism is language-specific (no way to recover).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:assert->c:one-line-macro",
        "formals": [],
        "formal_sorts": [],
        "post": {
            "lhs": op("concept:assert", []),
            "rhs": op(
                "c:one-line-macro",
                [
                    {"kind": "const", "value": "#define ASSERT(p) do { if (!(p)) { abort(); } } while(0)", "sort": ctor("MacroBody")},
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "c",
        "loss_record": {
            "structural_divergence": (
                "macro_replaces_native_assertion: "
                "C has no native assertion; assert.h exists but the substrate's "
                "concept:assert is the canonical one; "
                "realization uses a single-line macro with do-while wrapper "
                "to allow use in all syntactic contexts; "
                "macro expansion is syntax-level, not function-call level"
            ),
            "effect_divergence": (
                "abort_on_failure: "
                "the concept specifies held(p) as the postcondition; "
                "the C realization calls abort() directly on predicate failure; "
                "abort() is a blocking system call with no structured recovery; "
                "the effect is identical (program termination) but the mechanism "
                "is language-specific and non-recoverable"
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

    print("[B] Minting concept:assert->c:one-line-macro realization (N edge)...")
    real_memento = build_realization_assert_c()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:assert->c:one-line-macro.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:assert->c:one-line-macro: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:assert->c:one-line-macro", "cid": real_cid, "path": str(real_path)})

    # Build and mint abstraction once, with realizations already populated.
    print("[A] Minting concept:assert abstraction (with realization CID populated)...")
    abst_memento = build_abstraction_assert()
    abst_memento["realizations"] = [real_cid]
    abst_entry, abst_cid = catalog_entry(abst_memento)
    abst_path = ABST_DIR / f"concept:assert.{abst_cid}.json"
    write_json(abst_path, abst_entry)
    print(f"  concept:assert: {abst_cid[:40]}...")
    cid_rows.append({"kind": "abstraction", "name": "concept:assert", "cid": abst_cid, "path": str(abst_path)})

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
