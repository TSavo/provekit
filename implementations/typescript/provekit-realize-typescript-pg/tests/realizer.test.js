const assert = require("node:assert/strict");
const test = require("node:test");

const { emitStub } = require("../src/realizer");

test("sql query uses pg pool query with await", () => {
  const result = emitStub({
    functionName: "selectRows",
    params: ["sql", "args"],
    paramTypes: ["string", "unknown[]"],
    returnType: "unknown[]",
    conceptName: "concept:sql-query",
  });

  assert.equal(result.is_stub, false);
  assert.equal(result.extension, "ts");
  assert.match(result.source, /await pool\.query\(sql, args\)/);
  assert.match(result.source, /return result\.rows/);
});

test("sql execute appends returning id for pg insert id substitution", () => {
  const result = emitStub({
    functionName: "insertRow",
    params: ["sql", "args"],
    paramTypes: ["string", "unknown[]"],
    returnType: "SqlExecuteResult",
    conceptName: "concept:sql-execute",
  });

  assert.equal(result.is_stub, false);
  assert.match(result.source, /sql \+ " RETURNING id"/);
  assert.match(result.source, /last_insert_id/);
});
