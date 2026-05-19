"use strict";

const { test } = require("node:test");
const assert = require("node:assert/strict");

const { declaration, dimensionValues, CONCEPT_INSERT_AND_GET_ID_CID } = require("../src/platform_semantics");
const { dispatch } = require("../src/rpc");

// Golden CIDs (kit_cid elided per substrate spec).
const GOLDEN_RETURNING_CLAUSE_CID =
  "blake3-512:8cd2692a556731b240b590ec2a194628f7db28a9f0816f4993677f64b18b72ac9591a6ae519c6e13fa2fe60dff3c6c0943b5bc8fd6ef00f298702def3d3e996e";
const GOLDEN_INSERT_TAG_CID =
  "blake3-512:51eb072fd0f9cf1b7056bff0c7f085d4f917e9964ef5511fcb8c726402cf765dc0607138671410335f2812f36bac1ff1b52a67d149b8b5a87d11cf329419cf7c";

// Positive: declaration is non-empty
test("pg_declaration_is_non_empty", () => {
  const decl = declaration();
  assert.ok(decl.tags.length > 0, "must declare at least one op tag");
  assert.ok(decl.dimension_values.length > 0, "must declare dimension values");
  assert.ok(
    decl.tags.some((t) => t.op_cid === CONCEPT_INSERT_AND_GET_ID_CID),
    "must declare concept:insert-and-get-id"
  );
});

// Positive: ReturningClause dimension value CID matches golden
test("pg_returning_clause_cid_matches_golden", () => {
  const dvs = dimensionValues();
  const rowId = dvs.find((d) => d.dimension_name === "RowIdMechanism");
  assert.ok(rowId, "must have RowIdMechanism dimension");
  assert.strictEqual(rowId.value_name, "ReturningClause");
  assert.strictEqual(rowId.cid, GOLDEN_RETURNING_CLAUSE_CID);
});

// Positive: insert-and-get-id tag CID matches golden
test("pg_insert_tag_cid_matches_golden", () => {
  const decl = declaration();
  const tag = decl.tags.find((t) => t.op_cid === CONCEPT_INSERT_AND_GET_ID_CID);
  assert.ok(tag, "must have insert-and-get-id tag");
  assert.strictEqual(tag.cid, GOLDEN_INSERT_TAG_CID);
});

// Discrimination: pg ReturningClause must differ from better-sqlite3 LastInsertRowid
test("pg_returning_clause_differs_from_last_insert_rowid", () => {
  const dvs = dimensionValues();
  const rowId = dvs.find((d) => d.dimension_name === "RowIdMechanism");
  // LastInsertRowid golden CID (better-sqlite3)
  const lastInsertRowidCid =
    "blake3-512:619f9cb06fa946350f9c8050f0be5281c6e7f67730be491bbe1223e549263ef6cb63751c1c3ea4f2df23a25a9d7307fcbb9634e58a13983e0900d40240fc2cf6";
  assert.notStrictEqual(
    rowId.cid, lastInsertRowidCid,
    "ReturningClause and LastInsertRowid must hash to different CIDs"
  );
});

// Structural: compare_to is a proper Atomic formula with IrTerm::Ctor args
test("pg_dimension_value_compare_to_structure", () => {
  const dvs = dimensionValues();
  const rowId = dvs.find((d) => d.dimension_name === "RowIdMechanism");
  assert.strictEqual(rowId.compare_to.kind, "atomic");
  assert.strictEqual(rowId.compare_to.name, "row_id_source");
  assert.strictEqual(rowId.compare_to.args.length, 1);
  const arg = rowId.compare_to.args[0];
  assert.strictEqual(arg.kind, "ctor");
  assert.strictEqual(arg.name, "returning_clause_result");
});

// Positive: RPC dispatch returns correct shape
test("pg_rpc_dispatch_platform_semantics", () => {
  const response = dispatch({ jsonrpc: "2.0", id: 7, method: "provekit.plugin.platform_semantics" });
  assert.strictEqual(response.jsonrpc, "2.0");
  assert.strictEqual(response.id, 7);
  assert.ok(Array.isArray(response.result.tags));
  assert.ok(Array.isArray(response.result.dimension_values));
  assert.deepStrictEqual(response.result.op_aliases, {});
});
