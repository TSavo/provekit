from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-sqlite3/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

import json

from provekit_realize_python_sqlite3.realizer import (
    MissingTemplateError,
    body_template_for,
    current_body_templates,
    emit_stub,
)


def test_sql_query_uses_sqlite3_execute() -> None:
    result = emit_stub(
        function="select_rows",
        params=["sql", "args"],
        param_types=["str", "list[object]"],
        return_type="list[object]",
        concept_name="concept:sql-query",
    )

    assert result["source"] == (
        "def select_rows(sql, args):\n"
        "    cursor = db.execute(sql, tuple(args))\n"
        "    return cursor.fetchall()\n"
    )
    assert result["is_stub"] is False
    assert result["extension"] == "py"


# ---------------------------------------------------------------------------
# `.proof`-load-via-RPC (PIECE 1, fan-out off canonical-bodies-sqlite3.json).
# When cmd_materialize lifts the sqlite3 shim's signed binding entries and
# feeds them over RPC, the kit PREFERS them over its on-disk cache. Mirrors the
# core kit + SugarRealizer.entries() (Java PR #1458).
# ---------------------------------------------------------------------------

_RPC_SQL_QUERY_OVERRIDE = json.dumps(
    [
        {
            "concept_name": "concept:sql-query",
            "emission_template": {
                "kind": "verbatim",
                "template": "return db.execute(${param0}, ${param1}).fetchall()  # rpc",
            },
            "signature_guard": {"min_params": 2, "max_params": 2},
            "target_library_tag": "sqlite3",
        }
    ]
)


def test_rpc_body_template_prefers_proof_over_disk() -> None:
    disk_body = body_template_for(
        "concept:sql-query", ["sql", "args"], ["str", "list[object]"], "list[object]"
    )
    assert disk_body == "cursor = db.execute(sql, tuple(args))\nreturn cursor.fetchall()"
    token = current_body_templates.set(_RPC_SQL_QUERY_OVERRIDE)
    try:
        rpc_body = body_template_for(
            "concept:sql-query", ["sql", "args"], ["str", "list[object]"], "list[object]"
        )
    finally:
        current_body_templates.reset(token)
    assert rpc_body == "return db.execute(sql, args).fetchall()  # rpc"
    # No leakage after reset.
    assert (
        body_template_for(
            "concept:sql-query", ["sql", "args"], ["str", "list[object]"], "list[object]"
        )
        == "cursor = db.execute(sql, tuple(args))\nreturn cursor.fetchall()"
    )


def test_rpc_body_template_empty_falls_through_to_disk() -> None:
    assert current_body_templates.get() == ""
    body = body_template_for(
        "concept:sql-query", ["sql", "args"], ["str", "list[object]"], "list[object]"
    )
    assert body == "cursor = db.execute(sql, tuple(args))\nreturn cursor.fetchall()"


def test_rpc_body_template_unrelated_concept_does_not_shadow_disk() -> None:
    # RPC override for sql-query must not shadow disk entries for other concepts.
    token = current_body_templates.set(_RPC_SQL_QUERY_OVERRIDE)
    try:
        close_body = body_template_for(
            "concept:sql-connection-close", ["conn"], ["object"], "None"
        )
    finally:
        current_body_templates.reset(token)
    assert close_body is not None
    assert "close" in close_body


def test_unknown_concept_refuses_missing_body_template() -> None:
    try:
        emit_stub("missing", ["x"], ["int"], "int", "missing-concept")
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "missing-concept",
                "args_shape": ["int"],
                "function": "missing",
                "term_position": "body",
            }
        ]
    else:
        raise AssertionError("missing body-template should refuse")
