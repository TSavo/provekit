#!/usr/bin/env python3
"""
mint_result_java.py -- mint concept:result<T,E> -> java realization (N edge).

Pattern mirrors PR #672 (concept:pair->java) and #641 (concept:option-c).
This is N-edge-only: the concept:result abstraction is already on main (PR #668).
The M-edge (rust:Result->concept:result) is also already on main.

Mints:
  (A) RealizationDesugaringMemento: concept:result<T,E> -> java  (the N edge)

CID is BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).

Loss record (concrete, 5-dimension):
  - structural_divergence: sealed interface + records vs native sum type
  - domain_narrowing: no compile-time exhaustiveness checking in Java pre-21
  - ub_introduction: none (records are immutable and null-safe)
  - effect_divergence: none (records are pure)
  - value_divergence: Java has no native Result type; realization is a library shape

The Java 17+ sealed interface idiom:
  sealed interface Result<T, E> permits Success, Failure {
    record Success<T, E>(T value) implements Result<T, E> {}
    record Failure<T, E>(E error) implements Result<T, E> {}
  }

Pattern matching (Java 21+) with sealed types achieves exhaustiveness checking.
Pre-Java 21: instanceof + casting required; exhaustiveness unchecked.
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
# IR formula helpers (matching discharge.py / mint_trinity.py conventions)
# ---------------------------------------------------------------------------

def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


# ---------------------------------------------------------------------------
# (A) RealizationDesugaringMemento: concept:result<T,E> -> java  (N edge)
# ---------------------------------------------------------------------------

def build_realization_result_java():
    """
    N edge: concept:result<T,E> -> java (sealed interface + records).

    The Java realization encodes concept:result<T,E> as a sealed interface
    family (Java 17+):

      sealed interface Result<T, E> permits Success, Failure {
        record Success<T, E>(T value) implements Result<T, E> {}
        record Failure<T, E>(E error) implements Result<T, E> {}
      }

    Pattern matching in Java 21+ on sealed types achieves compile-time
    exhaustiveness checking (similar to Rust match). Pre-Java 21: instanceof
    + casting required; exhaustiveness is not statically checked.

    Loss record (5-dimension):

    structural_divergence:
      The abstraction is a single ADT term with two disjoint arms.
      Java sealed interfaces encode the arms as separate record classes.
      The arms are named classes (Success, Failure) rather than constructors.
      Pattern matching syntax differs (switch with instanceof guards in
      pre-21; switch patterns in 21+). Instantiation requires explicit
      record constructors, not a macro family.

    domain_narrowing:
      Java 21+ provides static exhaustiveness checking on sealed types via
      pattern matching. Pre-Java 21 and all legacy patterns checking via
      instanceof require manual exhaustiveness discipline. The realization
      narrows the domain to programs where either (a) the target is Java 21+
      with pattern matching, or (b) the programmer manually exhausts both arms.

    ub_introduction:
      None. Records are immutable and records enforce that all fields are
      initialized at construction. Field access (value() or error()) is safe
      and always valid. No undefined behaviour.

    effect_divergence:
      None. Records are pure; sealed interfaces add no effects.

    value_divergence:
      Java has no native Result type in the standard library. The realization
      introduces a library shape (sealed interface + records) that is not
      language-native, unlike Rust's enum Result<T,E>. Programs written
      against Result<T,E> in Java depend on this library definition;
      programs written in Rust depend on the built-in type.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:result->java:sealed-interface",
        "formals": ["value", "error"],
        "formal_sorts": [
            ctor("T"),
            ctor("E"),
        ],
        "post": {
            "lhs": op("concept:result", [var("value"), var("error")]),
            "rhs": op(
                "java:sealed-interface",
                [
                    op("java:interface-def", [
                        {"kind": "const", "value": "Result<T, E>", "sort": ctor("InterfaceName")},
                        op("java:permits-clause", [
                            {"kind": "const", "value": "Success", "sort": ctor("ClassName")},
                            {"kind": "const", "value": "Failure", "sort": ctor("ClassName")},
                        ]),
                        op("java:record-family", [
                            {"kind": "const", "value": "Success<T, E>(T value)", "sort": ctor("RecordDef")},
                            {"kind": "const", "value": "Failure<T, E>(E error)", "sort": ctor("RecordDef")},
                        ]),
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "java",
        "loss_record": {
            "structural_divergence": (
                "sealed interface + records replace native sum type: "
                "interface declaration with two record implementations "
                "replace a single ADT constructor; arms encoded as separate record classes "
                "(Success, Failure) instead of named constructors; "
                "pattern matching via switch on sealed type (Java 21+) or instanceof + casting (pre-21); "
                "exhaustiveness checking is static in Java 21+ but manual in earlier versions; "
                "instantiation requires explicit record constructors"
            ),
            "domain_narrowing": (
                "Java 21+ provides static exhaustiveness on sealed types via pattern matching; "
                "pre-Java 21 exhaustiveness is not statically enforced; "
                "the realization narrows the domain to programs where either "
                "(a) target is Java 21+ with pattern matching, or (b) programmer manually checks both arms; "
                "programs that omit the check (pre-21) are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "none: records are immutable and enforce initialization of all fields; "
                "field access (value() or error()) is always valid; "
                "no undefined behaviour"
            ),
            "effect_divergence": (
                "none: records and sealed interfaces introduce no effects"
            ),
            "value_divergence": (
                "Java has no native Result type; the realization introduces a library shape "
                "(sealed interface + records); Rust Result<T,E> is language-native; "
                "Java programs depend on this library definition; Rust programs depend on built-in type"
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

    print("[N] Minting concept:result->java:sealed-interface realization (N edge)...")
    real_memento = build_realization_result_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:result->java:sealed-interface.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:result->java:sealed-interface: {real_cid[:40]}...")
    cid_rows.append(
        {
            "kind": "realization",
            "name": "concept:result->java:sealed-interface",
            "cid": real_cid,
            "path": str(real_path),
        }
    )

    # Stability check: mint realization a second time and compare.
    print("\n[STABILITY] Re-minting realization for byte-stability check...")
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
