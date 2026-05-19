"use strict";

const { test } = require("node:test");
const assert = require("node:assert/strict");

const { declaration, dimensionValues, CONCEPT_INSERT_AND_GET_ID_CID } = require("../src/platform_semantics");
const { dispatch } = require("../src/rpc");

// Golden CIDs (kit_cid elided per substrate spec).
const GOLDEN_LAST_INSERT_ROWID_CID =
  "blake3-512:619f9cb06fa946350f9c8050f0be5281c6e7f67730be491bbe1223e549263ef6cb63751c1c3ea4f2df23a25a9d7307fcbb9634e58a13983e0900d40240fc2cf6";
const GOLDEN_INSERT_TAG_CID =
  "blake3-512:8e75d18dda9dcc90955d32c276e40792aca6e4ef830e2c3d1526a10320968f690d2d2d65cef540d5cccfc824c3524c2de32c23a4236e6362c73d97615c91b194";

// Positive: declaration is non-empty
test("bsq_declaration_is_non_empty", () => {
  const decl = declaration();
  assert.ok(decl.tags.length > 0, "must declare at least one op tag");
  assert.ok(decl.dimension_values.length > 0, "must declare dimension values");
  assert.ok(
    decl.tags.some((t) => t.op_cid === CONCEPT_INSERT_AND_GET_ID_CID),
    "must declare concept:insert-and-get-id"
  );
});

// Positive: LastInsertRowid dimension value CID matches golden
test("bsq_last_insert_rowid_cid_matches_golden", () => {
  const dvs = dimensionValues();
  const rowId = dvs.find((d) => d.dimension_name === "RowIdMechanism");
  assert.ok(rowId, "must have RowIdMechanism dimension");
  assert.strictEqual(rowId.value_name, "LastInsertRowid");
  assert.strictEqual(rowId.cid, GOLDEN_LAST_INSERT_ROWID_CID);
});

// Positive: insert-and-get-id tag CID matches golden
test("bsq_insert_tag_cid_matches_golden", () => {
  const decl = declaration();
  const tag = decl.tags.find((t) => t.op_cid === CONCEPT_INSERT_AND_GET_ID_CID);
  assert.ok(tag, "must have insert-and-get-id tag");
  assert.strictEqual(tag.cid, GOLDEN_INSERT_TAG_CID);
});

// Discrimination: better-sqlite3 LastInsertRowid must differ from python-sqlite3 CursorLastRowid
test("bsq_last_insert_rowid_differs_from_cursor_lastrowid", () => {
  const dvs = dimensionValues();
  const rowId = dvs.find((d) => d.dimension_name === "RowIdMechanism");
  // CursorLastRowid golden CID (python-sqlite3 / python-aiosqlite)
  const cursorLastRowidCid =
    "blake3-512:6fbe68f4eb8a7cf5e58bd5859f43ce9bff042e3b68f85d6576fd3055d08f9d2a36bf9c316f6580111e178d82495b820b66414c934e5723d0e1c3a337df269933";
  assert.notStrictEqual(
    rowId.cid, cursorLastRowidCid,
    "LastInsertRowid and CursorLastRowid must hash to different CIDs"
  );
});

// Structural: compare_to is a proper Atomic formula with IrTerm::Ctor args
test("bsq_dimension_value_compare_to_structure", () => {
  const dvs = dimensionValues();
  const rowId = dvs.find((d) => d.dimension_name === "RowIdMechanism");
  assert.strictEqual(rowId.compare_to.kind, "atomic");
  assert.strictEqual(rowId.compare_to.name, "row_id_source");
  assert.strictEqual(rowId.compare_to.args.length, 1);
  const arg = rowId.compare_to.args[0];
  assert.strictEqual(arg.kind, "ctor");
  assert.strictEqual(arg.name, "last_insert_rowid");
});

// Positive: RPC dispatch returns correct shape
test("bsq_rpc_dispatch_platform_semantics", () => {
  const response = dispatch({ jsonrpc: "2.0", id: 1, method: "provekit.plugin.platform_semantics" });
  assert.strictEqual(response.jsonrpc, "2.0");
  assert.strictEqual(response.id, 1);
  assert.ok(Array.isArray(response.result.tags));
  assert.ok(Array.isArray(response.result.dimension_values));
  assert.deepStrictEqual(response.result.op_aliases, {});
});
