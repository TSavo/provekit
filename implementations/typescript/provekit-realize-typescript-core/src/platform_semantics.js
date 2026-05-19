// SPDX-License-Identifier: Apache-2.0
//
// TypeScript kit platform semantics declaration.
//
// Implements the provekit.plugin.platform_semantics RPC method (PEP 1.7.0).
// Returns the JSON payload for the "typescript" target.
//
// JavaScript/TypeScript arithmetic uses IEEE 754 double-precision floating
// point for the Number type. There is no integer arithmetic type at the
// language level; all arithmetic ops produce doubles. Bitwise operators
// coerce their operands to signed 32-bit integers via the ToInt32 algorithm
// before operating, then return a double whose value equals the 32-bit result.
//
// CID computation follows the substrate spec:
//   DimensionValueMemento CID = blake3-512(JCS(memento WITHOUT cid + kit_cid))
//   PlatformSemanticTag CID   = blake3-512(JCS(tag WITHOUT cid + kit_cid))

"use strict";

const { blake3 } = require("@noble/hashes/blake3.js");

const KIT_ID = "provekit-realize-typescript-core@0.1.0";

// kit_cid is provenance metadata only (elided from CID computation per substrate spec).
// Computed as blake3-512(utf8(KIT_ID)) with dkLen=64.
const KIT_CID = "blake3-512:" + _hexOf(blake3(
  new TextEncoder().encode(KIT_ID),
  { dkLen: 64 }
));

// concept:literal CID (from #1282)
const CONCEPT_LITERAL_CID = "blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6";

// Canonical sort CIDs (from #1282)
// TypeScript admits: Float, String, Bool, Null (JS Number is IEEE 754 double; no Int or Bytes at language level)
// Args sorted alphabetically by CID string: BOOL (0ee1) < NULL (62f6) < FLOAT (b979) < STRING (be87)
const SORT_BOOL_CID   = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_NULL_CID   = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";
const SORT_FLOAT_CID  = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";

// Concept-op CIDs shared across all language kits (cross-kit hub CIDs).
// Source of truth: menagerie/concept-shapes/cids.tsv.
const OP_ADD    = "blake3-512:398980644a46039b0c2875ab36ccb61f52f284ccad5481593305ed3f10efe91e7863c00a3f2d673644430f691e6b5354f5d65f9da4fa23acdb13dc58f5b438f9";
const OP_SUB    = "blake3-512:b6c62a64669ff12d0af45d9932c1ab5e08576f1cac97b4abe60392a9f02393dac9765514b024b1481ddc829d4b7fb97950ad648a9944dceafa194b8423923533";
const OP_MUL    = "blake3-512:1df457dceb0ec7a6dc4596eb70be001be09180afc69fa3ff8121cd78a0daff5dd9606dbfd4fb9fcdc5d834939a6f19c52b80aace16dea6df5ffdce62d86bbfa2";
const OP_DIV    = "blake3-512:d7403da8d2a8921b71170b5fc34c12022118d0c545f25c7ff89fe77bbed02419e3528479ded0e746535ee92d0e1801bce46608c15c3d6d2a5567bec811cbc75a";
const OP_REM    = "blake3-512:235c6177611c2753a1c0d07d44391f5465ab50dc585372df52220118cb103ef19502192a07148bd2969d7f6f7ed0d134714d7745825f486768d0b0de8ac0b6dc";
const OP_SHL    = "blake3-512:37af5330572cf08650e3b6d5fdfc2649d56c0bb2e019f9be3861082c9d1961c1808beca6f9dfc39742ade25f06bfb499da74c89d33f64decd0c70f0972d021e1";
const OP_SHR    = "blake3-512:cb23fbc9d05a19b353e1fe85c77e241fdc8c58cde5a7c5cad008b721a51eaf682284d8bfe3b383d751cb58833e94beb6bd0dd4d330f9619f095c8b4daa8298da";
const OP_BITAND = "blake3-512:fcc41d285a20dae6c2deb2a854665d5d43bc829a09a76107d929898b3b169d1abf53ed71f302b00ec2146bcec3b5fe732ca7ecd4354e7739e67feea3db9fd6a2";
const OP_BITOR  = "blake3-512:5c455355a13fd97a872848613b34b2b56f9738c832f900558710af1cd053976157513f31a8feb123202557dc0a369b88bc7c946179fe817d6c2f80d4f318f824";
const OP_BITXOR = "blake3-512:16ba612da4883e853dd18b08c8e7b1803e1e2b0a42ab83c261048a49cdfd9b20bc54e809b8f4e8e5c9af63cc7447dee039cb826c611dfec137855a11a502adb9";
const OP_NEG    = "blake3-512:e0c3e13fd7e0d11fa3b78f4e083ab60b1166bdd905bc04e533e6dcc97d79330bd6a403caaf1265d8134ea3ccd5fe8cfd5a3e18f349ea7edcb6310c098e845c0f";
const OP_BITNOT = "blake3-512:eeaaf14737f661b6bce03f23d281974502182fea83909eeaade25e510887b26e80dac1b10af3b1f2f496b53898051d63e8d250e78cfa8e88380c84809e5eabe0";

// Cached result to avoid repeated computation.
let _cached = null;

/** Returns the platform_semantics declaration for the TypeScript kit. */
function declaration() {
  if (_cached !== null) return _cached;

  const dimValues = dimensionValues();
  const dimCids = {};
  for (const dv of dimValues) {
    dimCids[dv.dimension_name] = dv.cid;
  }

  const tags = [
    // Arithmetic ops: IEEE 754 saturate on overflow (-> +/-Infinity, not wrap or panic)
    _tag(OP_ADD, { ArithmeticOverflow: dimCids.ArithmeticOverflow }),
    _tag(OP_SUB, { ArithmeticOverflow: dimCids.ArithmeticOverflow }),
    _tag(OP_MUL, { ArithmeticOverflow: dimCids.ArithmeticOverflow }),
    _tag(OP_NEG, { ArithmeticOverflow: dimCids.ArithmeticOverflow }),
    // Division: always float (no integer truncation), div-by-zero gives NaN/Infinity
    _tag(OP_DIV, {
      IntegerDivisionRounding: dimCids.IntegerDivisionRounding,
      NullSemantics: dimCids.NullSemantics,
    }),
    // Remainder: same float semantics, NaN/Infinity on zero
    _tag(OP_REM, {
      IntegerDivisionRounding: dimCids.IntegerDivisionRounding,
      NullSemantics: dimCids.NullSemantics,
    }),
    // Bitwise shifts: ToInt32 coercion then wrapping shift
    _tag(OP_SHL, { ShiftMode: dimCids.ShiftMode }),
    _tag(OP_SHR, { ShiftMode: dimCids.ShiftMode }),
    // Bitwise ops: ToInt32 coercion
    _tag(OP_BITAND, { BitwiseSemantics: dimCids.BitwiseSemantics }),
    _tag(OP_BITOR,  { BitwiseSemantics: dimCids.BitwiseSemantics }),
    _tag(OP_BITXOR, { BitwiseSemantics: dimCids.BitwiseSemantics }),
    _tag(OP_BITNOT, { BitwiseSemantics: dimCids.BitwiseSemantics }),
    // concept:literal: only SortAdmission (Float, String, Bool, Null)
    _tag(CONCEPT_LITERAL_CID, { SortAdmission: dimCids.SortAdmission }),
  ];

  _cached = { tags, dimension_values: dimValues };
  return _cached;
}

/** Returns the dimension values for the TypeScript kit. */
function dimensionValues() {
  return [
    // ArithmeticOverflow: IEEE 754 double saturates to +/-Infinity rather than wrapping
    _dimValue("ArithmeticOverflow", "Ieee754Saturate"),
    // IntegerDivisionRounding: JS `/` always performs float division, never truncates
    _dimValue("IntegerDivisionRounding", "FloatDivision"),
    // NullSemantics: division or remainder by zero returns NaN or Infinity, no exception
    _dimValue("NullSemantics", "ReturnsNanOrInfinity"),
    // ShiftMode: bitwise shift operands are coerced to Int32 with wrapping
    _dimValue("ShiftMode", "Int32Wrapping"),
    // BitwiseSemantics: bitwise operands are coerced to Int32 via ToInt32 algorithm
    _dimValue("BitwiseSemantics", "Int32"),
    // concept:literal SortAdmission: TS admits Float, String, Bool, Null (no Int, no Bytes)
    _dimValueAdmitsSorts("SortAdmission", "JsValueTier", [
      SORT_BOOL_CID,   // 0ee1...
      SORT_NULL_CID,   // 62f6...
      SORT_FLOAT_CID,  // b979...
      SORT_STRING_CID, // be87...
    ]),
  ];
}

// --- Helpers ---

function _dimValue(dimensionName, valueName) {
  const compareTo = { kind: "atomic", name: `typescript:${valueName}`, args: [] };
  const forCid = {
    compare_to: compareTo,
    dimension_name: dimensionName,
    kind: "platform-dimension-value",
    schemaVersion: "1.0.0",
    value_name: valueName,
  };
  return {
    compare_to: compareTo,
    dimension_name: dimensionName,
    kind: "platform-dimension-value",
    kit_cid: KIT_CID,
    schemaVersion: "1.0.0",
    value_name: valueName,
    cid: _cidOf(forCid),
  };
}

// Build a DimensionValueMemento for an admits_sorts formula.
// sortedCids must be pre-sorted alphabetically by CID string value.
function _dimValueAdmitsSorts(dimensionName, valueName, sortedCids) {
  const args = sortedCids.map((cid) => ({
    kind: "const",
    sort: { kind: "primitive", name: "cid" },
    value: cid,
  }));
  const compareTo = { args, kind: "atomic", name: "admits_sorts" };
  const forCid = {
    compare_to: compareTo,
    dimension_name: dimensionName,
    kind: "platform-dimension-value",
    schemaVersion: "1.0.0",
    value_name: valueName,
  };
  return {
    compare_to: compareTo,
    dimension_name: dimensionName,
    kind: "platform-dimension-value",
    kit_cid: KIT_CID,
    schemaVersion: "1.0.0",
    value_name: valueName,
    cid: _cidOf(forCid),
  };
}

function _tag(opCid, dimensions) {
  const forCid = {
    dimensions,
    kind: "platform-semantic-tag",
    op_cid: opCid,
    schemaVersion: "1.0.0",
  };
  return {
    dimensions,
    kind: "platform-semantic-tag",
    kit_cid: KIT_CID,
    op_cid: opCid,
    schemaVersion: "1.0.0",
    cid: _cidOf(forCid),
  };
}

function _cidOf(obj) {
  const canonical = _jcs(obj);
  const bytes = new TextEncoder().encode(canonical);
  const hash = blake3(bytes, { dkLen: 64 });
  return "blake3-512:" + _hexOf(hash);
}

function _jcs(obj) {
  return JSON.stringify(_sortKeys(obj));
}

function _sortKeys(val) {
  if (val === null || typeof val !== "object") return val;
  if (Array.isArray(val)) return val.map(_sortKeys);
  return Object.fromEntries(
    Object.keys(val).sort().map((k) => [k, _sortKeys(val[k])])
  );
}

function _hexOf(bytes) {
  return Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}

module.exports = { declaration, dimensionValues, KIT_CID, CONCEPT_LITERAL_CID };
