#!/usr/bin/env python3
"""Regenerate menagerie/concept-shapes/catalog/index.json from the filesystem.

#1435: the embedded index lagged the catalog on disk (18 abstraction hubs
indexed vs 24 present, 340 entries stale overall). Consumers that trust the
embedded index (PR-0's singleton validator, the rust/python/java/c bind
lifters) saw a stale view while the filesystem-walking catalog verb (PR-10)
was correct.

This script walks the catalog kind-directories and rebuilds their entries
from the content-addressed filenames, while PRESERVING any pre-existing
index entries that do NOT live under a known catalog kind-directory (the
exam-manifest entry whose `path` points outside `catalog/` and is resolved
by kit_dispatch's exam-manifest lookup). It does not invent a `kind`: the
kind is derived from the directory name via KIND_BY_DIR.

`kit-source-aliases/` is intentionally NOT indexed (it has never appeared in
index.json); expanding the index's kind-set is out of scope for #1435.

Entry shape mirrors the existing index:
    { "cid", "kind", "name", "path" }
Output is sorted by CID for stable, churn-free diffs; `schema_version` is
preserved from the existing index.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

CATALOG = Path(__file__).resolve().parent.parent / "catalog"
INDEX = CATALOG / "index.json"

# Directory name (under catalog/) -> singular `kind` label used in the index.
KIND_BY_DIR = {
    "abstractions": "abstraction",
    "algorithms": "algorithm",
    "sorts": "sort",
    "realizations": "realization",
    "receipts": "receipt",
}

CID_SEP = ".blake3-512:"


def split_name_cid(filename: str) -> tuple[str, str] | None:
    """Split `<name>.blake3-512:<hex>.json` into (name, cid).

    Names may themselves contain dots/colons (e.g. `concept:gt`), so split on
    the literal `.blake3-512:` separator rather than on `.`. Returns None for
    files that are not content-addressed (e.g. `*.receipt.json` attempt
    artifacts) — matching the pre-existing index, which only listed
    CID-named mementos.
    """
    stem = filename[:-len(".json")] if filename.endswith(".json") else filename
    idx = stem.find(CID_SEP)
    if idx < 0:
        return None
    name = stem[:idx]
    cid = "blake3-512:" + stem[idx + len(CID_SEP):]
    return name, cid


def main() -> int:
    existing = json.loads(INDEX.read_text())
    schema_version = existing.get("schema_version", "1")
    old_entries = existing.get("entries", {})

    entries: dict[str, dict] = {}

    # 1. Rebuild entries for every catalog kind-directory from disk.
    indexed_dirs = set(KIND_BY_DIR)
    for dirname, kind in KIND_BY_DIR.items():
        d = CATALOG / dirname
        if not d.is_dir():
            continue
        for f in sorted(d.glob("*.json")):
            split = split_name_cid(f.name)
            if split is None:
                continue
            name, cid = split
            entries[cid] = {
                "cid": cid,
                "kind": kind,
                "name": name,
                "path": f"{dirname}/{f.name}",
            }

    # 2. Preserve pre-existing entries whose path is NOT under a rebuilt
    #    kind-directory (e.g. the exam-manifest entry at exams/...). These are
    #    resolved by other code paths and must not be dropped.
    for cid, meta in old_entries.items():
        path = meta.get("path", "")
        top = path.split("/", 1)[0] if path else ""
        if top in indexed_dirs:
            continue
        entries.setdefault(cid, meta)

    ordered = {cid: entries[cid] for cid in sorted(entries)}
    out = {"entries": ordered, "schema_version": schema_version}

    INDEX.write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n")

    from collections import Counter
    kinds = Counter(v["kind"] for v in ordered.values())
    print(f"wrote {len(ordered)} entries to {INDEX}")
    for k in sorted(kinds):
        print(f"  {k}: {kinds[k]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
