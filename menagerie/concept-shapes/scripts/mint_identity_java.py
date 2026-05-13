#!/usr/bin/env python3
"""
mint_identity_java.py -- mint concept:identity -> java realization (N edge).

Pattern mirrors PR #671 (concept:identity-c) and #674 (concept:result->java).
This is N-edge-only: the concept:identity abstraction is already on main (PR #680).
The M-edge (rust:identity->concept:identity) is also already on main.

Mints:
  (A) RealizationDesugaringMemento: concept:identity -> java  (the N edge)

CID is BLAKE3-512 via compute_fixture_cid.
discharge_receipt is null (PR1 form, N-edge-only).

Loss record (concrete, 5-dimension):
  - structural_divergence: lambda x -> x vs direct function reference
  - domain_narrowing: no generic variance constraints in Java
  - ub_introduction: none (Function.identity() is safe)
  - effect_divergence: none (pure function)
  - value_divergence: Java functional interface vs Rust trait object

The Java identity idiom (two equivalent forms):
  1. Function.identity()        -- standard library singleton from java.util.function
  2. x -> x                      -- lambda expression (Java 8+)

Both are zero-loss realizations; Function.identity() is the canonical form.
Pattern matching (Java 16+) with records or sealed types can use identity in filters.
Pre-Java 8: identity pattern not available in standard library; manual function class required.
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
# (A) RealizationDesugaringMemento: concept:identity -> java  (N edge)
# ---------------------------------------------------------------------------

def build_realization_identity_java():
    """
    N edge: concept:identity -> java (Function.identity() or lambda x -> x).

    The Java realization encodes concept:identity as either:

      1. Function.identity()          -- canonical form from java.util.function
      2. (Object x) -> x              -- lambda expression (Java 8+)

    Function.identity() is a static factory method returning a Function<T, T>
    that returns its argument unchanged. This is the standard library's
    type-safe, generic identity function.

    The lambda x -> x is syntactically equivalent and may be preferred
    in some contexts for clarity or to avoid import statements.

    Loss record (5-dimension):

    structural_divergence:
      The abstraction is a mathematical identity operation on any type T.
      Java realization encodes this as either a static method reference
      (Function.identity()) or a lambda expression (x -> x). Both preserve
      type information through generics <T>. The abstraction is silent on
      implementation strategy (method vs lambda); both are valid.

    domain_narrowing:
      Java 8+ provides Function<T, T> with identity(). Pre-Java 8,
      no standard library identity function exists; manual lambda or
      Function subclass required. The realization narrows the domain to
      Java 8+ environments. Earlier JDKs require custom implementation.

    ub_introduction:
      None. Function.identity() and lambda x -> x are pure, safe operations.
      No memory unsafety, no null dereferences (function itself is non-null;
      argument nullability is type-dependent and preserved from abstraction).

    effect_divergence:
      None. Function.identity() and lambda expressions are pure; they
      introduce no side effects, I/O, or mutable state.

    value_divergence:
      Java's Function<T, T> is a functional interface (nominal type with
      one abstract method: T apply(T)). Rust's identity is a function item
      (structural, zero-size type). Java realization depends on the JDK's
      Function interface and the runtime polymorphism of method invocation.
      Rust realization is a compile-time function with zero runtime overhead.
    """
    return {
        "kind": "equation",
        "fn_name": "concept:identity->java:function-identity",
        "formals": ["x"],
        "formal_sorts": [
            ctor("T"),
        ],
        "post": {
            "lhs": op("concept:identity", [var("x")]),
            "rhs": op(
                "java:function-identity",
                [
                    op("java:function-factory", [
                        {"kind": "const", "value": "Function.identity()", "sort": ctor("MethodReference")},
                        {"kind": "const", "value": "x -> x", "sort": ctor("LambdaAlternative")},
                    ]),
                    op("java:type-parameter", [
                        {"kind": "const", "value": "T", "sort": ctor("TypeVar")},
                    ]),
                ],
            ),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": "java",
        "loss_record": {
            "structural_divergence": (
                "Java encodes identity as Function.identity() (static factory) or x -> x (lambda); "
                "both preserve type T through generics <T>; abstraction is silent on implementation; "
                "method reference vs lambda is a stylistic choice with no semantic difference"
            ),
            "domain_narrowing": (
                "Java 8+ required for Function<T, T> and lambda expressions; "
                "pre-Java 8 requires manual Function subclass or equivalent; "
                "realization narrows domain to Java 8+ environments"
            ),
            "ub_introduction": (
                "none: Function.identity() and lambda x -> x are pure safe operations; "
                "no memory unsafety, no null dereferences; argument nullability "
                "is type-dependent and preserved from abstraction"
            ),
            "effect_divergence": (
                "none: Function.identity() and lambda expressions are pure; "
                "no side effects, I/O, or mutable state"
            ),
            "value_divergence": (
                "Java's Function<T, T> is a nominal interface type with one method apply(T): T; "
                "Rust's identity is a structural function item with zero size; "
                "Java realization depends on JDK Function interface and runtime polymorphism; "
                "Rust realization is compile-time function with zero runtime overhead"
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

    print("[N] Minting concept:identity->java:function-identity realization (N edge)...")
    real_memento = build_realization_identity_java()
    real_entry, real_cid = catalog_entry(real_memento)
    real_path = REAL_DIR / f"concept:identity->java:function-identity.{real_cid}.json"
    write_json(real_path, real_entry)
    print(f"  concept:identity->java:function-identity: {real_cid[:40]}...")
    cid_rows.append(
        {
            "kind": "realization",
            "name": "concept:identity->java:function-identity",
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
