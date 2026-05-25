const assert = require("node:assert/strict");
const test = require("node:test");

const { emitStub, getProofPath, shimProofEntries } = require("../src/realizer");
const { dispatch } = require("../src/rpc");

// Verify the realizer resolves from node_modules, not the central JSON registry
// (the flat typescript-canonical-bodies-pg.json was deleted in #1468).
test("pg_realizer_resolves_from_shim_proof_in_node_modules", () => {
  const proofPath = getProofPath();
  assert.ok(proofPath !== null, "proof path must resolve");
  assert.ok(proofPath.includes("node_modules"), "proof path must be under node_modules");
  assert.ok(proofPath.includes("provekit-shim-pg"), "proof path must reference shim package");
  const entries = shimProofEntries();
  assert.ok(entries.length > 0, "must have at least one entry");
});

// sql-query-all migrate shape: 2-param (sql, args) free-client binding → rows
test("sql query-all uses pg client query with await over migrate shape", () => {
  const result = emitStub({
    functionName: "selectRows",
    params: ["sql", "args"],
    paramTypes: ["string", "unknown[]"],
    returnType: "unknown[]",
    conceptName: "concept:sql-query-all",
  });

  assert.equal(result.is_stub, false);
  assert.equal(result.extension, "ts");
  assert.match(result.source, /await pool\.query\(sql, args\)/);
  assert.match(result.source, /return result\.rows/);
});

// sql-query-row migrate shape: 2-param (sql, args) → rows[0]
test("sql query-row returns single row from pg client query", () => {
  const result = emitStub({
    functionName: "selectRow",
    params: ["sql", "args"],
    paramTypes: ["string", "unknown[]"],
    returnType: "unknown",
    conceptName: "concept:sql-query-row",
  });

  assert.equal(result.is_stub, false);
  assert.match(result.source, /await pool\.query\(sql, args\)/);
  assert.match(result.source, /return result\.rows\[0\]/);
});

// sql-execute migrate shape: 2-param (sql, args) → RETURNING id + rows_affected
test("sql execute appends returning id for pg insert id substitution", () => {
  const result = emitStub({
    functionName: "insertRow",
    params: ["sql", "args"],
    paramTypes: ["string", "unknown[]"],
    returnType: "{ rows_affected: number; last_insert_id: unknown }",
    conceptName: "concept:sql-execute",
  });

  assert.equal(result.is_stub, false);
  assert.match(result.source, /sql \+ " RETURNING id"/);
  assert.match(result.source, /last_insert_id/);
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
  assert.ok(resp.result.entries.length > 0, `expected >0 entries, got ${resp.result.entries.length}`);
  const first = resp.result.entries[0];
  assert.ok(typeof first.concept_name === "string", "entry must have concept_name");
  assert.ok(first.emission_template && first.emission_template.kind === "verbatim", "entry must have verbatim emission_template");
  assert.ok(first.signature_guard, "entry must have signature_guard");
  assert.ok(typeof resp.result.proof_path === "string", "proof_path must be a string");
  assert.ok(resp.result.proof_path.includes("node_modules"), "proof_path must be in node_modules");
});

// NTT source substitution: 2-param NTT picks the migrate sql-query-all binding
test("rpc forwards named term tree sources to pg template substitution", () => {
  const response = dispatch({
    id: 1,
    method: "provekit.plugin.invoke",
    params: {
      function: "getUserById",
      params: ["id"],
      param_types: ["number"],
      return_type: "User",
      concept_name: "concept:sql-query-all",
      namedTermTree: {
        conceptName: "concept:sql-query-all",
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
