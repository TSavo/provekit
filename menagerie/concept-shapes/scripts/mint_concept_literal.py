#!/usr/bin/env python3
"""Mint the classical concept:literal value-tier operation."""

from __future__ import annotations

import json

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CATALOG_DIR = discharge.CATALOG_REAL
CID_FILE = discharge.CID_FILE

SPEC_FILENAME = "concept_literal_shape.spec.json"


def ctor(name: str, args: list[dict] | None = None) -> dict:
    return {"args": args or [], "kind": "ctor", "name": name}


def true_formula() -> dict:
    return {"args": [], "kind": "atomic", "name": "true"}


def build_concept_literal_spec() -> dict:
    return {
        "effects": {"effects": []},
        "fn_name": "concept:literal",
        "formal_sorts": [],
        "formals": [],
        "kind": "algorithm",
        "post": {
            "arity": [],
            "arity_shape": {
                "kind": "named",
                "slots": [],
            },
            "kind": "operation-contract",
            "leaf_shape": {
                "fields": [
                    {
                        "name": "value",
                        "role": "decoded logical literal value",
                    },
                    {
                        "name": "sort",
                        "role": "substrate-canonical sort CID",
                    },
                ],
                "kind": "named",
            },
            "operator": "literal",
            "result": "Term",
            "slot_terms": [],
            "wp_note": (
                "Classical value-tier literal leaf. The operation has zero operands; "
                "value and sort are properties of the term node, and sort cites a "
                "substrate-canonical sort CID from catalog/sorts."
            ),
        },
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


def catalog_index_path(path: str) -> str:
    prefix = "menagerie/concept-shapes/catalog/"
    return path[len(prefix) :] if path.startswith(prefix) else path


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


def mint_all() -> dict:
    discharge.build_tools()
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    spec = build_concept_literal_spec()
    discharge.write_json(SPEC_DIR / SPEC_FILENAME, spec)
    cid, path = discharge.mint("algorithm", SPEC_FILENAME)
    row = {"kind": "shape", "name": spec["fn_name"], "cid": cid, "path": path}
    append_cid_row(row)
    update_catalog_index(spec["fn_name"], cid, catalog_index_path(path))
    discharge.scan_created_text()
    print(f"concept_literal_shape_cid\t{row['name']}\t{row['cid']}")
    return row


if __name__ == "__main__":
    mint_all()
