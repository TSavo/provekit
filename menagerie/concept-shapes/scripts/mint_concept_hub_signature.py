#!/usr/bin/env python3
"""Mint the substrate-concept-hub LanguageSignatureMemento.

Per `docs/plans/2026-05-19-concept-hub-language-signature-ruling.md` (#1290):
the substrate's concept hub is a LanguageSignatureMemento in the LSP sense
(see `protocol/specs/2026-05-09-language-signature-protocol.md` §1.3). Its
CID anchors `target_language_signature_cid` for cross-language morphism
mementos (e.g., sort-classification answers per #1284) whose target side
is a substrate-canonical sort, op, or effect.

The signature enumerates:

- All substrate-canonical sort mementos in
  `menagerie/concept-shapes/catalog/sorts/`.
- All substrate-canonical concept-op mementos in
  `menagerie/concept-shapes/catalog/algorithms/` matching the `concept:*`
  prefix (excluding morphism:*, sort-morphism:*, language-specific shape
  mementos like `<name>:rust:to-shape`).

Equations and effect_signatures are empty in v1 (mint when substrate-
canonical equations/effects emerge).

The signature is written to
`menagerie/concept-hub-language-signature/specs/language_signature_concept_hub.spec.json`
following the AMP envelope convention (sibling shape to
`catalog/algorithms/concept:literal.<cid>.json`):

```
{
  "memento": { ... },
  "cid": "blake3-512:...",
  "signature": { "alg": "ed25519", "key_id": "UNSIGNED_DEV_ONLY", "sig_b64": "..." }
}
```

The `cid` is computed via JCS+blake3-512 over the `memento` value's
canonical JSON. The signature is a development placeholder; production
publishing replaces with a real ed25519 signature.

Re-mint cadence: re-run this script whenever substrate-canonical sorts or
concept-ops mint (new entries in `catalog/sorts/` or
`catalog/algorithms/concept:*.json`). The signature CID will change to
reflect the new content; downstream mementos pinning the old CID remain
valid for their version of the hub.

Usage:
    python3 mint_concept_hub_signature.py
"""

from __future__ import annotations

import base64
import hashlib
import json
import re
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parents[1]
CATALOG_SORTS_DIR = BASE / "catalog" / "sorts"
CATALOG_ALGORITHMS_DIR = BASE / "catalog" / "algorithms"
HUB_SIG_DIR = ROOT / "menagerie" / "concept-hub-language-signature" / "specs"
HUB_SIG_FILE = HUB_SIG_DIR / "language_signature_concept_hub.spec.json"
CIDS_TSV = BASE / "cids.tsv"

CID_FILENAME_RE = re.compile(r"^(?P<name>.+)\.(?P<cid>blake3-512:[0-9a-f]+)\.json$")
CONCEPT_OP_PREFIX = "concept:"
HUB_SIG_NAME = "language_signature_concept_hub"
HUB_FN_NAME = "concept-hub:v1"


def collect_sort_cids() -> list[tuple[str, str]]:
    """Return sorted list of (sort_name, cid) for catalog/sorts/ entries."""
    out: list[tuple[str, str]] = []
    for path in sorted(CATALOG_SORTS_DIR.glob("*.json")):
        match = CID_FILENAME_RE.match(path.name)
        if not match:
            raise ValueError(f"catalog/sorts/{path.name} does not match CID-filename convention")
        out.append((match.group("name"), match.group("cid")))
    return out


def collect_concept_op_cids() -> list[tuple[str, str]]:
    """Return sorted list of (op_name, cid) for catalog/algorithms/concept:*.json entries.

    Excludes morphism:*, sort-morphism:*, and language-specific shape mementos
    (e.g., `<name>:rust:to-shape`). Only substrate-canonical concept-ops.
    """
    out: list[tuple[str, str]] = []
    for path in sorted(CATALOG_ALGORITHMS_DIR.glob("concept:*.json")):
        match = CID_FILENAME_RE.match(path.name)
        if not match:
            raise ValueError(
                f"catalog/algorithms/{path.name} does not match CID-filename convention"
            )
        name = match.group("name")
        if not name.startswith(CONCEPT_OP_PREFIX):
            continue
        # Sanity: skip cross-language morphism entries (defensive; sorted glob
        # already filters by 'concept:' prefix, but the catalog has historical
        # entries like 'morphism:python:add:to:concept:add' that match the
        # prefix only via their suffix).
        if ":to:" in name or ":to-shape" in name:
            continue
        out.append((name, match.group("cid")))
    return out


def jcs_canonical(value: object) -> str:
    """JCS-canonical JSON serialization per RFC 8785.

    The substrate uses BLAKE3-512 over JCS-canonical bytes. JCS sorts object
    keys alphabetically and removes insignificant whitespace.
    """
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def blake3_512_of_bytes(data: bytes) -> str:
    """blake3-512 hex of bytes, prefixed `blake3-512:`."""
    try:
        from blake3 import blake3 as _blake3
    except ImportError as exc:
        raise SystemExit(
            "mint_concept_hub_signature.py: missing 'blake3' Python package.\n"
            "  Install with: pip install blake3"
        ) from exc
    digest = _blake3(data).digest(length=64)
    return f"blake3-512:{digest.hex()}"


def build_memento(sorts: list[tuple[str, str]], ops: list[tuple[str, str]]) -> dict:
    """Build the LanguageSignature memento content (pre-CID)."""
    return {
        "auto_minted_mementos": [],
        "effects": {"effects": []},
        "fn_name": HUB_FN_NAME,
        "formal_sorts": [],
        "formals": [],
        "kind": "LanguageSignatureMemento",
        "post": {
            "effect_signatures": [],
            "equations": [],
            "kind": "language-signature-bundle",
            "operations": [cid for _, cid in ops],
            "sorts": [cid for _, cid in sorts],
        },
        "pre": {"args": [], "kind": "atomic", "name": "true"},
        "protocol": "AMP",
        "return_sort": {"args": [], "kind": "ctor", "name": "LanguageSignature"},
        "schema_version": "1",
    }


def build_envelope(memento: dict, cid: str) -> dict:
    """Wrap memento in AMP envelope with cid + placeholder signature."""
    return {
        "cid": cid,
        "memento": memento,
        "signature": {
            "alg": "ed25519",
            "key_id": "UNSIGNED_DEV_ONLY",
            # Stable placeholder sig_b64 (64 bytes of zeros). Production
            # publishing replaces with a real ed25519 signature.
            "sig_b64": base64.b64encode(bytes(64)).decode("ascii"),
        },
    }


def write_spec_file(envelope: dict) -> None:
    """Write the envelope JSON to the signature spec file (deterministic)."""
    HUB_SIG_DIR.mkdir(parents=True, exist_ok=True)
    # Pretty-print with 2-space indent for human readability. JCS is only
    # used for CID computation; the file on disk can be pretty-printed.
    content = json.dumps(envelope, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    HUB_SIG_FILE.write_text(content, encoding="utf-8")


def update_cids_tsv(cid: str) -> None:
    """Append/update the concept-hub-signature CID in cids.tsv.

    Idempotent: if a row already exists with name `language_signature_concept_hub`,
    update its CID; otherwise append.
    """
    lines = CIDS_TSV.read_text(encoding="utf-8").splitlines(keepends=True)
    relpath = (
        HUB_SIG_FILE.relative_to(ROOT).as_posix()
    )
    new_line = f"language_signature\t{HUB_SIG_NAME}\t{cid}\t{relpath}\n"
    updated = False
    new_lines: list[str] = []
    for line in lines:
        parts = line.split("\t")
        if len(parts) >= 2 and parts[1] == HUB_SIG_NAME:
            new_lines.append(new_line)
            updated = True
        else:
            new_lines.append(line)
    if not updated:
        if new_lines and not new_lines[-1].endswith("\n"):
            new_lines[-1] = new_lines[-1] + "\n"
        new_lines.append(new_line)
    CIDS_TSV.write_text("".join(new_lines), encoding="utf-8")


def main() -> None:
    sorts = collect_sort_cids()
    ops = collect_concept_op_cids()
    if not sorts:
        raise SystemExit("mint_concept_hub_signature: catalog/sorts/ is empty")
    if not ops:
        raise SystemExit(
            "mint_concept_hub_signature: catalog/algorithms/concept:*.json is empty"
        )
    memento = build_memento(sorts, ops)
    cid = blake3_512_of_bytes(jcs_canonical(memento).encode("utf-8"))
    envelope = build_envelope(memento, cid)
    write_spec_file(envelope)
    update_cids_tsv(cid)
    print(f"Minted concept-hub signature: {cid}")
    print(f"  sorts: {len(sorts)} ({', '.join(name for name, _ in sorts[:5])}{'...' if len(sorts) > 5 else ''})")
    print(f"  operations: {len(ops)} ({', '.join(name for name, _ in ops[:5])}{'...' if len(ops) > 5 else ''})")
    print(f"  spec file: {HUB_SIG_FILE.relative_to(ROOT)}")
    print(f"  cids.tsv entry: language_signature\\t{HUB_SIG_NAME}\\t{cid}")


if __name__ == "__main__":
    main()
