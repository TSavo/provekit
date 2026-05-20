// SPDX-License-Identifier: Apache-2.0
//
// TypeScript kit literal-encoding answers.
//
// Implements the provekit.plugin.literal_encoding_answers RPC method.
// Returns one LiteralEncodingMemento per sort the TypeScript kit admits
// at literal positions per its SortAdmission declaration.
//
// TypeScript admits: Float, String, Bool, Null (no Int, no Bytes).
// JS Number is IEEE 754 double; there is no distinct integer literal sort.
// Buffers/Uint8Array are not a primitive literal sort at the JS/TS value layer.

"use strict";

const { blake3 } = require("@noble/hashes/blake3.js");
const { KIT_CID } = require("./platform_semantics");

// Canonical sort CIDs (from #1282)
const SORT_BOOL_CID   = "blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074";
const SORT_NULL_CID   = "blake3-512:62f6040bd3f414c1e6c2b7bdf276669cd5613b33cb508a81170170064ca3ffba771a4b0002dc52e059fce5f9f63a1874ef71bd4ec89ae06e89c87a3e91aac3b5";
const SORT_FLOAT_CID  = "blake3-512:b979e70c4d5e53d9bdf13d6f08330be3c5b0714b8c770d69bbd05946b86c36df5274be8145a2683cc29c278155c9c1ee65b6897913524eecb9e4c89c71862f57";
const SORT_STRING_CID = "blake3-512:be8721d24849feb74c4721520bdba02d352a94f49253a627cd509127472aa1c47cbe99cb705cac4159b5365abcce0c9aaa4901fe67630827deb6be1f9daeea10";

const CONCEPT_LITERAL_NAME = "concept:literal";

/**
 * Returns the set of LiteralEncodingMemento answers for the TypeScript kit.
 * TypeScript admits: Float (3.14), String ("hello"), Bool (true), Null (null).
 */
function answers() {
  return [
    // Float: bit-preserving shape {"__float_bits__": <u64>} (IEEE 754 raw bits).
    // 4614253070214989087 == 0x40091EB851EB851F == bits of 3.14 as f64.
    // JS cannot represent this u64 in a JSON number without precision loss, so the
    // CID is computed from a manually-built JCS string; the value object carries
    // the nearest IEEE 754 double (4614253070214989087 rounds to 4614253070214989000
    // in JS Number -- a known TS platform limitation documented in the substrate issue).
    _mementoFloatBits(SORT_FLOAT_CID, "3.14", 4614253070214989087n),
    _memento(SORT_STRING_CID, '"hello"', "hello"),
    _memento(SORT_BOOL_CID, "true", true),
    _memento(SORT_NULL_CID, "null", null),
  ];
}

function _memento(sortCid, sourceExample, decodedValue) {
  const expectedTermShapeNode = {
    concept_name: CONCEPT_LITERAL_NAME,
    sort: sortCid,
    value: decodedValue,
  };
  const forCid = {
    expected_term_shape_node: expectedTermShapeNode,
    kind: "literal-encoding-memento",
    language: "typescript",
    schemaVersion: "1.0.0",
    sort_cid: sortCid,
    source_example: sourceExample,
  };
  return {
    cid: _cidOf(forCid),
    expected_term_shape_node: expectedTermShapeNode,
    kind: "literal-encoding-memento",
    kit_cid: KIT_CID,
    language: "typescript",
    schemaVersion: "1.0.0",
    sort_cid: sortCid,
    source_example: sourceExample,
  };
}

/**
 * Special-case memento builder for Float __float_bits__ shape.
 * Builds the JCS string manually to avoid JS Number precision loss when
 * serializing u64 values that exceed Number.MAX_SAFE_INTEGER.
 *
 * @param {string} sortCid
 * @param {string} sourceExample
 * @param {bigint} floatBits - IEEE 754 raw bits as BigInt
 */
function _mementoFloatBits(sortCid, sourceExample, floatBits) {
  const floatBitsStr = floatBits.toString();
  // Build JCS (RFC 8785) manually: keys sorted alphabetically, no whitespace.
  // expected_term_shape_node inner keys: concept_name, sort, value (alphabetical order)
  const valueJson = "{\"__float_bits__\":" + floatBitsStr + "}";
  const termShapeJson =
    "{\"concept_name\":\"concept:literal\"" +
    ",\"sort\":" + JSON.stringify(sortCid) +
    ",\"value\":" + valueJson +
    "}";
  // Top-level forCid keys: expected_term_shape_node, kind, language, schemaVersion, sort_cid, source_example
  const forCidJson =
    "{\"expected_term_shape_node\":" + termShapeJson +
    ",\"kind\":\"literal-encoding-memento\"" +
    ",\"language\":\"typescript\"" +
    ",\"schemaVersion\":\"1.0.0\"" +
    ",\"sort_cid\":" + JSON.stringify(sortCid) +
    ",\"source_example\":" + JSON.stringify(sourceExample) +
    "}";
  const cid = _cidOfRaw(forCidJson);
  return {
    cid,
    expected_term_shape_node: {
      concept_name: CONCEPT_LITERAL_NAME,
      sort: sortCid,
      value: { __float_bits__: Number(floatBits) }, // nearest representable; CID is exact
    },
    kind: "literal-encoding-memento",
    kit_cid: KIT_CID,
    language: "typescript",
    schemaVersion: "1.0.0",
    sort_cid: sortCid,
    source_example: sourceExample,
  };
}

function _cidOfRaw(jcsString) {
  const bytes = new TextEncoder().encode(jcsString);
  const hash = blake3(bytes, { dkLen: 64 });
  return "blake3-512:" + _hexOf(hash);
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

module.exports = { answers };
