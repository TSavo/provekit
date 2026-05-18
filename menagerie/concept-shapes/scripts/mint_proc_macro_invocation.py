#!/usr/bin/env python3
"""Mint procedural macro invocation concept shapes."""

from __future__ import annotations

import json

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CATALOG_DIR = discharge.CATALOG_REAL
CID_FILE = discharge.CID_FILE


def ctor(name: str) -> dict:
    return {"args": [], "kind": "ctor", "name": name}


def var(name: str) -> dict:
    return {"kind": "var", "name": name}


def true_formula() -> dict:
    return {"args": [], "kind": "atomic", "name": "true"}


def operation_contract(
    operator: str,
    arity: list[str],
    result: str,
    slots: list[str],
    wp_note: str,
) -> dict:
    return {
        "arity": arity,
        "arity_shape": {
            "kind": "named",
            "slots": slots,
        },
        "kind": "operation-contract",
        "operator": operator,
        "result": result,
        "slot_terms": [var(slot) for slot in slots],
        "wp_note": wp_note,
    }


def build_proc_macro_invocation_spec() -> dict:
    formals = ["macro_cid", "args", "token_stream"]
    return {
        "effects": {"effects": []},
        "fn_name": "concept:proc-macro-invocation",
        "formal_sorts": [ctor("Cid"), ctor("List<Term>"), ctor("TokenStream")],
        "formals": formals,
        "kind": "algorithm",
        "post": operation_contract(
            "proc-macro-invocation",
            ["Cid", "List<Term>", "TokenStream"],
            "Term",
            formals,
            (
                "Carries a source-visible procedural macro invocation without expanding "
                "its token stream. macro_cid identifies the macro surface, args carries "
                "parsed argument terms when available, and token_stream preserves the "
                "lossless source tokens."
            ),
        ),
        "pre": true_formula(),
        "return_sort": ctor("Term"),
    }


def build_derive_attribute_spec() -> dict:
    formals = ["macro_cid", "traits", "token_stream"]
    return {
        "effects": {"effects": []},
        "fn_name": "concept:derive-attribute",
        "formal_sorts": [ctor("Cid"), ctor("List<Term>"), ctor("TokenStream")],
        "formals": formals,
        "kind": "algorithm",
        "post": operation_contract(
            "derive-attribute",
            ["Cid", "List<Term>", "TokenStream"],
            "Term",
            formals,
            (
                "Typed subcase of proc-macro-invocation for Rust derive attributes. "
                "traits carries the derived trait paths as terms and token_stream "
                "preserves the original attribute tokens."
            ),
        ),
        "pre": true_formula(),
        "return_sort": ctor("Term"),
    }


def append_cid_row(row: dict) -> None:
    existing = (
        CID_FILE.read_text(encoding="utf-8").splitlines()
        if CID_FILE.exists()
        else ["kind\tname\tcid\tpath"]
    )
    seen: dict[tuple[str, str], str] = {}
    for line in existing[1:]:
        parts = line.split("\t")
        if len(parts) >= 3:
            seen[(parts[0], parts[1])] = parts[2]

    key = (row["kind"], row["name"])
    if key in seen:
        if seen[key] != row["cid"]:
            raise SystemExit(
                f"one-name-one-CID violation: {row['kind']} {row['name']} "
                f"already registered as {seen[key]!r} but new mint produced {row['cid']!r}"
            )
        return

    existing.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
    CID_FILE.write_text("\n".join(existing) + "\n", encoding="utf-8")


def catalog_algorithm_path(name: str, cid: str) -> str:
    path = CATALOG_DIR / "algorithms" / f"{name}.{cid}.json"
    return discharge.rel_path(path)


def catalog_index_path(path: str) -> str:
    prefix = "menagerie/concept-shapes/catalog/"
    return path[len(prefix):] if path.startswith(prefix) else path


def algorithm_payload(spec: dict) -> dict:
    return {
        "auto_minted_mementos": [],
        "effects": spec["effects"],
        "fn_name": spec["fn_name"],
        "formal_sorts": spec["formal_sorts"],
        "formals": spec["formals"],
        "kind": "AlgorithmMemento",
        "post": spec["post"],
        "pre": spec["pre"],
        "protocol": "AMP",
        "return_sort": spec["return_sort"],
        "schema_version": "1",
    }


def mint_algorithm_spec(filename: str, spec: dict) -> dict:
    discharge.write_json(SPEC_DIR / filename, spec)
    payload = algorithm_payload(spec)
    cid = discharge.canonical_cid_value(payload)
    path = catalog_algorithm_path(spec["fn_name"], cid)
    catalog_path = BASE / "catalog" / catalog_index_path(path)
    discharge.write_json(
        catalog_path,
        {
            "cid": cid,
            "memento": payload,
            "signature": None,
        },
    )
    update_catalog_index(spec["fn_name"], cid, catalog_index_path(path))
    return {"kind": "shape", "name": spec["fn_name"], "cid": cid, "path": path}


def update_catalog_index(name: str, cid: str, path: str) -> None:
    index_path = CATALOG_DIR / "index.json"
    if index_path.exists():
        index = discharge.read_json(index_path)
    else:
        index = {
            "entries": {},
            "schema_version": "provekit-algebraic-catalog-index/1",
        }
    entries = index.setdefault("entries", {})
    entries[cid] = {
        "cid": cid,
        "kind": "algorithm",
        "name": name,
        "path": path,
    }
    ordered = {
        "schema_version": index.get(
            "schema_version", "provekit-algebraic-catalog-index/1"
        ),
        "entries": {key: entries[key] for key in sorted(entries)},
    }
    index_path.write_text(
        json.dumps(ordered, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )


def update_readme(rows: list[dict]) -> None:
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8")
    start = "## Proc Macro Invocation Concept Shapes\n"
    if start in text:
        text = text[: text.index(start)].rstrip() + "\n"

    cid_by_name = {row["name"]: row["cid"] for row in rows}
    section = [
        "## Proc Macro Invocation Concept Shapes",
        "",
        "`concept:proc-macro-invocation(macro_cid, args, token_stream)` carries procedural macro syntax without expansion.",
        "`concept:derive-attribute(macro_cid, traits, token_stream)` is the typed Rust derive subcase.",
        "",
        "| Concept | Shape CID | Notes |",
        "| --- | --- | --- |",
        f"| `concept:proc-macro-invocation` | `{cid_by_name['concept:proc-macro-invocation']}` | carries macro CID, parsed args, and lossless source tokens |",
        f"| `concept:derive-attribute` | `{cid_by_name['concept:derive-attribute']}` | typed derive attribute subcase with trait path terms |",
    ]
    readme.write_text(text.rstrip() + "\n\n" + "\n".join(section) + "\n", encoding="utf-8")


def mint_all() -> list[dict]:
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    specs = [
        ("proc-macro-invocation_shape.spec.json", build_proc_macro_invocation_spec()),
        ("derive-attribute_shape.spec.json", build_derive_attribute_spec()),
    ]
    rows = []
    for filename, spec in specs:
        row = mint_algorithm_spec(filename, spec)
        append_cid_row(row)
        rows.append(row)
        print(f"proc_macro_invocation_shape_cid\t{row['name']}\t{row['cid']}")
    update_readme(rows)
    discharge.scan_created_text()
    return rows


if __name__ == "__main__":
    mint_all()
