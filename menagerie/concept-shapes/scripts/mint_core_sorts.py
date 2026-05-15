#!/usr/bin/env python3
"""Mint the core CCL sort hierarchy primitives."""

from __future__ import annotations

import re

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

SORT_OP_SPEC_FILENAME = "sort_shape.spec.json"

SORTS = [
    ("Int", "primitive"),
    ("Bool", "primitive"),
    ("String", "primitive"),
    ("Bytes", "primitive"),
    ("Cid", "primitive"),
    ("OpCid", "primitive"),
    ("SortCid", "primitive"),
    ("EffectName", "primitive"),
    ("Formula", "primitive"),
    ("Term", "primitive"),
    ("List<T>", "parametric"),
    ("Map<K,V>", "parametric"),
]


def ctor(name: str) -> dict:
    return {"args": [], "kind": "ctor", "name": name}


def var(name: str) -> dict:
    return {"kind": "var", "name": name}


def true_formula() -> dict:
    return {"args": [], "kind": "atomic", "name": "true"}


def operation_contract(operator: str, arity: list[str], result: str, slots: list[str], wp_note: str) -> dict:
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


def effect_signature(name: str) -> dict:
    return {"kind": "effect-signature", "name": name}


def build_sort_op_spec() -> dict:
    formals = ["name", "kind"]
    post = operation_contract(
        "sort",
        ["String", "String"],
        "SortCid",
        formals,
        (
            "Mints a sort instance citation. Well-formed iff name is a non-empty string "
            "and kind is one of {primitive, parametric, refined, reference}."
        ),
    )

    return {
        "effects": {"effects": []},
        "fn_name": "concept:sort",
        "formal_sorts": [ctor("String"), ctor("String")],
        "formals": formals,
        "kind": "algorithm",
        "post": post,
        "pre": true_formula(),
        "return_sort": ctor("SortCid"),
    }


def build_sort_instance(name: str, kind: str) -> dict:
    return {
        "effects": {"effects": []},
        "fn_name": name,
        "formal_sorts": [],
        "formals": [],
        "kind": "sort",
        "post": {
            "kind": "sort-instance",
            "name": name,
            "sort_kind": kind,
        },
        "pre": true_formula(),
        "return_sort": ctor("SortCid"),
    }


def sort_filename(name: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9]+", "_", name).strip("_").lower()
    return f"sort-instance_{slug}.spec.json"


def append_cid_row(row: dict) -> None:
    existing = CID_FILE.read_text(encoding="utf-8").splitlines() if CID_FILE.exists() else ["kind\tname\tcid\tpath"]
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


def replace_readme_section(text: str, heading: str, section: list[str]) -> str:
    marker = f"## {heading}\n"
    if marker not in text:
        return text.rstrip() + "\n\n" + "\n".join(section) + "\n"

    start = text.index(marker)
    next_start = text.find("\n## ", start + len(marker))
    if next_start == -1:
        return text[:start].rstrip() + "\n\n" + "\n".join(section) + "\n"
    return text[:start].rstrip() + "\n\n" + "\n".join(section) + text[next_start:]


def update_readme(rows: list[dict]) -> None:
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8")

    section = [
        "## Core Sort Hierarchy",
        "",
        "`concept:sort(name, kind)` describes the core sort hierarchy primitive and the initial sort instances.",
        "",
        "| Concept | Kind | CID |",
        "| --- | --- | --- |",
    ]
    for row in rows:
        section.append(f"| `{row['name']}` | `{row['kind']}` | `{row['cid']}` |")

    readme.write_text(replace_readme_section(text, "Core Sort Hierarchy", section), encoding="utf-8")


def mint_sort_instance(filename: str) -> tuple[str, str]:
    try:
        return discharge.mint("sort-instance", filename)
    except SystemExit as exc:
        message = str(exc)
        if "unrecognized subcommand" not in message and "invalid value" not in message:
            raise
        return discharge.mint("sort", filename)


def mint_all() -> list[dict]:
    SPEC_DIR.mkdir(parents=True, exist_ok=True)

    op_spec = build_sort_op_spec()
    discharge.write_json(SPEC_DIR / SORT_OP_SPEC_FILENAME, op_spec)
    op_cid, op_path = discharge.mint("algorithm", SORT_OP_SPEC_FILENAME)
    rows = [{"kind": "shape", "name": op_spec["fn_name"], "cid": op_cid, "path": op_path}]

    for name, kind in SORTS:
        filename = sort_filename(name)
        discharge.write_json(SPEC_DIR / filename, build_sort_instance(name, kind))
        cid, path = mint_sort_instance(filename)
        rows.append({"kind": "sort-instance", "name": name, "cid": cid, "path": path})

    for row in rows:
        append_cid_row(row)
    update_readme(rows)
    discharge.scan_created_text()

    for row in rows:
        print(f"core_sort_cid\t{row['kind']}\t{row['name']}\t{row['cid']}")
    return rows


if __name__ == "__main__":
    mint_all()
