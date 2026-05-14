const assert = require("node:assert/strict");
const test = require("node:test");

const { emitStub } = require("../src/realizer");

test("sql query uses better-sqlite3 prepare all without await", () => {
  const result = emitStub({
    functionName: "selectRows",
    params: ["sql", "args"],
    paramTypes: ["string", "unknown[]"],
    returnType: "unknown[]",
    conceptName: "concept:sql-query",
  });

  assert.equal(result.is_stub, false);
  assert.equal(result.extension, "ts");
  assert.match(result.source, /db\.prepare\(sql\)\.all\(args\)/);
  assert.doesNotMatch(result.source, /await/);
});

test("sql execute returns better-sqlite3 row count and insert id", () => {
  const result = emitStub({
    functionName: "insertRow",
    params: ["sql", "args"],
    paramTypes: ["string", "unknown[]"],
    returnType: "SqlExecuteResult",
    conceptName: "concept:sql-execute",
  });

  assert.equal(result.is_stub, false);
  assert.match(result.source, /lastInsertRowid/);
  assert.match(result.source, /rows_affected/);
});
