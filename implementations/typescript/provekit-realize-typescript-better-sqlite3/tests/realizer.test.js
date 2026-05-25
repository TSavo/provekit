const assert = require("node:assert/strict");
const test = require("node:test");

const { emitStub, getProofPath, shimProofEntries } = require("../src/realizer");
const { dispatch } = require("../src/rpc");

// Verify the realizer resolves from node_modules, not the central JSON registry.
test("bsq_realizer_resolves_from_shim_proof_in_node_modules", () => {
  const proofPath = getProofPath();
  assert.ok(proofPath !== null, "proof path must resolve");
  assert.ok(proofPath.includes("node_modules"), "proof path must be under node_modules");
  assert.ok(proofPath.includes("provekit-shim-better-sqlite3"), "proof path must reference shim package");
  const entries = shimProofEntries();
  assert.ok(entries.length > 0, "must have at least one entry");
  assert.ok(entries.length >= 40, `expected >=40 shim entries, got ${entries.length}`);
});

// sql-query: 3-param (db, sql, args) → prepare+all path
test("sql query uses better-sqlite3 prepare all without await", () => {
  const result = emitStub({
    functionName: "selectRows",
    params: ["db", "sql", "args"],
    paramTypes: ["Database.Database", "string", "unknown[]"],
    returnType: "unknown[]",
    conceptName: "concept:sql-query",
  });

  assert.equal(result.is_stub, false);
  assert.equal(result.extension, "ts");
  assert.match(result.source, /db\.prepare\(sql\)/);
  assert.doesNotMatch(result.source, /await/);
});

// sql-execute: 3-param (db, sql, args) → prepare+run path
test("sql execute produces prepare run body from shim", () => {
  const result = emitStub({
    functionName: "insertRow",
    params: ["db", "sql", "args"],
    paramTypes: ["Database.Database", "string", "unknown[]"],
    returnType: "Database.RunResult",
    conceptName: "concept:sql-execute",
  });

  assert.equal(result.is_stub, false);
  assert.match(result.source, /db\.prepare\(sql\)/);
  assert.match(result.source, /\.run\(args\)/);
});

// body_template_entries RPC returns the kit-built template entries (the
// substrate content-addresses these with the universal sorted-JCS scheme).
test("rpc body_template_entries returns shim-resolved entries", () => {
  const resp = dispatch({
    id: 1,
    method: "provekit.plugin.body_template_entries",
    params: {},
  });
  assert.ok(!resp.error, `unexpected error: ${JSON.stringify(resp.error)}`);
  assert.ok(Array.isArray(resp.result.entries), "entries must be an array");
  assert.ok(resp.result.entries.length >= 40, `expected >=40 entries, got ${resp.result.entries.length}`);
  const first = resp.result.entries[0];
  assert.ok(typeof first.concept_name === "string", "entry must have concept_name");
  assert.ok(first.emission_template && first.emission_template.kind === "verbatim", "entry must have verbatim emission_template");
  assert.ok(first.signature_guard, "entry must have signature_guard");
  assert.ok(typeof resp.result.proof_path === "string", "proof_path must be a string");
  assert.ok(resp.result.proof_path.includes("node_modules"), "proof_path must be in node_modules");
});

// NTT source substitution: 2-param NTT picks stmt-based binding from shim
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
  // Shim arity-2 sql-query bindings use a prepared Statement as first param.
  // The NTT source for the sql arg is inlined as the statement.
  assert.match(response.result.source, /"SELECT id FROM users WHERE id = \?"/);
});

