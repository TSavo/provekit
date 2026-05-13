#!/usr/bin/env python3
"""Normalize cids.tsv paths to repo-relative form.

The legacy minter scripts call `str(path)` on an absolute `pathlib.Path`,
which captures the worktree prefix in the cids.tsv `path` column. Running
mint.sh from a different worktree then rewrites every row, churning the
file with worktree-local data that carries no information beyond what the
repo-relative path already provides.

This script reads cids.tsv, rewrites every absolute path to start at
`menagerie/concept-shapes/`, writes the result back. Idempotent: a second
run on a clean file is a no-op. Designed to run as the last step in
mint.sh so the committed cids.tsv is worktree-independent.

Closes task #73 (mint.sh cids.tsv absolute-path bug). The full fix would
edit `append_cids_tsv` in all 25+ minter scripts; this post-pass is the
boy-scout one-file fix that achieves the same observable outcome without
the 25-file blast radius.
"""

from __future__ import annotations

import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
CIDS_TSV = SCRIPT_DIR.parent / "cids.tsv"
ANCHOR = "menagerie/concept-shapes/"


def normalize_row(line: str) -> str:
    """Rewrite the path column of one cids.tsv row to repo-relative form.

    Header rows and short rows are passed through unchanged. The path
    column is the fourth tab-separated field; if it contains the
    `menagerie/concept-shapes/` anchor, everything before the anchor is
    stripped. If it does not contain the anchor, the row is returned
    unchanged (we do not invent paths we cannot verify).
    """
    parts = line.split("\t")
    if len(parts) < 4:
        return line
    path = parts[3]
    idx = path.find(ANCHOR)
    if idx <= 0:
        return line
    parts[3] = path[idx:]
    return "\t".join(parts)


def main() -> int:
    if not CIDS_TSV.exists():
        print(f"normalize_cids_paths: {CIDS_TSV} not found, nothing to do", file=sys.stderr)
        return 0
    original = CIDS_TSV.read_text(encoding="utf-8")
    lines = original.splitlines()
    rewritten = [normalize_row(line) for line in lines]
    output = "\n".join(rewritten) + ("\n" if original.endswith("\n") else "")
    if output == original:
        print("normalize_cids_paths: already normalized, no changes")
        return 0
    CIDS_TSV.write_text(output, encoding="utf-8")
    changed = sum(1 for a, b in zip(lines, rewritten) if a != b)
    print(f"normalize_cids_paths: rewrote {changed} row(s) to repo-relative paths")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
