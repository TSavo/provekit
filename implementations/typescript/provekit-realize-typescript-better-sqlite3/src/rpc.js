const readline = require("node:readline");

const realizer = require("./realizer");
const { emitStub } = realizer;
const { declaration: platformSemanticsDeclaration } = require("./platform_semantics");

function runRpc() {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: false });
  rl.on("line", (line) => {
    if (line.trim() === "") return;
    let method = "";
    try {
      const request = JSON.parse(line);
      method = String(request.method ?? "");
      send(dispatch(request));
      if (method === "provekit.plugin.shutdown") rl.close();
    } catch (error) {
      send(errorResponse(null, -32700, `PARSE_ERROR: ${error.message}`));
    }
  });
}

function dispatch(request) {
  const msgId = request.id ?? null;
  const method = String(request.method ?? "");
  const params = request.params ?? {};
  if (method === "provekit.plugin.invoke") {
    if (params === null || typeof params !== "object" || Array.isArray(params)) {
      return errorResponse(msgId, -32602, "INVALID_PARAMS: params must be an object");
    }
    return {
      jsonrpc: "2.0",
      id: msgId,
      result: emitStub({
        functionName: String(params.function ?? ""),
        params: stringList(params.params),
        paramTypes: stringList(params.param_types),
        returnType: String(params.return_type ?? ""),
        conceptName: String(params.concept_name ?? ""),
        mode: typeof params.mode === "string" ? params.mode : undefined,
        namedTermTree: params.namedTermTree ?? params.named_term_tree,
      }),
    };
  }
  if (method === "provekit.plugin.platform_semantics") {
    const decl = platformSemanticsDeclaration();
    return { jsonrpc: "2.0", id: msgId, result: { tags: decl.tags, dimension_values: decl.dimension_values, op_aliases: {} } };
  }
  if (method === "provekit.plugin.body_template_entries") {
    // The kit resolved the shim .proof from node_modules and built the template
    // entries. It hands them back so the substrate can content-address them with
    // the canonical sorted-JCS scheme (the same cid_for_json algebra every
    // language uses). The kit does NOT compute the CID itself — the address
    // space is universal and owned by the substrate.
    const proofPath = realizer.getProofPath ? realizer.getProofPath() : null;
    if (!proofPath) {
      return errorResponse(msgId, 1404, "SHIM_NOT_FOUND: provekit-shim-better-sqlite3 not installed in node_modules");
    }
    const entries = realizer.shimProofEntries ? realizer.shimProofEntries() : [];
    return { jsonrpc: "2.0", id: msgId, result: { entries, proof_path: proofPath } };
  }
  if (method === "provekit.plugin.resolve_dependency_proofs") {
    console.error("provekit-realize-typescript-better-sqlite3: resolve_dependency_proofs not yet implemented for typescript; returning empty proof_paths");
    return { jsonrpc: "2.0", id: msgId, result: { proof_paths: [] } };
  }
  if (method === "provekit.plugin.shutdown") {
    return { jsonrpc: "2.0", id: msgId, result: null };
  }
  return errorResponse(msgId, -32601, `METHOD_NOT_FOUND: ${method}`);
}

function stringList(value) {
  if (!Array.isArray(value)) return [];
  return value.map((item) => String(item));
}

function send(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function errorResponse(id, code, message) {
  return { jsonrpc: "2.0", id, error: { code, message } };
}

module.exports = {
  dispatch,
  runRpc,
};
