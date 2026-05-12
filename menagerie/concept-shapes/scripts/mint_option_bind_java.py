#!/usr/bin/env python3
"""
mint_option_bind_java.py -- mint concept:option-bind -> java realization (N edge).

Pattern mirrors mint_result_java.py (PR #694).
This is N-edge-only: the concept:option-bind abstraction is already on main (PR #682).

Mints:
  (A) RealizationDesugaringMemento: concept:option-bind -> java:optional-flat-map  (the N edge)

The Java realization of concept:option-bind uses java.util.Optional.flatMap():
  Optional<T> opt = ...;
  Optional<U> result = opt.flatMap(f);

java.util.Optional.flatMap(Function<T, Optional<U>>) is the canonical Java
monadic bind for Optional. It satisfies:
  Optional.of(v).flatMap(f) == f.apply(v)
  Optional.empty().flatMap(f) == Optional.empty()

All CIDs are BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).
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


def var(name):
    return {"kind": "var", "name": name}


def op(name, args):
    return {"kind": "op", "name": name, "args": args}


def ctor(name):
    return {"kind": "ctor", "name": name, "args": []}


def build_realization_option_bind_java():
    """
    N edge: concept:option-bind -> java:optional-flat-map.

    Java idiom: Optional<T>.flatMap(Function<T, Optional<U>>).
      Optional<U> result = opt.flatMap(f);

    Loss record (5-dimension):

    structural_divergence:
      Java's Optional is a library class (not a language primitive); flatMap
      is a method call on Optional, not an infix operator; the function f
      must return Optional<U> (not a raw value); Java Optional is reference-
      typed so empty() uses null internally; the flatMap method is in
      java.util (Java 8+), not available in pre-Java-8 code.

    domain_narrowing:
      Java Optional cannot hold null as a "present" value; Optional.of(null)
      throws NullPointerException; programs using null as a valid T value cannot
      use Optional and are in the narrowed-out domain; the realization requires
      non-null wrapped values throughout the option chain.

    ub_introduction:
      none: Java has no undefined behavior; Optional.flatMap is always safe;
      empty Optional propagates cleanly; no memory corruption.

    effect_divergence:
      none: Optional.flatMap is a pure functional operation with no side effects
      beyond what f introduces; f is called at most once; no resource acquisition.

    value_divergence:
      none: Optional.of(v).flatMap(f) == f.apply(v) and
      Optional.empty().flatMap(f) == Optional.empty() hold exactly;
      monadic laws are satisfied; no value-representation gap.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:option-bind->java:optional-flat-map",
        "formals": ["T", "U"],
        "formal_sorts": [
            ctor("T"),
            ctor("U"),
        ],
        "post": {
            "lhs": op("concept:option-bind", [var("T"), var("U")]),
            "rhs": op(
                "java:optional-flat-map",
                [
                    op("java:method-call", [
                        {"kind": "const", "value": "Optional<T>", "sort": ctor("ReceiverType")},
                        {"kind": "const", "value": "flatMap", "sort": ctor("MethodName")},
                        op("java:function-type", [
                            var("T"),
                            op("java:optional-type", [var("U")]),
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
                "Java Optional is a library class, not a language primitive; "
                "flatMap is a method call not an infix operator; "
                "f must return Optional<U> not a raw value; "
                "requires Java 8+; not available in pre-Java-8 code"
            ),
            "domain_narrowing": (
                "Optional cannot hold null as a present value; "
                "Optional.of(null) throws NullPointerException; "
                "programs using null as a valid T value are in the narrowed-out domain; "
                "realization requires non-null wrapped values throughout the option chain"
            ),
            "ub_introduction": (
                "none: Java has no undefined behavior; "
                "Optional.flatMap is always safe; "
                "empty Optional propagates cleanly; no memory corruption"
            ),
            "effect_divergence": (
                "none: Optional.flatMap is a pure functional operation; "
                "f is called at most once; no resource acquisition or I/O"
            ),
            "value_divergence": (
                "none: Optional.of(v).flatMap(f) == f.apply(v) and "
                "Optional.empty().flatMap(f) == Optional.empty() hold exactly; "
                "monadic laws satisfied; no value-representation gap"
            ),
        },
        "discharge_receipt": None,
        "effects": [],
    }


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

    print("[N] Minting concept:option-bind->java:optional-flat-map realization (N edge)...")
    real_memento = build_realization_option_bind_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:option-bind->java:optional-flat-map.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:option-bind->java:optional-flat-map: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:option-bind->java:optional-flat-map", "cid": real_cid, "path": str(real_path)})

    print("\n[STABILITY] Re-minting realization for byte-stability check...")
    check_cid = compute_cid(real_memento)
    if check_cid != real_cid:
        raise SystemExit(f"ABORTING: CID instability: {check_cid} != {real_cid}")
    print(f"  STABLE: realization: ok")

    append_cids_tsv(cid_rows)
    print(f"\n[DONE] realization (N edge) CID: {real_cid}")
    return real_cid


if __name__ == "__main__":
    mint_all()
