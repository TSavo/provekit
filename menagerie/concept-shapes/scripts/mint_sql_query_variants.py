#!/usr/bin/env python3
"""Mint the three result-cardinality variants of concept:sql-query.

Phase 0 of the concept-catalog refinement tracked in issue #1468.

concept:sql-query is split by result cardinality into three distinct
concepts, because a different post-condition is a different contract (it is
NOT realization loss). The original concept:sql-query op-def mis-filed
cardinality / cursor-lifetime as loss dimensions; here they are POST-CONTRACT:

  - concept:sql-query-row      one row or null  (exists at most one)
  - concept:sql-query-all      fully-materialized array of all matching rows
  - concept:sql-query-iterate  lazy single-pass cursor over rows

Genuine surviving loss (same post, bounded fidelity): implementation-defined
row order in the absence of an ORDER BY clause. Sync-versus-async stays an
effect annotation (Async / IO effects) plus a sync-vs-async loss dimension,
exactly as in the original concept:sql-query. Transaction isolation likewise
remains a loss dimension.

This minter mirrors mint_sql_concepts.py and delegates CID computation and
the unsigned dev signature to the provekit CLI via discharge.mint, so the
catalog algorithm CIDs are produced the one canonical way. It does NOT
delete the original concept:sql-query artifacts (later phases migrate
consumers off it first) and it does NOT touch the op-definition citations
or index.cids.json (those are hand-authored to mirror the committed
sentinel convention).
"""

from __future__ import annotations

import discharge

SPEC_DIR = discharge.SPEC_DIR
CID_FILE = discharge.CID_FILE

# Same post, bounded-fidelity loss that survives the cardinality split.
# Row order is implementation-defined absent ORDER BY; transaction isolation
# and sync-vs-async remain realization choices. Cardinality, cursor lifetime
# and materialization are NOT here: they are post-contract distinctions that
# separate the three concepts.
SQL_QUERY_VARIANT_LOSS_DIMS = [
    "row-order",
    "sync-vs-async",
    "transaction-isolation",
]

SPEC_FILENAMES = {
    "sql-query-row": "op_sql_query_row.spec.json",
    "sql-query-all": "op_sql_query_all.spec.json",
    "sql-query-iterate": "op_sql_query_iterate.spec.json",
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
    slots = [{"name": "sql"}, {"name": "args"}]

    # concept:sql-query-row : at most one row.
    row_post = operation_contract(
        "sql-query-row",
        ["Sql", "SqlArgs"],
        "Optional<SqlRow>",
        slots,
        (
            "Executes a parameterized SQL read operation and returns at most one row. "
            "Sql is the parameterized query string. SqlArgs carries positional or named parameters. "
            "Optional<SqlRow> is present (one row) when the query matches at least one row and absent (null) otherwise; "
            "the result cardinality is at most one (a single row or nothing), not a row set. "
            "Returning at most one row is the post-condition of this concept, not a realization loss. "
            "Implementation-defined row order in the absence of an ORDER BY clause, transaction isolation, "
            "and sync versus async behavior are realization loss dimensions."
        ),
    )
    row_post["cardinality"] = "at-most-one"
    row_post["row_shape_sketch"] = {
        "result_sort": "Optional<SqlRow>",
        "row_sort": "SqlRow",
    }

    # concept:sql-query-all : fully-materialized array of all matching rows.
    all_post = operation_contract(
        "sql-query-all",
        ["Sql", "SqlArgs"],
        "SqlRowSet",
        slots,
        (
            "Executes a parameterized SQL read operation and returns a fully-materialized array of all matching rows. "
            "Sql is the parameterized query string. SqlArgs carries positional or named parameters. "
            "SqlRowSet is a fully-materialized, in-memory row set: indexable, length-known, and re-readable. "
            "Materializing every matching row in memory is the post-condition of this concept, not a realization loss. "
            "Implementation-defined row order in the absence of an ORDER BY clause, transaction isolation, "
            "and sync versus async behavior are realization loss dimensions."
        ),
    )
    all_post["cardinality"] = "all"
    all_post["materialization"] = "fully-materialized"
    all_post["row_shape_sketch"] = {
        "row_set_sort": "SqlRowSet",
        "row_sort": "SqlRow",
    }

    # concept:sql-query-iterate : lazy single-pass cursor over rows.
    iterate_post = operation_contract(
        "sql-query-iterate",
        ["Sql", "SqlArgs"],
        "SqlRowCursor",
        slots,
        (
            "Executes a parameterized SQL read operation and returns a lazy single-pass cursor over the matching rows. "
            "Sql is the parameterized query string. SqlArgs carries positional or named parameters. "
            "SqlRowCursor is a lazy, not-materialized, consume-once cursor whose validity is bound to the cursor lifetime: "
            "rows are produced on demand and cannot be re-read or indexed. "
            "Returning a lazy single-pass cursor rather than a materialized row set is the post-condition of this concept, "
            "not a realization loss. "
            "Implementation-defined row order in the absence of an ORDER BY clause, transaction isolation, "
            "and sync versus async behavior are realization loss dimensions."
        ),
    )
    iterate_post["cardinality"] = "all"
    iterate_post["materialization"] = "lazy-single-pass"
    iterate_post["cursor_lifetime_bound"] = True
    iterate_post["row_shape_sketch"] = {
        "cursor_sort": "SqlRowCursor",
        "row_sort": "SqlRow",
    }

    return {
        "sql-query-row": shape_spec(
            "concept:sql-query-row",
            ["sql", "args"],
            ["Sql", "SqlArgs"],
            "Optional<SqlRow>",
            row_post,
            SQL_QUERY_VARIANT_LOSS_DIMS,
        ),
        "sql-query-all": shape_spec(
            "concept:sql-query-all",
            ["sql", "args"],
            ["Sql", "SqlArgs"],
            "SqlRowSet",
            all_post,
            SQL_QUERY_VARIANT_LOSS_DIMS,
        ),
        "sql-query-iterate": shape_spec(
            "concept:sql-query-iterate",
            ["sql", "args"],
            ["Sql", "SqlArgs"],
            "SqlRowCursor",
            iterate_post,
            SQL_QUERY_VARIANT_LOSS_DIMS,
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
        print(f"sql_query_variant_cid\t{row['name']}\t{row['cid']}")
    return rows


if __name__ == "__main__":
    mint_all()
