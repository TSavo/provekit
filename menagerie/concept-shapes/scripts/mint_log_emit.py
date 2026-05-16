#!/usr/bin/env python3
"""Mint the concept:log-emit hub op."""

from __future__ import annotations

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

SPEC_FILENAME = "log-emit_shape.spec.json"

LOSS_DIMS = [
    "level-semantics",
    "mdc-context-propagation",
    "sink-buffered-vs-immediate",
    "structured-vs-formatted",
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


def build_shape_spec() -> dict:
    post = operation_contract(
        "log-emit",
        ["LogLevel", "String", "StructuredFields"],
        "Unit",
        ["level", "message", "structured_fields"],
        (
            "Emits a runtime log event at the requested level with message and "
            "structured fields. Per-language logger bindings may lose level "
            "precision, structured fields, buffering guarantees, or context propagation."
        ),
    )

    return {
        "effects": {"effects": [effect_signature("IO")]},
        "fn_name": "concept:log-emit",
        "formal_sorts": [ctor("LogLevel"), ctor("String"), ctor("StructuredFields")],
        "formals": ["level", "message", "structured_fields"],
        "kind": "algorithm",
        "log_levels": ["trace", "debug", "info", "warn", "error", "fatal"],
        "loss_dimensions": sorted(LOSS_DIMS),
        "post": post,
        "pre": true_formula(),
        "return_sort": ctor("Unit"),
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
    start = "## Log Emit Concept Shape\n"
    if start in text:
        text = text[: text.index(start)].rstrip() + "\n"

    section = [
        "## Log Emit Concept Shape",
        "",
        "`concept:log-emit(level, message, structured_fields)` is the hub op for logger-agnostic monitor and emitter body-template composition.",
        "",
        "| Concept | Shape CID | Notes |",
        "| --- | --- | --- |",
        f"| `concept:log-emit` | `{row['cid']}` | effect is IO; per-language logger sugars carry honest loss dimensions |",
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
    print(f"log_emit_shape_cid\t{row['name']}\t{row['cid']}")
    return row


if __name__ == "__main__":
    mint_all()
