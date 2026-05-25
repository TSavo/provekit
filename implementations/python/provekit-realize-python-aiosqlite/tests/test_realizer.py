from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-aiosqlite/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))
SHIM_SRC = ROOT / "examples/provekit-shim-python-aiosqlite"
if str(SHIM_SRC) not in sys.path:
    sys.path.insert(0, str(SHIM_SRC))

from provekit_realize_python_aiosqlite.realizer import (
    MissingTemplateError,
    body_template_for,
    emit_stub,
)

# The kit resolves its shim proof from the installed/importable shim package.


SQL_QUERY_NTT = {
    "conceptName": "concept:sql-query",
    "operationKind": "op-application",
    "shapeCid": "blake3-512:sql-query",
    "args": [
        {
            "conceptName": "Sql",
            "operationKind": "const",
            "shapeCid": "blake3-512:sql",
            "args": [],
        },
        {
            "conceptName": "SqlArgs",
            "operationKind": "tuple",
            "shapeCid": "blake3-512:sql-args",
            "args": [],
        },
    ],
}


def test_sql_query_uses_aiosqlite_execute(disk_fixture) -> None:
    result = emit_stub(
        function="select_rows",
        params=["sql", "args"],
        param_types=["str", "list[object]"],
        return_type="list[object]",
        concept_name="concept:sql-query",
    )

    assert result["source"] == (
        "async def select_rows(sql, args):\n"
        "    async with db.execute(sql, tuple(args)) as cursor:\n"
        "        return await cursor.fetchall()\n"
    )
    assert result["is_stub"] is False
    assert result["extension"] == "py"


def test_sql_query_uses_named_term_tree_args_shape(disk_fixture) -> None:
    result = emit_stub(
        function="get_user_by_id",
        params=["id"],
        param_types=["int"],
        return_type="User",
        concept_name="concept:sql-query",
        named_term_tree=SQL_QUERY_NTT,
    )

    assert result["source"] == (
        "async def get_user_by_id(id):\n"
        "    async with db.execute(sql, tuple(args)) as cursor:\n"
        "        return await cursor.fetchall()\n"
    )
    assert result["is_stub"] is False
    assert result["extension"] == "py"


def test_sql_query_self_resolves_from_shim_proof(disk_fixture) -> None:
    body = body_template_for(
        "concept:sql-query", ["sql", "args"], ["str", "list[object]"], "list[object]"
    )
    assert body == (
        "async with db.execute(sql, tuple(args)) as cursor:\n"
        "    return await cursor.fetchall()"
    )


def test_unrelated_concept_resolves_from_same_shim_proof(disk_fixture) -> None:
    close_body = body_template_for("concept:sql-connection-close", ["conn"], ["object"], "None")
    assert close_body is not None
    assert "close" in close_body


def test_sql_query_without_named_term_tree_uses_param_types_fallback(disk_fixture) -> None:
    try:
        emit_stub("get_user_by_id", ["id"], ["int"], "User", "concept:sql-query")
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "concept:sql-query",
                "args_shape": ["int"],
                "function": "get_user_by_id",
                "term_position": "body",
            }
        ]
    else:
        raise AssertionError("bare callsite signature should refuse")


def test_missing_template_reports_named_term_tree_args_shape() -> None:
    try:
        emit_stub(
            "missing",
            ["id"],
            ["int"],
            "User",
            "missing-concept",
            named_term_tree=SQL_QUERY_NTT,
        )
    except MissingTemplateError as exc:
        assert [entry.to_json() for entry in exc.entries] == [
            {
                "operation_kind": "missing-concept",
                "args_shape": ["str", "list[object]"],
                "function": "missing",
                "term_position": "body",
            }
        ]
    else:
        raise AssertionError("missing body-template should refuse")


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
