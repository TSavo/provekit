"""Shared test fixtures for the python-aiosqlite realize kit.

As of the python-aiosqlite fan-out (#1468), `python-canonical-bodies-aiosqlite.json`
is DELETED from the repo: the @ProveKitSugar shim `.proof` (carried over RPC via
the kit-owned shim proof resolver is the body authority, and the migrate path derives
its provenance `binding_cid` from that `.proof` too. So the disk-fallback
invariant can no longer be exercised against a shipped repo file. `disk_fixture`
points `PROVEKIT_REPO_ROOT` at a temp tree carrying a MINIMAL canonical-bodies
JSON (only the concepts the tests probe) and clears `_disk_entries`' lru_cache,
so the "empty RPC -> disk fallback" and "RPC prefers over disk" invariants stay
covered WITHOUT depending on the deleted file. Mirrors the python-sqlite3 kit
conftest (#1463), with the async (`async with ...`) body shape aiosqlite ships.
"""

from __future__ import annotations

import json
import sys
from collections.abc import Iterator
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-aiosqlite/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_aiosqlite import realizer  # noqa: E402
from provekit_realize_python_aiosqlite.realizer import BODY_TEMPLATE_REL  # noqa: E402

# Minimal disk body-templates content. Only the concepts the tests probe, in
# the 2-param `db.execute(sql, tuple(args))` shape the shim ships. A 1-param
# lookup MISSES this (no matching guard), which is exactly
# the "bare signature -> missing template" invariant the rpc tests assert.
_DISK_FIXTURE = {
    "header": {
        "content": {
            "entries": [
                {
                    "concept_name": "concept:sql-query-all",
                    "emission_template": {
                        "kind": "verbatim",
                        "template": (
                            "async with db.execute(${param0}, tuple(${param1})) as cursor:\n"
                            "    return await cursor.fetchall()"
                        ),
                    },
                    "signature_guard": {"min_params": 2, "max_params": 2},
                },
                {
                    "concept_name": "concept:sql-connection-close",
                    "emission_template": {
                        "kind": "verbatim",
                        "template": "await ${param0}.close()",
                    },
                    "signature_guard": {"min_params": 1, "max_params": 1},
                },
            ]
        }
    }
}


@pytest.fixture()
def disk_fixture(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> Iterator[Path]:
    """Stand up a temp repo root carrying a minimal canonical-bodies-aiosqlite.json
    so the disk-fallback path resolves deterministically, independent of the
    (now deleted) shipped fixture. Clears the realizer's disk-entry lru_cache on
    entry AND exit so neither this fixture nor other tests see stale entries."""
    target = tmp_path / BODY_TEMPLATE_REL
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(_DISK_FIXTURE), encoding="utf-8")
    monkeypatch.setenv("PROVEKIT_REPO_ROOT", str(tmp_path))
    realizer._disk_entries.cache_clear()
    try:
        yield tmp_path
    finally:
        realizer._disk_entries.cache_clear()
