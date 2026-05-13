#!/usr/bin/env python3
"""
mint_tagged_union_java.py -- mint concept:tagged-union -> java realization (N edge).

Pattern mirrors PR #674 (concept:result->java) adapted for concept:tagged-union<T1,T2>.
This is N-edge-only: concept:tagged-union abstraction is already on main.

Mints:
  (A) RealizationDesugaringMemento: concept:tagged-union -> java  (the N edge)

CID is BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).

Loss record (concrete, 5-dimension):
  - structural_divergence: sealed interface + records replace native sum type
  - domain_narrowing: Java 21+ for static pattern matching; pre-21 requires manual instanceof
  - ub_introduction: none (records are immutable, field access always valid)
  - effect_divergence: none (sealed interfaces + records introduce no effects)
  - value_divergence: Java has no native two-variant union type

The Java idiom: sealed interface + records (Java 17+) with two record implementations:

  sealed interface TaggedUnion<T1, T2> permits LeftCase, RightCase {}
  record LeftCase<T1, T2>(T1 value) implements TaggedUnion<T1, T2> {}
  record RightCase<T1, T2>(T2 value) implements TaggedUnion<T1, T2> {}

Pattern matching (Java 21+):
  switch (tu) {
    case LeftCase<T1, T2>(var v) -> /* process v : T1 */
    case RightCase<T1, T2>(var v) -> /* process v : T2 */
  }

Pre-Java 21: instanceof + casting required.
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
# (A) RealizationDesugaringMemento: concept:tagged-union -> java  (N edge)
# ---------------------------------------------------------------------------

def build_realization_tagged_union_java():
    """
    N edge: concept:tagged-union<T1,T2> -> java (sealed interface + records).

    The Java realization encodes concept:tagged-union<T1,T2> as a sealed interface
    with two record implementations:

      sealed interface TaggedUnion<T1, T2> permits LeftCase, RightCase {}
      record LeftCase<T1, T2>(T1 value) implements TaggedUnion<T1, T2> {}
      record RightCase<T1, T2>(T2 value) implements TaggedUnion<T1, T2> {}

    Sealed interfaces (Java 17+) restrict which classes can implement the interface,
    providing a known set of subtypes. Records (Java 16+) provide immutable data
    carriers with auto-generated accessors. Together they form a safe sum type.

    Pattern matching (Java 21+):
      switch (tu) {
        case LeftCase<T1, T2>(var v) -> { /* process v : T1 */ }
        case RightCase<T1, T2>(var v) -> { /* process v : T2 */ }
      }

    Pre-Java 21 requires instanceof and explicit casting.

    Loss record (5-dimension):

    structural_divergence:
      The abstraction is a single ADT term with two disjoint arms (LEFT and RIGHT).
      Java realization requires multiple declarations: a sealed interface with
      two record implementations. The arms are encoded as separate record classes
      (LeftCase, RightCase) instead of named constructors. Pattern matching via
      switch on sealed type (Java 21+) or instanceof + casting (pre-21);
      exhaustiveness checking is static in Java 21+ but manual in earlier versions.
      Instantiation requires explicit record constructors.

    domain_narrowing:
      Java 21+ provides static exhaustiveness on sealed types via pattern matching.
      Pre-Java 21 exhaustiveness is not statically enforced; the realization narrows
      the domain to programs where either (a) target is Java 21+ with pattern matching,
      or (b) programmer manually checks both arms via instanceof. Programs that omit
      the check (pre-21) are in the narrowed-out domain.

    ub_introduction:
      None: records are immutable and enforce initialization of all fields.
      Field access (value() getter for both LeftCase and RightCase) is always valid.
      No memory unsafety, no null dereference (the record's field is non-null if
      initialized, nullability preserved from abstraction contract).

    effect_divergence:
      None: sealed interfaces and records introduce no effects. Record accessor
      methods are pure.

    value_divergence:
      Java has no native two-variant union type. The realization introduces a
      library shape (sealed interface + two records). Rust's enum-based tagged union
      is language-native; Java programs depend on this library definition.
      Rust programs depend on built-in enum type. Different runtime characteristics:
      Rust enums have zero-overhead abstraction; Java records have method call
      overhead (though typically JIT-optimized away).
    """
    return {
        "kind": "equation",
        "fn_name": "concept:tagged-union->java:sealed-interface",
        "formals": ["value"],
        "formal_sorts": [
            ctor("T1"),
            ctor("T2"),
        ],
        "post": {
            "lhs": op("concept:tagged-union", [var("value")]),
            "rhs": op(
                "java:sealed-interface",
                [
                    op("java:interface-def", [
                        {
                            "kind": "const",
                            "value": "TaggedUnion<T1, T2>",
                            "sort": ctor("InterfaceName")
                        },
                        op("java:permits-clause", [
                            {
                                "kind": "const",
                                "value": "LeftCase",
                                "sort": ctor("ClassName")
                            },
                            {
                                "kind": "const",
                                "value": "RightCase",
                                "sort": ctor("ClassName")
                            },
                        ]),
                        op("java:record-family", [
                            {
                                "kind": "const",
                                "value": "LeftCase<T1, T2>(T1 value)",
                                "sort": ctor("RecordDef")
                            },
                            {
                                "kind": "const",
                                "value": "RightCase<T1, T2>(T2 value)",
                                "sort": ctor("RecordDef")
                            },
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
                "interface declaration with two record implementations replace a single ADT constructor; "
                "arms encoded as separate record classes (LeftCase, RightCase) instead of named constructors; "
                "pattern matching via switch on sealed type (Java 21+) or instanceof + casting (pre-21); "
                "exhaustiveness checking is static in Java 21+ but manual in earlier versions; "
                "instantiation requires explicit record constructors"
            ),
            "domain_narrowing": (
                "Java 21+ provides static exhaustiveness on sealed types via pattern matching; "
                "pre-Java 21 exhaustiveness is not statically enforced; "
                "the realization narrows the domain to programs where either (a) target is Java 21+ with pattern matching, "
                "or (b) programmer manually checks both arms via instanceof; "
                "programs that omit the check (pre-21) are in the narrowed-out domain"
            ),
            "ub_introduction": (
                "none: records are immutable and enforce initialization of all fields; "
                "field access (value() getter for both LeftCase and RightCase) is always valid; "
                "no memory unsafety, no null dereference"
            ),
            "effect_divergence": (
                "none: sealed interfaces and records introduce no effects; "
                "record accessor methods are pure"
            ),
            "value_divergence": (
                "Java has no native two-variant union type; "
                "the realization introduces a library shape (sealed interface + two records); "
                "Rust enum-based tagged union is language-native; "
                "Java programs depend on this library definition; "
                "Rust programs depend on built-in enum type"
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

    print("[N] Minting concept:tagged-union->java:sealed-interface realization (N edge)...")
    real_memento = build_realization_tagged_union_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:tagged-union->java:sealed-interface.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:tagged-union->java:sealed-interface: {real_cid[:40]}...")
    cid_rows.append(
        {
            "kind": "realization",
            "name": "concept:tagged-union->java:sealed-interface",
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
