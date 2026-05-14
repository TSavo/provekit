#!/usr/bin/env python3
"""Mint SQL query and execute concept shape entries."""

from __future__ import annotations

import discharge

SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

SQL_QUERY_LOSS_DIMS = [
    "cursor-lifetime",
    "row-order",
    "sync-vs-async",
    "transaction-isolation",
]

SQL_EXECUTE_LOSS_DIMS = [
    "last-insert-id",
    "sync-vs-async",
    "transaction-isolation",
]

SPEC_FILENAMES = {
    "sql-query": "op_sql_query.spec.json",
    "sql-execute": "op_sql_execute.spec.json",
}


def ctor(name: str) -> dict:
    return {"args": [], "kind": "ctor", "name": name}


def true_formula() -> dict:
    return {"args": [], "kind": "atomic", "name": "true"}


def operation_contract(operator: str, arity: list[str], result: str, slots: list[dict], wp_note: str) -> dict:
    return {
        "arity": arity,
        "arity_shape": {"kind": "named", "slots": slots},
        "kind": "operation-contract",
        "operator": operator,
        "result": result,
        "wp_note": wp_note,
    }


def shape_spec(
    fn_name: str,
    formals: list[str],
    formal_sorts: list[str],
    return_sort: str,
    post: dict,
    loss_dimensions: list[str],
) -> dict:
    return {
        "effects": {
            "effects": [
                {"kind": "effect-signature", "name": "Async"},
                {"kind": "effect-signature", "name": "IO"},
            ]
        },
        "fn_name": fn_name,
        "formal_sorts": [ctor(sort) for sort in formal_sorts],
        "formals": formals,
        "kind": "algorithm",
        "loss_dimensions": sorted(loss_dimensions),
        "post": post,
        "pre": true_formula(),
        "return_sort": ctor(return_sort),
    }


def build_shape_specs() -> dict[str, dict]:
    query_post = operation_contract(
        "sql-query",
        ["Sql", "SqlArgs"],
        "SqlRowSet",
        [{"name": "sql"}, {"name": "args"}],
        (
            "Executes a parameterized SQL read operation and returns a row set. "
            "Sql is the parameterized query string. SqlArgs carries positional or named parameters. "
            "SqlRowSet is the result iteration. Row order, cursor lifetime, transaction isolation, and sync versus async behavior are realization loss dimensions."
        ),
    )
    query_post["row_shape_sketch"] = {
        "row_sort": "SqlRow",
        "row_set_sort": "SqlRowSet",
    }

    execute_post = operation_contract(
        "sql-execute",
        ["Sql", "SqlArgs"],
        "SqlExecuteResult",
        [{"name": "sql"}, {"name": "args"}],
        (
            "Executes a parameterized SQL write operation and returns rows_affected plus an optional last_insert_id. "
            "The last_insert_id field is a loss dimension because database engines and client libraries expose it differently."
        ),
    )
    execute_post["result_shape_sketch"] = {
        "kind": "named",
        "slots": [
            {"name": "rows_affected", "sort": "Int"},
            {
                "loss_dimension": "last-insert-id",
                "name": "last_insert_id",
                "optional": True,
                "sort": "Optional<SqlId>",
            },
        ],
    }
    execute_post["last_insert_id_loss_claim"] = {
        "field": "last_insert_id",
        "kind": "library-dependent-field",
        "loss_dimension": "last-insert-id",
    }

    return {
        "sql-query": shape_spec(
            "concept:sql-query",
            ["sql", "args"],
            ["Sql", "SqlArgs"],
            "SqlRowSet",
            query_post,
            SQL_QUERY_LOSS_DIMS,
        ),
        "sql-execute": shape_spec(
            "concept:sql-execute",
            ["sql", "args"],
            ["Sql", "SqlArgs"],
            "SqlExecuteResult",
            execute_post,
            SQL_EXECUTE_LOSS_DIMS,
        ),
    }


def append_cids(rows: list[dict]) -> None:
    existing = CID_FILE.read_text(encoding="utf-8").splitlines() if CID_FILE.exists() else ["kind\tname\tcid\tpath"]
    seen: dict[tuple[str, str], str] = {}
    for line in existing[1:]:
        parts = line.split("\t")
        if len(parts) >= 3:
            seen[(parts[0], parts[1])] = parts[2]

    for row in rows:
        key = (row["kind"], row["name"])
        if key in seen:
            if seen[key] != row["cid"]:
                raise SystemExit(
                    f"one-name-one-CID violation: {row['kind']} {row['name']} "
                    f"already registered as {seen[key]!r} but new mint produced {row['cid']!r}"
                )
            continue
        existing.append(f"{row['kind']}\t{row['name']}\t{row['cid']}\t{row['path']}")
        seen[key] = row["cid"]
    CID_FILE.write_text("\n".join(existing) + "\n", encoding="utf-8")


def mint_all() -> list[dict]:
    discharge.build_tools()
    SPEC_DIR.mkdir(parents=True, exist_ok=True)
    rows = []
    for slug, spec in build_shape_specs().items():
        spec_name = SPEC_FILENAMES[slug]
        discharge.write_json(SPEC_DIR / spec_name, spec)
        cid, path = discharge.mint("algorithm", spec_name)
        rows.append({"kind": "shape", "name": spec["fn_name"], "cid": cid, "path": path})

    append_cids(rows)
    discharge.scan_created_text()
    for row in rows:
        print(f"sql_shape_cid\t{row['name']}\t{row['cid']}")
    return rows


if __name__ == "__main__":
    mint_all()
