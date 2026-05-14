const assert = require("node:assert/strict");
const test = require("node:test");

const { emitStub } = require("../src/realizer");

test("http request uses fetch and emits an async TypeScript function", () => {
  const result = emitStub({
    functionName: "fetchStatus",
    params: ["url"],
    paramTypes: ["string"],
    returnType: "number",
    conceptName: "concept:http-request",
  });

  assert.equal(result.is_stub, false);
  assert.equal(result.extension, "ts");
  assert.equal(
    result.source,
    "async function fetchStatus(url) {\n  const response = await fetch(url);\n  return response.status;\n}\n",
  );
});

test("contract observation witness body template emits provekit witness call", () => {
  const result = emitStub({
    functionName: "observeContract",
    params: ["callsiteCid", "contractCid", "mode"],
    paramTypes: ["string", "string", "string"],
    returnType: "ContractObservationResult",
    conceptName: "concept:contract-observation",
    mode: "witness",
  });

  assert.equal(result.is_stub, false);
  assert.match(result.source, /provekit_witness\.observe/);
  assert.match(result.source, /callsiteCid/);
  assert.match(result.source, /contractCid/);
  assert.match(result.source, /mode/);
});

test("contract observation gate mode does not render witness body template", () => {
  const result = emitStub({
    functionName: "observeContract",
    params: ["callsiteCid", "contractCid", "mode"],
    paramTypes: ["string", "string", "string"],
    returnType: "ContractObservationResult",
    conceptName: "concept:contract-observation",
    mode: "gate",
  });

  assert.equal(result.is_stub, true);
  assert.doesNotMatch(result.source, /provekit_witness\.observe/);
});

test("unknown concept falls back to a TypeScript stub", () => {
  const result = emitStub({
    functionName: "missing",
    params: ["value"],
    paramTypes: ["number"],
    returnType: "number",
    conceptName: "concept:missing",
  });

  assert.equal(result.is_stub, true);
  assert.equal(
    result.source,
    "function missing(value) {\n  throw new Error(\"provekit-bind canonical: concept:missing\");\n}\n",
  );
});
