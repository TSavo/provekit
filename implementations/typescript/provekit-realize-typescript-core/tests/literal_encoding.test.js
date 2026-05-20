"use strict";

const { test } = require("node:test");
const assert = require("node:assert/strict");

const { answers } = require("../src/literal_encoding");
const { dispatch } = require("../src/rpc");

// Canonical sort CIDs (from #1282)
const SORT_BOOL_CID   = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_NULL_CID   = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";
const SORT_FLOAT_CID  = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";

// TypeScript admits: Float, String, Bool, Null (no Int, no Bytes) -- 4 answers.

test("ts_literal_encoding_answers_count", () => {
  const a = answers();
  assert.strictEqual(a.length, 4, "TypeScript admits Float, String, Bool, Null (4 sorts)");
});

test("ts_literal_encoding_answers_sort_cids", () => {
  const a = answers();
  const sortCids = new Set(a.map((m) => m.sort_cid));
  assert.ok(sortCids.has(SORT_FLOAT_CID), "must contain Float");
  assert.ok(sortCids.has(SORT_STRING_CID), "must contain String");
  assert.ok(sortCids.has(SORT_BOOL_CID), "must contain Bool");
  assert.ok(sortCids.has(SORT_NULL_CID), "must contain Null");
});

test("ts_literal_encoding_answers_language", () => {
  const a = answers();
  for (const m of a) {
    assert.strictEqual(m.language, "typescript", "language must be typescript");
  }
});

test("ts_literal_encoding_answers_kind", () => {
  const a = answers();
  for (const m of a) {
    assert.strictEqual(m.kind, "literal-encoding-memento");
  }
});

test("ts_literal_encoding_answers_cid_format", () => {
  const a = answers();
  for (const m of a) {
    assert.ok(m.cid.startsWith("blake3-512:"), "CID must start with blake3-512:");
    assert.ok(m.cid.length > 20, "CID must not be empty");
  }
});

// Golden LiteralEncodingMemento CIDs (kit_cid elided per #1262 / #1271)
// TS uses manually-built JCS for Float (BigInt-safe) and _sortKeys+JSON.stringify for others.
// Float value is {"__float_bits__": 4614253070214989087} (bit-preserving, #1262).
const GOLDEN_CIDS = {
  [SORT_FLOAT_CID]:  "blake3-512:00600d78e49b56cf1db5aedda4927602105460c8cea4a628fd83d614121f7c4063a36d4754c62080fd9ec73d24555f50e42a4ef4e4360282491621ec403d1da4",
  [SORT_STRING_CID]: "blake3-512:2afc602d467f858fb5c5e58138e1365be54f9a698a30ed823d5c5c5b966dc24a140c58ab5e1bd75a4a6239d7b934e8d1b7e78f8eb84a96b668f1f8dd3a7049f3",
  [SORT_BOOL_CID]:   "blake3-512:784022d1f5e6a28447da659e347b009e2730b5ac0ab68a3d774e9b46cb59ed1bd07f76e9d6f48f40e123af93f2d65b89d5af4e0b57737694a0835c15405ab7e7",
  [SORT_NULL_CID]:   "blake3-512:d58343632e0e01b35f866256148c90b529656ed22da8ab7e4a1b96e823f651ba5d27145470e514a71443aee432a5d98bb83c19eac9fefd4502d926dbbc62b75e",
};

test("ts_literal_encoding_answers_golden_cids", () => {
  const a = answers();
  const bySortCid = {};
  for (const m of a) bySortCid[m.sort_cid] = m.cid;

  for (const [sortCid, expectedCid] of Object.entries(GOLDEN_CIDS)) {
    assert.strictEqual(
      bySortCid[sortCid], expectedCid,
      `Golden CID mismatch for sort ${sortCid.slice(0, 20)}`
    );
  }
});

test("ts_literal_encoding_answers_rpc_dispatch", () => {
  const response = dispatch({
    jsonrpc: "2.0",
    id: 1,
    method: "provekit.plugin.literal_encoding_answers",
    params: {},
  });
  assert.strictEqual(response.jsonrpc, "2.0");
  assert.strictEqual(response.id, 1);
  assert.ok(Array.isArray(response.result.answers));
  assert.strictEqual(response.result.answers.length, 4);
});
