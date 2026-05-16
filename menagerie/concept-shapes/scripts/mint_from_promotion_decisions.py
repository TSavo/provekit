#!/usr/bin/env python3
"""Mint concept op specs from admitted PromotionDecisionMementos."""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import discharge

BASE = discharge.BASE
SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE


@dataclass(frozen=True)
class MintSummary:
    admitted_seen: int
    written: int
    skipped_existing: int


def load_jcs_round_tripped(path: Path) -> dict[str, Any]:
    """Parse JSON and force a sorted-key JSON round trip before use."""
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    round_tripped = json.loads(
        json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
    )
    if not isinstance(round_tripped, dict):
        raise SystemExit(f"{path}: expected JSON object")
    return round_tripped


def safe_filename(cid: str) -> str:
    return "".join(ch if ch.isalnum() or ch in "-_" else "_" for ch in cid)


def promoted_op_slug(promoted_op: str) -> str:
    name = promoted_op.removeprefix("concept:")
    slug = re.sub(r"[^A-Za-z0-9_]+", "_", name).strip("_")
    if not slug:
        raise SystemExit(f"cannot derive op-spec filename from promoted_op {promoted_op!r}")
    return slug


def spec_name_for_promoted_op(promoted_op: str) -> str:
    return f"op_{promoted_op_slug(promoted_op)}.spec.json"


def shape_name_candidates(promoted_op: str) -> set[str]:
    names = {promoted_op}
    if not promoted_op.startswith("concept:"):
        names.add(f"concept:{promoted_op}")
    return names


def existing_shape_names() -> set[str]:
    if not CID_FILE.exists():
        return set()
    names: set[str] = set()
    for line in CID_FILE.read_text(encoding="utf-8").splitlines()[1:]:
        parts = line.split("\t")
        if len(parts) >= 2 and parts[0] == "shape":
            names.add(parts[1])
    return names


def append_cids(rows: list[dict[str, str]]) -> None:
    existing = CID_FILE.read_text(encoding="utf-8").splitlines() if CID_FILE.exists() else ["kind\tname\tcid\tpath"]
    seen: dict[tuple[str, str], str] = {}
    for line in existing[1:]:
        parts = line.split("\t")
        if len(parts) >= 3:
            seen[(parts[0], parts[1])] = parts[2]

    for row in rows:
        key = (row["kind"], row["name"])
        if key in seen:
            if seen[key] != row["cid"]:
                raise SystemExit(
                    f"one-name-one-CID violation: {row['kind']} {row['name']} "
                    f"already registered as {seen[key]!r} but new mint produced {row['cid']!r}"
                )
            continue
        existing.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
        seen[key] = row["cid"]
    CID_FILE.write_text("\n".join(existing) + "\n", encoding="utf-8")


def decision_header(decision: dict[str, Any], path: Path) -> dict[str, Any]:
    header = decision.get("header")
    if not isinstance(header, dict):
        raise SystemExit(f"{path}: PromotionDecisionMemento is missing header object")
    if header.get("kind") != "promotion-decision":
        raise SystemExit(f"{path}: expected header.kind promotion-decision")
    if header.get("schemaVersion") != "1":
        raise SystemExit(f"{path}: expected header.schemaVersion 1")
    return header


def decision_payload(header: dict[str, Any], path: Path) -> dict[str, Any]:
    payload = header.get("decision_payload")
    if not isinstance(payload, dict):
        raise SystemExit(f"{path}: PromotionDecisionMemento is missing decision_payload object")
    return payload


def admitted(payload: dict[str, Any], header: dict[str, Any]) -> bool:
    return payload.get("result", header.get("result")) == "admitted"


def required_str(value: Any, field: str, path: Path) -> str:
    if not isinstance(value, str) or not value:
        raise SystemExit(f"{path}: expected non-empty string field {field}")
    return value


def first_evidence_cid(header: dict[str, Any], path: Path) -> str:
    evidence_cids = header.get("evidence_cids")
    if not isinstance(evidence_cids, list) or not evidence_cids:
        raise SystemExit(f"{path}: admitted decision must carry at least one evidence CID")
    first = evidence_cids[0]
    if not isinstance(first, str) or not first:
        raise SystemExit(f"{path}: first evidence CID must be a non-empty string")
    return first


def resolve_contract_path(bind_output_dir: Path, cid: str, decision_path: Path) -> Path:
    contracts_dir = bind_output_dir / "contracts"
    for stem in [cid, safe_filename(cid)]:
        candidate = contracts_dir / f"{stem}.json"
        if candidate.exists():
            return candidate
    raise SystemExit(f"{decision_path}: cannot resolve evidence CID {cid!r} under {contracts_dir}")


def contract_body(contract: dict[str, Any], path: Path) -> dict[str, Any]:
    body = contract.get("header") if isinstance(contract.get("header"), dict) else contract
    if not isinstance(body, dict):
        raise SystemExit(f"{path}: expected CompoundContractMemento object")
    if body.get("kind") != "compound-contract":
        raise SystemExit(f"{path}: expected kind compound-contract")
    return body


def required_field(body: dict[str, Any], field: str, path: Path) -> Any:
    if field not in body:
        raise SystemExit(f"{path}: compound contract missing required field {field}")
    return body[field]


def contract_pre(body: dict[str, Any], path: Path) -> Any:
    if "pre" in body:
        return body["pre"]
    if "composed_pre" in body:
        return body["composed_pre"]
    raise SystemExit(f"{path}: compound contract missing pre or composed_pre")


def contract_post(body: dict[str, Any], path: Path) -> Any:
    if "post" in body:
        return body["post"]
    if "composed_post" in body:
        return body["composed_post"]
    raise SystemExit(f"{path}: compound contract missing post or composed_post")


def op_spec_from_contract(promoted_op: str, reason: str, contract: dict[str, Any], contract_path: Path) -> dict[str, Any]:
    body = contract_body(contract, contract_path)
    return {
        "effects": required_field(body, "effects", contract_path),
        "fn_name": promoted_op,
        "formal_sorts": required_field(body, "formal_sorts", contract_path),
        "formals": required_field(body, "formals", contract_path),
        "kind": "algorithm",
        "locus": reason,
        "post": contract_post(body, contract_path),
        "pre": contract_pre(body, contract_path),
        "return_sort": required_field(body, "return_sort", contract_path),
    }


def already_exists(promoted_op: str, spec_name: str, known_shape_names: set[str]) -> bool:
    if (SPEC_DIR / spec_name).exists():
        return True
    return bool(shape_name_candidates(promoted_op) & known_shape_names)


def mint_from_bind_output(bind_output_dir: Path) -> MintSummary:
    decisions_dir = bind_output_dir / "promotion-decisions"
    known_shape_names = existing_shape_names()
    rows: list[dict[str, str]] = []
    admitted_seen = 0
    written = 0
    skipped_existing = 0

    for decision_path in sorted(decisions_dir.glob("*.json")):
        decision = load_jcs_round_tripped(decision_path)
        header = decision_header(decision, decision_path)
        payload = decision_payload(header, decision_path)
        if not admitted(payload, header):
            continue

        admitted_seen += 1
        promoted_op = required_str(payload.get("promoted_op"), "decision_payload.promoted_op", decision_path)
        spec_name = spec_name_for_promoted_op(promoted_op)
        if already_exists(promoted_op, spec_name, known_shape_names):
            skipped_existing += 1
            continue

        reason = required_str(payload.get("reason"), "decision_payload.reason", decision_path)
        evidence_cid = first_evidence_cid(header, decision_path)
        contract_path = resolve_contract_path(bind_output_dir, evidence_cid, decision_path)
        contract = load_jcs_round_tripped(contract_path)
        spec = op_spec_from_contract(promoted_op, reason, contract, contract_path)

        SPEC_DIR.mkdir(parents=True, exist_ok=True)
        discharge.write_json(SPEC_DIR / spec_name, spec)
        shape_cid, catalog_path = discharge.mint("algorithm", spec_name)
        rows.append({"kind": "shape", "name": promoted_op, "cid": shape_cid, "path": catalog_path})
        known_shape_names.add(promoted_op)
        written += 1

    append_cids(rows)
    if rows:
        discharge.scan_created_text()

    summary = MintSummary(admitted_seen=admitted_seen, written=written, skipped_existing=skipped_existing)
    print(
        "promotion_decision_mint_summary"
        f"\tadmitted_seen={summary.admitted_seen}"
        f"\twritten={summary.written}"
        f"\tskipped_existing={summary.skipped_existing}"
    )
    return summary


def main(argv: list[str] | None = None) -> MintSummary:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bind_output_dir", type=Path)
    args = parser.parse_args(argv)
    return mint_from_bind_output(args.bind_output_dir)


if __name__ == "__main__":
    main()
