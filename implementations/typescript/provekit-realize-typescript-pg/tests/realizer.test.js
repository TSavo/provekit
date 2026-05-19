const assert = require("node:assert/strict");
const test = require("node:test");

const { emitStub } = require("../src/realizer");
const { dispatch } = require("../src/rpc");

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

test("rpc forwards named term tree sources to pg template substitution", () => {
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
            source: "\"SELECT id FROM users WHERE id = $1\"",
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
  assert.match(response.result.source, /await pool\.query\("SELECT id FROM users WHERE id = \$1", \[id\]\)/);
});
