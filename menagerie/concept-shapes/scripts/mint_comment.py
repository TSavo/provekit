#!/usr/bin/env python3
"""Mint the concept:comment trivia op."""

from __future__ import annotations

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

SPEC_FILENAME = "comment_shape.spec.json"


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


def build_shape_spec() -> dict:
    post = operation_contract(
        "comment",
        ["String"],
        "Stmt",
        ["surface"],
        (
            "Carries source comment trivia as a first-class statement concept. "
            "The operation has no runtime effect; formatters own surrounding whitespace."
        ),
    )

    return {
        "effects": {"effects": []},
        "fn_name": "concept:comment",
        "formal_sorts": [ctor("String")],
        "formals": ["surface"],
        "kind": "algorithm",
        "post": post,
        "pre": true_formula(),
        "return_sort": ctor("Stmt"),
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


def update_readme(row: dict) -> None:
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8")
    start = "## Comment Concept Shape\n"
    section = [
        "## Comment Concept Shape",
        "",
        "`concept:comment(surface)` carries source comments through lift, bind, and lower as trivia with no runtime effect.",
        "",
        "| Concept | Shape CID | Notes |",
        "| --- | --- | --- |",
        f"| `concept:comment` | `{row['cid']}` | surface is the raw comment delimiter text; formatters own whitespace |",
    ]
    section_text = "\n".join(section)
    if start not in text:
        readme.write_text(text.rstrip() + "\n\n" + section_text + "\n", encoding="utf-8")
        return

    section_start = text.index(start)
    next_section = text.find("\n## ", section_start + len(start))
    prefix = text[:section_start].rstrip()
    if next_section == -1:
        updated = prefix + "\n\n" + section_text + "\n"
    else:
        suffix = text[next_section:].lstrip("\n")
        updated = prefix + "\n\n" + section_text + "\n\n" + suffix
    readme.write_text(updated, encoding="utf-8")


def mint_all() -> dict:
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    spec = build_shape_spec()
    discharge.write_json(SPEC_DIR / SPEC_FILENAME, spec)
    cid, path = discharge.mint("algorithm", SPEC_FILENAME)
    row = {"kind": "shape", "name": spec["fn_name"], "cid": cid, "path": path}
    append_cid_row(row)
    update_readme(row)
    discharge.scan_created_text()
    print(f"comment_shape_cid\t{row['name']}\t{row['cid']}")
    return row


if __name__ == "__main__":
    mint_all()
