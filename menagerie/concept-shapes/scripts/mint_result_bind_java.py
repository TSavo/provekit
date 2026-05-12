#!/usr/bin/env python3
"""
mint_result_bind_java.py -- mint concept:result-bind -> java realization (N edge).

Pattern mirrors mint_result_java.py (PR #694).
This is N-edge-only: the concept:result-bind abstraction is already on main (PR #683).

Mints:
  (A) RealizationDesugaringMemento: concept:result-bind -> java:result-bind-switch  (the N edge)

The Java realization of concept:result-bind uses a sealed-switch pattern:
  Result<U,E> result_bind(Result<T,E> result, Function<T, Result<U,E>> f) {
      return switch (result) {
          case Result.Success<T,E>(T v) -> f.apply(v);
          case Result.Failure<T,E>(E e) -> new Result.Failure<>(e);
      };
  }

This pairs with the Java concept:result realization (sealed interface, PR #694).
Available from Java 21+ with pattern matching for switch.

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


def build_realization_result_bind_java():
    """
    N edge: concept:result-bind -> java:result-bind-switch.

    Java idiom: sealed-switch dispatch over the sealed Result interface.
      return switch (result) {
          case Result.Success<T,E>(T v) -> f.apply(v);
          case Result.Failure<T,E>(E e) -> new Result.Failure<>(e);
      };

    Pairs with concept:result -> java:sealed-interface (PR #694).
    Available in Java 21+ with sealed type pattern matching.

    Loss record (5-dimension):

    structural_divergence:
      Java has no native monadic bind operator; the bind must be a user-defined
      method using sealed-type pattern matching; the switch expression requires
      Java 21+ pattern matching on sealed types; pre-Java-21 requires instanceof
      checking which is more verbose; the bind is not a method on the Result type
      (no monad typeclass in Java), requiring a standalone function.

    domain_narrowing:
      the realization depends on the sealed-interface Result from PR #694;
      programs using other Result shapes (e.g., Optional-based, exception-based)
      are in the narrowed-out domain; the sealed switch also requires Java 21+
      for exhaustiveness checking; pre-21 code lacks static exhaustiveness.

    ub_introduction:
      none: Java has no undefined behavior; sealed-switch is always exhaustive
      (compiler-enforced in Java 21+); no memory corruption or undefined state.

    effect_divergence:
      none: the bind method is a pure function; f is called at most once (only
      on the Success branch); no resource acquisition or I/O introduced by
      the bind itself.

    value_divergence:
      none: bind(Success(v), f) == f.apply(v) and bind(Failure(e), f) == Failure(e)
      hold exactly; error type E is preserved on the Failure branch;
      monadic associativity law is satisfied.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:result-bind->java:result-bind-switch",
        "formals": ["result", "f"],
        "formal_sorts": [
            ctor("T"),
            ctor("E"),
            ctor("U"),
        ],
        "post": {
            "lhs": op("concept:result-bind", [var("result"), var("f")]),
            "rhs": op(
                "java:result-bind-switch",
                [
                    op("java:sealed-switch", [
                        var("result"),
                        op("java:switch-arm", [
                            {"kind": "const", "value": "case Result.Success<T,E>(T v) -> f.apply(v)", "sort": ctor("OkArm")},
                        ]),
                        op("java:switch-arm", [
                            {"kind": "const", "value": "case Result.Failure<T,E>(E e) -> new Result.Failure<>(e)", "sort": ctor("ErrArm")},
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
                "Java has no native monadic bind operator; bind is user-defined; "
                "requires Java 21+ sealed-type pattern matching for exhaustiveness; "
                "pre-Java-21 requires instanceof checking; "
                "bind is a standalone function, not a method on Result type"
            ),
            "domain_narrowing": (
                "realization depends on the sealed-interface Result from PR #694; "
                "other Result shapes (Optional-based, exception-based) are narrowed-out; "
                "sealed switch requires Java 21+ for static exhaustiveness checking; "
                "pre-21 code lacks exhaustiveness enforcement"
            ),
            "ub_introduction": (
                "none: Java has no undefined behavior; "
                "sealed switch is exhaustive (compiler-enforced in Java 21+); "
                "no memory corruption or undefined state"
            ),
            "effect_divergence": (
                "none: bind is a pure function; f is called at most once (Success branch); "
                "no resource acquisition or I/O introduced by bind itself"
            ),
            "value_divergence": (
                "none: bind(Success(v), f) == f.apply(v) and "
                "bind(Failure(e), f) == Failure(e) hold exactly; "
                "error type E preserved on Failure branch; "
                "monadic associativity satisfied"
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

    print("[N] Minting concept:result-bind->java:result-bind-switch realization (N edge)...")
    real_memento = build_realization_result_bind_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:result-bind->java:result-bind-switch.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:result-bind->java:result-bind-switch: {real_cid[:40]}...")
    cid_rows.append({"kind": "realization", "name": "concept:result-bind->java:result-bind-switch", "cid": real_cid, "path": str(real_path)})

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
