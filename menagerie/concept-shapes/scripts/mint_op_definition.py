#!/usr/bin/env python3
"""Mint the concept:op-definition hub op."""

from __future__ import annotations

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

SPEC_FILENAME = "op-definition_shape.spec.json"


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


def build_shape_spec() -> dict:
    formals = ["name", "arg_sorts", "return_sort", "effects", "wp_rule"]
    post = operation_contract(
        "op-definition",
        ["String", "List<Cid>", "Cid", "List<EffectName>", "Formula"],
        "Cid",
        formals,
        (
            "Mints an op-definition citation. Well-formed iff every arg_sort and return_sort "
            "resolves to a minted sort CID, every effect name resolves to a minted EffectName, "
            "and wp_rule is parseable Formula prose at v1.0 (machine-checkable grammar pending "
            "task #61). The returned Cid identifies the minted op-definition memento."
        ),
    )

    return {
        "effects": {"effects": []},
        "fn_name": "concept:op-definition",
        "formal_sorts": [
            ctor("String"),
            ctor("List<Cid>"),
            ctor("Cid"),
            ctor("List<EffectName>"),
            ctor("Formula"),
        ],
        "formals": formals,
        "kind": "algorithm",
        "post": post,
        "pre": true_formula(),
        "return_sort": ctor("Cid"),
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


def update_readme(row: dict) -> None:
    readme = BASE / "README.md"
    text = readme.read_text(encoding="utf-8")
    start = "## Op Definition Concept Shape\n"
    if start in text:
        text = text[: text.index(start)].rstrip() + "\n"

    section = [
        "## Op Definition Concept Shape",
        "",
        "`concept:op-definition(name, arg_sorts, return_sort, effects, wp_rule)` describes a hub op definition; CCL's meta-layer primitive.",
        "",
        "| Concept | Shape CID | Notes |",
        "| --- | --- | --- |",
        f"| `concept:op-definition` | `{row['cid']}` | describes a hub op definition; CCL's meta-layer primitive |",
    ]
    readme.write_text(text.rstrip() + "\n\n" + "\n".join(section) + "\n", encoding="utf-8")


def mint_all() -> dict:
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    spec = build_shape_spec()
    discharge.write_json(SPEC_DIR / SPEC_FILENAME, spec)
    cid, path = discharge.mint("algorithm", SPEC_FILENAME)
    row = {"kind": "shape", "name": spec["fn_name"], "cid": cid, "path": path}
    append_cid_row(row)
    update_readme(row)
    discharge.scan_created_text()
    print(f"op_definition_shape_cid\t{row['name']}\t{row['cid']}")
    return row


if __name__ == "__main__":
    mint_all()
