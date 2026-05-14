from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-sqlite3/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_sqlite3.realizer import emit_stub


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
