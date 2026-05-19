from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-sqlite3/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_sqlite3.platform_semantics import (
    CONCEPT_INSERT_AND_GET_ID_CID,
    declaration,
    dimension_values,
)
from provekit_realize_python_sqlite3.rpc import dispatch

# Golden CIDs verified by independent computation (kit_cid elided per substrate spec).
GOLDEN_CURSOR_LASTROWID_CID = (
    "blake3-512:"
    "6fbe68f4eb8a7cf5e58bd5859f43ce9bff042e3b68f85d6576fd3055d08f9d2a"
    "36bf9c316f6580111e178d82495b820b66414c934e5723d0e1c3a337df269933"
)
GOLDEN_INSERT_TAG_CID = (
    "blake3-512:"
    "2e2c882ac4c9b8a109b16a5645da75493eadaaa3f242a6169b36c7f3bb799b8a"
    "12cf4315ed0c0fe2f9da742da555e642499a532104c946ea923971ced3d1b6c8"
)


def test_sqlite3_declaration_is_non_empty() -> None:
    # Positive: declaration has at least one tag and one dimension value.
    decl = declaration()
    assert decl["tags"], "must declare at least one op tag"
    assert decl["dimension_values"], "must declare dimension values"
    assert any(t["op_cid"] == CONCEPT_INSERT_AND_GET_ID_CID for t in decl["tags"]), \
        "must declare concept:insert-and-get-id"


def test_sqlite3_cursor_lastrowid_cid_matches_golden() -> None:
    # Positive: CursorLastRowid dimension value CID matches golden.
    dvs = dimension_values()
    row_id = next(d for d in dvs if d["dimension_name"] == "RowIdMechanism")
    assert row_id["value_name"] == "CursorLastRowid"
    assert row_id["cid"] == GOLDEN_CURSOR_LASTROWID_CID


def test_sqlite3_insert_tag_cid_matches_golden() -> None:
    # Positive: insert-and-get-id tag CID matches golden.
    decl = declaration()
    tag = next(t for t in decl["tags"] if t["op_cid"] == CONCEPT_INSERT_AND_GET_ID_CID)
    assert tag["cid"] == GOLDEN_INSERT_TAG_CID


def test_sqlite3_cursor_lastrowid_differs_from_last_insert_rowid() -> None:
    # Discrimination: CursorLastRowid must differ from LastInsertRowid (better-sqlite3).
    dvs = dimension_values()
    row_id = next(d for d in dvs if d["dimension_name"] == "RowIdMechanism")
    # LastInsertRowid golden CID (better-sqlite3)
    last_insert_rowid_cid = (
        "blake3-512:"
        "619f9cb06fa946350f9c8050f0be5281c6e7f67730be491bbe1223e549263ef6"
        "cb63751c1c3ea4f2df23a25a9d7307fcbb9634e58a13983e0900d40240fc2cf6"
    )
    assert row_id["cid"] != last_insert_rowid_cid, \
        "CursorLastRowid and LastInsertRowid must hash to different CIDs"


def test_sqlite3_dimension_value_compare_to_structure() -> None:
    # Structural: compare_to is IrFormula::Atomic with IrTerm::Ctor args.
    dvs = dimension_values()
    row_id = next(d for d in dvs if d["dimension_name"] == "RowIdMechanism")
    ct = row_id["compare_to"]
    assert ct["kind"] == "atomic"
    assert ct["name"] == "row_id_source"
    assert len(ct["args"]) == 1
    arg = ct["args"][0]
    assert arg["kind"] == "ctor"
    assert arg["name"] == "cursor_lastrowid"


def test_sqlite3_rpc_dispatch_platform_semantics() -> None:
    # Positive: RPC dispatch returns correct shape for platform_semantics.
    response = dispatch({"jsonrpc": "2.0", "id": 3, "method": "provekit.plugin.platform_semantics"})
    assert response["jsonrpc"] == "2.0"
    assert response["id"] == 3
    assert isinstance(response["result"]["tags"], list)
    assert isinstance(response["result"]["dimension_values"], list)
    assert response["result"]["op_aliases"] == {}
    assert len(response["result"]["tags"]) > 0
    assert len(response["result"]["dimension_values"]) > 0
