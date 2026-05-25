from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-sqlite3/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))
SHIM_SRC = ROOT / "examples/provekit-shim-python-sqlite3"
if str(SHIM_SRC) not in sys.path:
    sys.path.insert(0, str(SHIM_SRC))

from provekit_realize_python_sqlite3.realizer import (
    MissingTemplateError,
    body_template_for,
    emit_stub,
)

# The kit resolves its shim proof from the installed/importable shim package.


def test_sql_query_all_uses_sqlite3_execute(disk_fixture) -> None:
    result = emit_stub(
        function="select_rows",
        params=["sql", "args"],
        param_types=["str", "list[object]"],
        return_type="list[object]",
        concept_name="concept:sql-query-all",
    )

    assert result["source"] == (
        "def select_rows(sql, args):\n"
        "    cursor = db.execute(sql, tuple(args))\n"
        "    return cursor.fetchall()\n"
    )
    assert result["is_stub"] is False
    assert result["extension"] == "py"


def test_sql_query_all_self_resolves_from_shim_proof(disk_fixture) -> None:
    body = body_template_for(
        "concept:sql-query-all", ["sql", "args"], ["str", "list[object]"], "list[object]"
    )
    assert body == "cursor = db.execute(sql, tuple(args))\nreturn cursor.fetchall()"


def test_unrelated_concept_resolves_from_same_shim_proof(disk_fixture) -> None:
    close_body = body_template_for("concept:sql-connection-close", ["conn"], ["object"], "None")
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
