#!/usr/bin/env python3
"""Mint the concept:contract-observation hub op."""

from __future__ import annotations

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

SPEC_FILENAME = "contract-observation_shape.spec.json"

LOSS_DIMS = [
    "composition-point",
    "observer-runtime-availability",
    "signature-material",
    "surface-enforcement",
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


def wrapper_effect(name: str) -> dict:
    return {"kind": "effect-signature", "name": name}


def observation_mode(mode: str, composition_points: list[str], wrapper_effects: list[str]) -> dict:
    return {
        "composition_points": composition_points,
        "mode": mode,
        "wrapper_effects": [wrapper_effect(name) for name in wrapper_effects],
    }


def build_shape_spec() -> dict:
    post = operation_contract(
        "contract-observation",
        ["Cid", "Cid", "ContractObservationMode"],
        "ContractObservationResult",
        ["callsite_cid", "contract_cid", "mode"],
        (
            "Observes the contract at a callsite in the selected mode. Observer effects "
            "belong to the generated ObservationWrapperMemento and wrapper FCM, not to "
            "the wrapped object FunctionContractMemento. Composition point is explicit "
            "per realization body-template cell."
        ),
    )

    return {
        "effects": {"effects": []},
        "fn_name": "concept:contract-observation",
        "formal_sorts": [ctor("Cid"), ctor("Cid"), ctor("ContractObservationMode")],
        "formals": ["callsite_cid", "contract_cid", "mode"],
        "kind": "algorithm",
        "loss_dimensions": sorted(LOSS_DIMS),
        "observation_modes": [
            observation_mode("Emitter", ["before", "after-return", "after-throw"], ["IO"]),
            observation_mode("Gate", ["before", "after-return", "after-throw", "around"], ["Throw"]),
            observation_mode("Monitor", ["before", "after-return", "after-throw"], ["Reads"]),
            observation_mode("Witness", ["after-return"], ["IO", "Sign"]),
        ],
        "post": post,
        "pre": true_formula(),
        "return_sort": ctor("ContractObservationResult"),
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
    start = "## Contract Observation Concept Shape\n"
    if start in text:
        text = text[: text.index(start)].rstrip() + "\n"

    section = [
        "## Contract Observation Concept Shape",
        "",
        "`concept:contract-observation(callsite_cid, contract_cid, mode)` is the hub op for witness, monitor, emitter, and gate observation wrappers.",
        "",
        "| Concept | Shape CID | Notes |",
        "| --- | --- | --- |",
        f"| `concept:contract-observation` | `{row['cid']}` | mode is a formal slot; observer effects live on the wrapper memento, not the object FCM |",
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
    print(f"contract_observation_shape_cid\t{row['name']}\t{row['cid']}")
    return row


if __name__ == "__main__":
    mint_all()
