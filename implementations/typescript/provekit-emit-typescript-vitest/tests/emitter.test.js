"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

const { emit } = require("../src/emitter");

test("emit renders a passing Vitest module from neutral predicates", () => {
  const result = emit({
    contract_id: "concept:eq",
    function: "identity",
    predicates: [
      {
        kind: "op",
        name: "concept:eq",
        args: [
          { kind: "const", value: 2 },
          { kind: "const", value: 2 },
        ],
      },
      {
        kind: "op",
        name: "concept:lt",
        args: [
          { kind: "var", name: "lo" },
          { kind: "var", name: "hi" },
        ],
      },
    ],
  });

  assert.equal(result.kind, "typescript-vitest-test-emission");
  assert.equal(result.path, "provekit_identity.test.ts");
  assert.equal(result.extension, "ts");
  assert.match(result.emitted_artifact_cid, /^blake3-512:[0-9a-f]{128}$/);
  assert.deepEqual(result.emitted_predicates, ["eq", "lt"]);
  assert.deepEqual(result.unsupported_predicates, []);
  assert.equal(result.is_complete, true);
  assert.match(result.source, /describe\("provekit contract identity"/);
  assert.match(result.source, /expect\(2\)\.toEqual\(2\);/);
  assert.match(result.source, /const lo = 0;/);
  assert.match(result.source, /const hi = 1;/);
  assert.match(result.source, /expect\(lo < hi\)\.toBe\(true\);/);
});

test("emit reports unsupported predicates instead of emitting vacuous tests", () => {
  const result = emit({
    function: "unsupported",
    predicates: [{ kind: "op", name: "concept:unknown", args: [] }],
  });

  assert.equal(result.is_complete, false);
  assert.deepEqual(result.emitted_predicates, []);
  assert.deepEqual(result.unsupported_predicates, ["concept:unknown"]);
});
