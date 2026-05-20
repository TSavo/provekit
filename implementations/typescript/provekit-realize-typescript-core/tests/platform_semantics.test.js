"use strict";

const { test } = require("node:test");
const assert = require("node:assert/strict");

const { declaration, dimensionValues, CONCEPT_LITERAL_CID, _jcs } = require("../src/platform_semantics");
const { dispatch } = require("../src/rpc");

// Golden CIDs for dimension values (kit_cid elided per substrate spec).
// Independently computed and cross-checked against Python core golden values.
const GOLDEN_DIM_VALUE_CIDS = {
  ArithmeticOverflow:       "blake3-512:f4997b8efc9565dbbd3e5339017b2c8ae097912e05a3d349ec7dbbd78deb2fe6e835e88086fd8802902d6208ba17d777bc4dcd27e904c5bf768d95010d09fef3",
  IntegerDivisionRounding:  "blake3-512:12acb888fe27733082713413b587bc20e8437dc6ec1873516de9f9476f8cc3c76e0f7caaf0cb91d41d26f2e6ffa9873cf2a3786188ecd7fbd8f23ee77e2ed63a",
  NullSemantics:            "blake3-512:2b6e67a5513cc768b1c59b9fdbf7fb7ef3f7d12235a4b8c1cd63fcbc52b0c23a8e43b88cbd3bd4c730f2f5f40751d3f1b8c6ba12f3c1307b7a0ff81f593d492f",
  ShiftMode:                "blake3-512:31de26cc4a328f4a3817d5b9587fa67c0cc30b07e1109113e6727f5a553afb0e1f1dc8f01d10cec210f3c04956df2b062c4a127612d535cdbb018ecf4531f5fa",
  BitwiseSemantics:         "blake3-512:de3c71b070d8e228ad2699a0013f95c4ec18a638a1b1e8c5d406737dddde297b738c38eed094f99d8d512ba1f9255ecac911191134f5bea228f68bb4cddde78a",
  // concept:literal SortAdmission: TS admits Float, String, Bool, Null (no Int, no Bytes)
  // value_name "JsValueTier" -- TS-specific, no cross-kit equivalence claim
  SortAdmission:            "blake3-512:91bcbfd2eb398a5dbc614e4cb12595d59f750b752586e2f381e1c17e5e5e392f7884b9171f9cf3c753d9256c1621682bac07f119382509a72cc690619681d274",
};

// Golden concept:literal tag CID
const GOLDEN_CONCEPT_LITERAL_TAG_CID =
  "blake3-512:229156b2e5d513191b6a8acb1b26a7b00b11818e79d763403ce955f0e14024f4ca75814957be03a76830bb7fe6e473e5a9d98fd2996670745f3eb1cf3a2a565c";

const EXPECTED_DIMENSIONS = {
  ArithmeticOverflow:      "Ieee754Saturate",
  IntegerDivisionRounding: "FloatDivision",
  NullSemantics:           "ReturnsNanOrInfinity",
  ShiftMode:               "Int32Wrapping",
  BitwiseSemantics:        "Int32",
  SortAdmission:           "JsValueTier",
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

// Structural: compare_to for each dimension value is a proper atomic formula.
// SortAdmission uses admits_sorts (not typescript:X) with non-empty CID args.
test("ts_dimension_value_compare_to_shapes", () => {
  const dvs = dimensionValues();
  for (const dv of dvs) {
    assert.strictEqual(dv.compare_to.kind, "atomic");
    if (dv.dimension_name === "SortAdmission") {
      assert.strictEqual(dv.compare_to.name, "admits_sorts");
      assert.ok(dv.compare_to.args.length > 0, "SortAdmission args must not be empty");
      for (const arg of dv.compare_to.args) {
        assert.strictEqual(arg.kind, "const");
        assert.strictEqual(arg.sort.kind, "primitive");
        assert.strictEqual(arg.sort.name, "cid");
        assert.ok(arg.value.startsWith("blake3-512:"));
      }
    } else {
      assert.ok(dv.compare_to.name.startsWith("typescript:"), `compare_to.name must be 'typescript:...'`);
      assert.deepStrictEqual(dv.compare_to.args, []);
    }
  }
});

// Positive: concept:literal tag has only SortAdmission dimension
test("ts_concept_literal_has_sort_admission_only", () => {
  const decl = declaration();
  const literalTags = decl.tags.filter((t) => t.op_cid === CONCEPT_LITERAL_CID);
  assert.strictEqual(literalTags.length, 1, "expected exactly one concept:literal tag");
  const literalTag = literalTags[0];
  const dimKeys = Object.keys(literalTag.dimensions);
  assert.deepStrictEqual(dimKeys, ["SortAdmission"], "concept:literal must have only SortAdmission");
  assert.strictEqual(
    literalTag.dimensions.SortAdmission,
    GOLDEN_DIM_VALUE_CIDS.SortAdmission,
    "SortAdmission CID must match golden"
  );
  assert.strictEqual(literalTag.cid, GOLDEN_CONCEPT_LITERAL_TAG_CID, "tag CID must match golden");
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

// JCS BigInt conformance: _jcs must emit large integers verbatim without truncation.
// JSON.stringify() throws on BigInt; the old _sortKeys+JSON.stringify path silently
// truncated u64 values above Number.MAX_SAFE_INTEGER (2^53 = 9007199254740992).
test("ts_jcs_bigint_emitted_verbatim", () => {
  // 4614253070214989087 == f64::to_bits(3.14), exceeds Number.MAX_SAFE_INTEGER.
  // JSON.stringify(4614253070214989087) returns "4614253070214989000" (truncated);
  // _jcs must emit all 19 digits.
  const result = _jcs({ __float_bits__: 4614253070214989087n });
  assert.strictEqual(result, '{"__float_bits__":4614253070214989087}',
    "_jcs must not truncate BigInt values above Number.MAX_SAFE_INTEGER");
});

// Discrimination: confirm that JS Number truncation gives a different result,
// proving the test would catch the regression if the BigInt path were removed.
test("ts_jcs_number_truncates_but_bigint_does_not", () => {
  const truncated = _jcs({ __float_bits__: 4614253070214989087 }); // JS Number
  const exact     = _jcs({ __float_bits__: 4614253070214989087n }); // BigInt
  assert.notStrictEqual(truncated, exact,
    "JS Number truncates large u64; BigInt preserves all digits");
});
