const assert = require("node:assert/strict");
const test = require("node:test");

const { emitStub } = require("../src/realizer");
const { dispatch } = require("../src/rpc");

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

test("rpc forwards named term tree sources to better-sqlite3 template substitution", () => {
  const response = dispatch({
    id: 1,
    method: "provekit.plugin.invoke",
    params: {
      function: "getUserById",
      params: ["id"],
      param_types: ["number"],
      return_type: "User",
      concept_name: "concept:sql-query",
      namedTermTree: {
        conceptName: "concept:sql-query",
        operationKind: "op-application",
        args: [
          {
            args: [],
            conceptName: "Sql",
            operationKind: "const",
            sort: "Sql",
            source: "\"SELECT id FROM users WHERE id = ?\"",
          },
          {
            args: [],
            conceptName: "SqlArgs",
            operationKind: "args",
            sort: "SqlArgs",
            source: "[id]",
          },
        ],
      },
    },
  });

  assert.equal(response.result.is_stub, false);
  assert.match(response.result.source, /db\.prepare\("SELECT id FROM users WHERE id = \?"\)\.all\(\[id\]\)/);
});
