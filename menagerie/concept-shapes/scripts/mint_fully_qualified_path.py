#!/usr/bin/env python3
"""Mint the concept:fully-qualified-path hub op."""

from __future__ import annotations

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

SPEC_FILENAME = "fully-qualified-path_shape.spec.json"


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


def build_shape_spec() -> dict:
    formals = ["path"]
    post = operation_contract(
        "fully-qualified-path",
        ["String"],
        "Path",
        formals,
        (
            "Carries a Rust path exactly as resolved or structurally qualified by "
            "the lifter, including module, trait, associated item, and crate-root "
            "segments."
        ),
    )

    return {
        "effects": {"effects": []},
        "fn_name": "concept:fully-qualified-path",
        "formal_sorts": [ctor("String")],
        "formals": formals,
        "kind": "algorithm",
        "path_roles": ["module", "trait", "associated-item", "crate-root"],
        "post": post,
        "pre": true_formula(),
        "return_sort": ctor("Path"),
    }


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


def update_readme(row: dict) -> None:
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8")

    section = [
        "## Fully Qualified Path Concept Shape",
        "",
        "`concept:fully-qualified-path(path)` is the hub op for Rust paths whose module, trait, crate-root, and associated item segments must survive lifting.",
        "",
        "| Concept | Shape CID | Notes |",
        "| --- | --- | --- |",
        f"| `concept:fully-qualified-path` | `{row['cid']}` | carries the exact path string as the substrate term payload |",
    ]
    readme.write_text(
        replace_readme_section(text, "Fully Qualified Path Concept Shape", section),
        encoding="utf-8",
    )


def mint_all() -> dict:
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    spec = build_shape_spec()
    discharge.write_json(SPEC_DIR / SPEC_FILENAME, spec)
    cid, path = discharge.mint("algorithm", SPEC_FILENAME)
    row = {"kind": "shape", "name": spec["fn_name"], "cid": cid, "path": path}
    append_cid_row(row)
    update_readme(row)
    discharge.scan_created_text()
    print(f"fully_qualified_path_shape_cid\t{row['name']}\t{row['cid']}")
    return row


if __name__ == "__main__":
    mint_all()
