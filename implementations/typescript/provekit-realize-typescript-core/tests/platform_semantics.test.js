"use strict";

const { test } = require("node:test");
const assert = require("node:assert/strict");

const { declaration, dimensionValues } = require("../src/platform_semantics");
const { dispatch } = require("../src/rpc");

// Golden CIDs for dimension values (kit_cid elided per substrate spec).
// Independently computed and cross-checked against Python core golden values.
const GOLDEN_DIM_VALUE_CIDS = {
  ArithmeticOverflow:       "blake3-512:f4997b8efc9565dbbd3e5339017b2c8ae097912e05a3d349ec7dbbd78deb2fe6e835e88086fd8802902d6208ba17d777bc4dcd27e904c5bf768d95010d09fef3",
  IntegerDivisionRounding:  "blake3-512:12acb888fe27733082713413b587bc20e8437dc6ec1873516de9f9476f8cc3c76e0f7caaf0cb91d41d26f2e6ffa9873cf2a3786188ecd7fbd8f23ee77e2ed63a",
  NullSemantics:            "blake3-512:2b6e67a5513cc768b1c59b9fdbf7fb7ef3f7d12235a4b8c1cd63fcbc52b0c23a8e43b88cbd3bd4c730f2f5f40751d3f1b8c6ba12f3c1307b7a0ff81f593d492f",
  ShiftMode:                "blake3-512:31de26cc4a328f4a3817d5b9587fa67c0cc30b07e1109113e6727f5a553afb0e1f1dc8f01d10cec210f3c04956df2b062c4a127612d535cdbb018ecf4531f5fa",
  BitwiseSemantics:         "blake3-512:de3c71b070d8e228ad2699a0013f95c4ec18a638a1b1e8c5d406737dddde297b738c38eed094f99d8d512ba1f9255ecac911191134f5bea228f68bb4cddde78a",
};

const EXPECTED_DIMENSIONS = {
  ArithmeticOverflow:      "Ieee754Saturate",
  IntegerDivisionRounding: "FloatDivision",
  NullSemantics:           "ReturnsNanOrInfinity",
  ShiftMode:               "Int32Wrapping",
  BitwiseSemantics:        "Int32",
};

// Positive: declaration is non-empty
test("ts_declaration_is_non_empty", () => {
  const decl = declaration();
  assert.ok(decl.tags.length > 0, "must declare at least one op tag");
  assert.ok(decl.dimension_values.length > 0, "must declare dimension values");
});

// Positive: dimension values have correct dimension_name/value_name mappings
test("ts_dimension_value_names", () => {
  const dvs = dimensionValues();
  const actual = {};
  for (const dv of dvs) {
    actual[dv.dimension_name] = dv.value_name;
  }
  assert.deepStrictEqual(actual, EXPECTED_DIMENSIONS);
});

// Positive: dimension value CIDs match goldens
test("ts_dimension_value_cids_match_goldens", () => {
  const dvs = dimensionValues();
  for (const dv of dvs) {
    const expected = GOLDEN_DIM_VALUE_CIDS[dv.dimension_name];
    assert.strictEqual(
      dv.cid, expected,
      `${dv.dimension_name} CID mismatch`
    );
  }
});

// Positive: all tag CIDs start with blake3-512: and are 139 chars
test("ts_tag_cids_are_valid_blake3_512", () => {
  const decl = declaration();
  for (const tag of decl.tags) {
    assert.ok(tag.cid.startsWith("blake3-512:"), `tag ${tag.op_cid}: CID must start with blake3-512:`);
    assert.strictEqual(tag.cid.length, 139, `tag ${tag.op_cid}: CID must be 139 chars`);
  }
});

// Discrimination: TS uses Ieee754Saturate; Python uses ArbitraryPrecision.
// They must produce different ArithmeticOverflow dimension value CIDs.
test("ts_arithmetic_overflow_differs_from_python", () => {
  const dvs = dimensionValues();
  const tsOverflow = dvs.find((d) => d.dimension_name === "ArithmeticOverflow");
  assert.ok(tsOverflow, "must have ArithmeticOverflow");
  assert.strictEqual(tsOverflow.value_name, "Ieee754Saturate");
  // Python ArbitraryPrecision golden CID (from python-core tests)
  const pythonArbitraryPrecisionCid =
    "blake3-512:d528ffa68485e200a65ac1119b3561aa28b56f52a04a31059ff41afeeff812843c4b9b12be9682445481b304e30d166baa64bc03fdb5f0fe40e07a0b1091d373";
  assert.notStrictEqual(
    tsOverflow.cid, pythonArbitraryPrecisionCid,
    "Ieee754Saturate and ArbitraryPrecision must hash to different CIDs"
  );
});

// Structural: compare_to for each dimension value is a proper atomic formula
test("ts_dimension_value_compare_to_shapes", () => {
  const dvs = dimensionValues();
  for (const dv of dvs) {
    assert.strictEqual(dv.compare_to.kind, "atomic");
    assert.ok(dv.compare_to.name.startsWith("typescript:"), `compare_to.name must be 'typescript:...'`);
    assert.deepStrictEqual(dv.compare_to.args, []);
  }
});

// Positive: RPC dispatch returns correct shape for platform_semantics
test("ts_rpc_dispatch_platform_semantics", () => {
  const response = dispatch({ jsonrpc: "2.0", id: 42, method: "provekit.plugin.platform_semantics" });
  assert.strictEqual(response.jsonrpc, "2.0");
  assert.strictEqual(response.id, 42);
  assert.ok(Array.isArray(response.result.tags));
  assert.ok(Array.isArray(response.result.dimension_values));
  assert.deepStrictEqual(response.result.op_aliases, {});
  assert.ok(response.result.tags.length > 0);
  assert.ok(response.result.dimension_values.length > 0);
});
