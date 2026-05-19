const readline = require("node:readline");

const { emitStub } = require("./realizer");

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
