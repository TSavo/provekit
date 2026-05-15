#!/usr/bin/env python3
"""Lift op specs into concept:op-definition citations."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Callable, Iterable

import discharge

BASE = Path(__file__).resolve().parents[1]
SPEC_DIR = BASE / "specs"
OUT_DIR = SPEC_DIR / "op-definitions"
CID_FILE = BASE / "cids.tsv"
OP_DEFINITION_NAME = "concept:op-definition"
CID_PREFIX = "blake3-512:"
CID_HEX_LEN = 128

Citation = dict[str, object]


def jcs_text(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def read_json(path: Path) -> object:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def write_jcs(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(jcs_text(value), encoding="utf-8")


def source_spec_paths(spec_dir: Path = SPEC_DIR) -> list[Path]:
    paths = set(spec_dir.glob("op_*.spec.json"))
    paths.update(spec_dir.glob("*_shape.spec.json"))
    return sorted(path for path in paths if path.is_file())


def resolve_op_definition_cid(cid_file: Path = CID_FILE) -> str:
    if not cid_file.exists():
        raise SystemExit(f"{cid_file} not found; run mint_op_definition.py first")

    for line in cid_file.read_text(encoding="utf-8").splitlines()[1:]:
        parts = line.split("\t")
        if len(parts) >= 3 and parts[0] == "shape" and parts[1] == OP_DEFINITION_NAME:
            validate_cid(parts[2])
            return parts[2]

    raise SystemExit(f"{OP_DEFINITION_NAME} CID not found in {cid_file}")


def validate_cid(cid: str) -> None:
    if not cid.startswith(CID_PREFIX):
        raise ValueError(f"CID must start with {CID_PREFIX!r}: {cid!r}")
    digest = cid[len(CID_PREFIX) :]
    if len(digest) != CID_HEX_LEN or any(ch not in "0123456789abcdef" for ch in digest):
        raise ValueError(f"CID digest must be {CID_HEX_LEN} lowercase hex chars: {cid!r}")


def sort_name(sort: object) -> str:
    if isinstance(sort, str):
        return sort
    if isinstance(sort, dict):
        name = sort.get("name")
        if isinstance(name, str) and name:
            return name
    raise ValueError(f"cannot extract bare sort name from {sort!r}")


def effect_name(effect: object) -> str:
    if isinstance(effect, str):
        return effect
    if isinstance(effect, dict):
        name = effect.get("name")
        if isinstance(name, str) and name:
            return name
        kind = effect.get("kind")
        if isinstance(kind, str) and kind:
            return kind
    raise ValueError(f"cannot extract effect name from {effect!r}")


def effect_names(spec: dict[str, object]) -> list[str]:
    effects = spec.get("effects", {})
    if effects is None:
        return []
    if isinstance(effects, dict):
        raw_effects = effects.get("effects", [])
    else:
        raw_effects = effects
    if not isinstance(raw_effects, list):
        raise ValueError(f"`effects` must be a list or an object with an effects list: {raw_effects!r}")
    return [effect_name(effect) for effect in raw_effects]


def wp_rule(spec: dict[str, object]) -> str:
    post = spec.get("post", {})
    if not isinstance(post, dict):
        raise ValueError(f"`post` must be an object: {post!r}")
    note = post.get("wp_note", "")
    if not isinstance(note, str):
        raise ValueError(f"`post.wp_note` must be a string when present: {note!r}")
    return note


def require_list(spec: dict[str, object], key: str) -> list[object]:
    value = spec.get(key)
    if not isinstance(value, list):
        raise ValueError(f"`{key}` must be a list: {value!r}")
    return value


def require_string(spec: dict[str, object], key: str) -> str:
    value = spec.get(key)
    if not isinstance(value, str) or not value:
        raise ValueError(f"`{key}` must be a non-empty string: {value!r}")
    return value


def build_citation(spec: dict[str, object], op_definition_cid: str) -> Citation:
    validate_cid(op_definition_cid)
    name = require_string(spec, "fn_name")
    formals = require_list(spec, "formals")
    formal_sorts = require_list(spec, "formal_sorts")
    if len(formals) != len(formal_sorts):
        raise ValueError(f"{name}: formals/formal_sorts length mismatch")

    citation: Citation = {
        "args": {
            "arg_sorts": [sort_name(sort) for sort in formal_sorts],
            "effects": effect_names(spec),
            "name": name,
            "return_sort": sort_name(spec.get("return_sort")),
            "wp_rule": wp_rule(spec),
        },
        "kind": "op-application",
        "op_definition_cid": op_definition_cid,
    }
    validate_citation(citation, op_definition_cid)
    return citation


def validate_citation(citation: Citation, op_definition_cid: str) -> None:
    if set(citation) != {"args", "kind", "op_definition_cid"}:
        raise ValueError(f"unexpected citation keys: {sorted(citation)}")
    if citation["kind"] != "op-application":
        raise ValueError("citation kind must be op-application")
    if citation["op_definition_cid"] != op_definition_cid:
        raise ValueError("citation op_definition_cid does not match minted concept:op-definition CID")

    args = citation["args"]
    if not isinstance(args, dict):
        raise ValueError("citation args must be an object")
    if set(args) != {"name", "arg_sorts", "return_sort", "effects", "wp_rule"}:
        raise ValueError(f"unexpected args keys: {sorted(args)}")
    if not isinstance(args["name"], str) or not args["name"]:
        raise ValueError("args.name must be a non-empty string")
    if not isinstance(args["return_sort"], str) or not args["return_sort"]:
        raise ValueError("args.return_sort must be a non-empty string")
    if not isinstance(args["wp_rule"], str):
        raise ValueError("args.wp_rule must be a string")
    for key in ("arg_sorts", "effects"):
        values = args[key]
        if not isinstance(values, list) or not all(isinstance(item, str) and item for item in values):
            raise ValueError(f"args.{key} must be a list of non-empty strings")


def artifact_path(out_dir: Path, op_name: str) -> Path:
    filename = op_name.replace("/", "_")
    return out_dir / f"{filename}.op-def.ccl.json"


def canonical_cid(value: Citation) -> str:
    if not discharge.CANON.exists():
        discharge.build_tools()
    return discharge.canonical_cid_value(value)


def remove_stale_artifacts(out_dir: Path, expected_paths: Iterable[Path]) -> None:
    expected = {path.resolve() for path in expected_paths}
    if not out_dir.exists():
        return
    for path in out_dir.glob("*.op-def.ccl.json"):
        if path.resolve() not in expected:
            path.unlink()


def lift_catalog(
    spec_dir: Path = SPEC_DIR,
    out_dir: Path = OUT_DIR,
    op_definition_cid: str | None = None,
    cid_for_value: Callable[[Citation], str] = canonical_cid,
) -> list[dict[str, str]]:
    op_definition_cid = op_definition_cid or resolve_op_definition_cid()
    rows: list[dict[str, str]] = []
    expected_paths: list[Path] = []
    index: dict[str, str] = {}

    for spec_path in source_spec_paths(spec_dir):
        spec = read_json(spec_path)
        if not isinstance(spec, dict):
            raise ValueError(f"{spec_path}: spec must be a JSON object")
        citation = build_citation(spec, op_definition_cid)
        name = citation["args"]["name"]  # type: ignore[index]
        if not isinstance(name, str):
            raise ValueError(f"{spec_path}: args.name must be a string")
        path = artifact_path(out_dir, name)
        cid = cid_for_value(citation)
        validate_cid(cid)
        write_jcs(path, citation)
        index[name] = cid
        expected_paths.append(path)
        rows.append({"cid": cid, "name": name, "path": discharge.rel_path(path)})

    remove_stale_artifacts(out_dir, expected_paths)
    write_jcs(out_dir / "index.cids.json", index)
    return rows


def main() -> int:
    rows = lift_catalog()
    print(f"op_definition_lift_count\t{len(rows)}")
    print(f"op_definition_index\t{discharge.rel_path(OUT_DIR / 'index.cids.json')}")
    for row in rows:
        print(f"op_definition_cid\t{row['name']}\t{row['cid']}")
    discharge.scan_created_text()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
