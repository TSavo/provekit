#!/usr/bin/env python3
"""
mint_operation_realizations.py -- mint concept hubs + rust/java realizations for
operations currently hand-coded in TermShapeLifter (path B / #1369 follow-on).

Ops minted (each: 1 abstraction + rust realization + java realization):
  - concept:utf8-encode               (String -> Bytes)
  - concept:format-string-interp       (FormatString * List[Value] -> String)
  - concept:json-text-coerce           (Json -> Option[String])
  - concept:list-create                (Unit -> List)
  - concept:option-is-some             (Option -> Bool)
  - concept:map-create                 (Unit -> Map)

Schema mirrors mint_assert.py: equation memento with post.lhs/post.rhs,
role=abstraction-realization, direction=left-to-right, target_lang set.
The rhs op carries the kit-specific *operator name* the lifter/realizer
will key off (e.g. java:string-getBytes-utf8, rust:str-as-bytes).
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
BINARY = Path("/Users/tsavo/provekit/implementations/rust/target/debug/compute_fixture_cid")
if not BINARY.exists():
    sys.exit("compute_fixture_cid binary not found")

DEFERRED_RECEIPT = "deferred:pending-pk-1391"
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


def var(name): return {"kind": "var", "name": name}
def op(name, args): return {"kind": "op", "name": name, "args": args}
def ctor(name, args=None): return {"kind": "ctor", "name": name, "args": args or []}


# ---------------------------------------------------------------------------
# Concept abstractions
# ---------------------------------------------------------------------------

def make_abstraction(operator, formals, formal_sorts, result_sort, note):
    return {
        "kind": "concept-abstraction",
        "operator": operator,
        "tier": "abstraction",
        "slots": [],
        "formals": formals,
        "formal_sorts": formal_sorts,
        "result_sort": result_sort,
        "contract": {"kind": "wp-rule", "formals": [], "body": {"kind": "atomic", "name": "true", "args": []}},
        "contract_note": note,
        "realizations": [],
    }


def make_realization(fn_name, formals, formal_sorts, lhs_op, rhs_op, target_lang, target_library, loss_note):
    formal_vars = [var(f) for f in formals]
    return {
        "kind": "equation",
        "fn_name": fn_name,
        "formals": formals,
        "formal_sorts": formal_sorts,
        "post": {
            "lhs": op(lhs_op, formal_vars),
            "rhs": op(rhs_op, formal_vars),
        },
        "role": "abstraction-realization",
        "direction": "left-to-right",
        "target_lang": target_lang,
        "target_library": target_library,
        "loss_record": {
            "structural_divergence": loss_note,
        },
        "discharge_receipt": DEFERRED_RECEIPT,
        "effects": [],
    }


# ---------------------------------------------------------------------------
# Per-concept specs
# ---------------------------------------------------------------------------

# (operator, formals, formal_sorts, result_sort, note, rust_op, rust_loss, java_op, java_loss)
SPECS = [
    (
        "concept:utf8-encode",
        ["s"], [ctor("String")], ctor("Bytes"),
        "Encode a string as UTF-8 bytes. Total function.",
        "rust:str-as-bytes",
        "rust .as_bytes() returns &[u8] view; concept models owned Bytes",
        "java:string-getBytes-utf8",
        "java getBytes(StandardCharsets.UTF_8) allocates new byte[]",
    ),
    (
        "concept:format-string-interp",
        ["fmt", "args"], [ctor("FormatString"), ctor("ListOfValue")], ctor("String"),
        "Format-string interpolation: substitute positional args into fmt placeholders.",
        "rust:format-macro",
        "rust format!() is a proc-macro; placeholder syntax {} differs from java %s",
        "java:string-format-static",
        "java String.format() varargs; %s placeholders; ParseException on bad fmt",
    ),
    (
        "concept:json-text-coerce",
        ["v"], [ctor("Json")], ctor("OptionOfString"),
        "Coerce a JSON value to a text string; returns None if not a string.",
        "rust:serde-value-as-str",
        "rust Value::as_str returns Option<&str>",
        "java:jackson-jsonnode-asText",
        "java JsonNode.asText() returns empty string for null (lossy); use is_null() check upstream",
    ),
    (
        "concept:list-create",
        [], [], ctor("List"),
        "Construct an empty list.",
        "rust:vec-new",
        "rust Vec::new() allocates 0-capacity heap vector",
        "java:array-list-new",
        "java new ArrayList<>() allocates default-capacity (10) array-backed list",
    ),
    (
        "concept:option-is-some",
        ["o"], [ctor("OptionOfA")], ctor("Bool"),
        "Predicate: is the option present (Some)?",
        "rust:option-is-some",
        "rust Option::is_some() borrows option",
        "java:objects-nonnull",
        "java Objects.nonNull(v) inverts null-check; nullable reference is the option encoding",
    ),
    (
        "concept:map-create",
        [], [], ctor("Map"),
        "Construct an empty map.",
        "rust:hashmap-new",
        "rust HashMap::new() default hasher (SipHash-1-3 randomised)",
        "java:hashmap-new",
        "java new HashMap<>() default load factor 0.75, capacity 16",
    ),
]


def mint_all():
    ABST_DIR.mkdir(parents=True, exist_ok=True)
    REAL_DIR.mkdir(parents=True, exist_ok=True)

    cid_rows = []

    for spec in SPECS:
        (operator, formals, formal_sorts, result_sort, note,
         rust_op, rust_loss, java_op, java_loss) = spec

        # Rust realization
        rust_memento = make_realization(
            f"{operator}->{rust_op}", formals, formal_sorts,
            operator, rust_op, "rust", "rust-language-builtin", rust_loss,
        )
        rust_entry, rust_cid = catalog_entry(rust_memento)
        rust_path = REAL_DIR / f"{operator}->{rust_op}.{rust_cid}.json"
        write_json(rust_path, rust_entry)
        print(f"  {operator}->{rust_op}: {rust_cid[:40]}...")
        cid_rows.append({"kind": "realization", "name": f"{operator}->{rust_op}", "cid": rust_cid, "path": str(rust_path)})

        # Java realization
        java_memento = make_realization(
            f"{operator}->{java_op}", formals, formal_sorts,
            operator, java_op, "java", "java-language-builtin", java_loss,
        )
        java_entry, java_cid = catalog_entry(java_memento)
        java_path = REAL_DIR / f"{operator}->{java_op}.{java_cid}.json"
        write_json(java_path, java_entry)
        print(f"  {operator}->{java_op}: {java_cid[:40]}...")
        cid_rows.append({"kind": "realization", "name": f"{operator}->{java_op}", "cid": java_cid, "path": str(java_path)})

        # Concept abstraction (with both realization CIDs populated)
        abst_memento = make_abstraction(operator, formals, formal_sorts, result_sort, note)
        abst_memento["realizations"] = [rust_cid, java_cid]
        abst_entry, abst_cid = catalog_entry(abst_memento)
        abst_path = ABST_DIR / f"{operator}.{abst_cid}.json"
        write_json(abst_path, abst_entry)
        print(f"  {operator} (abstraction): {abst_cid[:40]}...")
        cid_rows.append({"kind": "abstraction", "name": operator, "cid": abst_cid, "path": str(abst_path)})

    # Append to cids.tsv
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
    for row in cid_rows:
        key = (row["kind"], row["name"])
        if key not in seen:
            existing_lines.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
            seen.add(key)
    CID_FILE.write_text("\n".join(existing_lines) + "\n", encoding="utf-8")

    print(f"\n[DONE] Minted {len(cid_rows)} entries ({len(SPECS)} concepts x 3 mementos each).")


if __name__ == "__main__":
    mint_all()
