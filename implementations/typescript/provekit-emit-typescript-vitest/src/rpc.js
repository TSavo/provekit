"use strict";

const path = require("node:path");
const readline = require("node:readline");
const { spawnSync } = require("node:child_process");

const { emit } = require("./emitter");

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
    if (!isObject(params)) return errorResponse(msgId, -32602, "INVALID_PARAMS: params must be an object");
    return { jsonrpc: "2.0", id: msgId, result: emit(params) };
  }
  if (method === "provekit.plugin.check") {
    if (!isObject(params)) return errorResponse(msgId, -32602, "INVALID_PARAMS: params must be an object");
    const outDir = stringField(params, "out_dir") || stringField(params, "outDir");
    const artifactPath = stringField(params, "artifact_path") || stringField(params, "artifactPath");
    if (outDir === "") return errorResponse(msgId, -32602, "INVALID_PARAMS: missing out_dir");
    if (artifactPath === "") return errorResponse(msgId, -32602, "INVALID_PARAMS: missing artifact_path");
    return { jsonrpc: "2.0", id: msgId, result: checkVitest(outDir, artifactPath) };
  }
  if (method === "provekit.plugin.shutdown") {
    return { jsonrpc: "2.0", id: msgId, result: null };
  }
  return errorResponse(msgId, -32601, `METHOD_NOT_FOUND: ${method}`);
}

function checkVitest(outDir, artifactPath) {
  const vitestBin = path.join(__dirname, "..", "node_modules", "vitest", "vitest.mjs");
  const target = path.isAbsolute(artifactPath) ? artifactPath : path.resolve(outDir, artifactPath);
  const filter = path.relative(outDir, target);
  const completed = spawnSync(process.execPath, [vitestBin, "run", "--globals", filter], {
    cwd: outDir,
    encoding: "utf8",
  });
  return {
    ok: completed.status === 0,
    command: `${process.execPath} ${vitestBin} run --globals ${filter}`,
    cwd: outDir,
    stdout: completed.stdout || "",
    stderr: completed.stderr || "",
    exitCode: completed.status === null ? 1 : completed.status,
  };
}

function send(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function errorResponse(id, code, message) {
  return { jsonrpc: "2.0", id, error: { code, message } };
}

function stringField(value, field) {
  return typeof value[field] === "string" ? value[field] : "";
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

module.exports = {
  checkVitest,
  dispatch,
  runRpc,
};
