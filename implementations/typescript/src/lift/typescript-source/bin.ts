#!/usr/bin/env node
import readline from "node:readline";

import {
  compileTypeScriptSourceIr,
  liftTypeScriptLibraryBindingsPaths,
  liftTypeScriptSourcePaths,
  recognizeTypeScriptSources,
  type FunctionContractMemento,
} from "./index.js";
import { normalizeTypeScriptSourceVerifyDocument } from "./verify.js";

const DIALECT = "typescript-source";
const SURFACE_ALIASES = new Set([DIALECT, "typescript-bind"]);
const VERSION = "0.1.0-draft";

interface JsonRpcRequest {
  jsonrpc?: string;
  id?: unknown;
  method?: string;
  params?: Record<string, unknown>;
}

export function main(argv: string[] = process.argv.slice(2)): void {
  if (!argv.includes("--rpc")) {
    process.stderr.write("usage: provekit-lift-typescript-source --rpc\n");
    process.exit(1);
  }
  runRpcMode();
}

function runRpcMode(): void {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: false });
  rl.on("line", (line) => {
    if (line.trim() === "") return;
    let request: JsonRpcRequest;
    try {
      request = JSON.parse(line) as JsonRpcRequest;
    } catch (error) {
      write(errorResponse(null, -32700, `PARSE_ERROR: ${(error as Error).message}`));
      return;
    }
    try {
      const response = dispatch(request);
      if (response) write(response);
    } catch (error) {
      write(errorResponse(request.id ?? null, -32603, (error as Error).message));
    }
  });
}

function dispatch(request: JsonRpcRequest): Record<string, unknown> | null {
  switch (request.method) {
    case "initialize":
      return success(request.id, {
        name: "provekit-lift-typescript-source",
        version: VERSION,
        protocol_version: "pep/1.7.0",
        capabilities: {
          authoring_surfaces: [...SURFACE_ALIASES],
          ir_version: "v1.1.0",
          emits_signed_mementos: false,
        },
      });
    case "lift":
      return liftRpc(request);
    case "compile":
      return compileRpc(request);
    case "provekit.plugin.recognize":
      return recognizeRpc(request);
    case "shutdown":
      return success(request.id, null);
    default:
      return errorResponse(request.id ?? null, -32601, `METHOD_NOT_FOUND: ${request.method ?? ""}`);
  }
}

function liftRpc(request: JsonRpcRequest): Record<string, unknown> {
  const params = request.params ?? {};
  const surface = typeof params.surface === "string" ? params.surface : DIALECT;
  if (!SURFACE_ALIASES.has(surface)) {
    return errorResponse(request.id ?? null, 1003, `SURFACE_NOT_SUPPORTED: ${surface}`);
  }
  const sourcePaths = Array.isArray(params.source_paths)
    ? params.source_paths.filter((path): path is string => typeof path === "string")
    : [];
  if (sourcePaths.length === 0) {
    return errorResponse(request.id ?? null, -32602, "source_paths must be a non-empty array of strings");
  }
  const workspaceRoot = typeof params.workspace_root === "string" ? params.workspace_root : ".";
  const options = params.options && typeof params.options === "object" ? (params.options as Record<string, unknown>) : {};
  const layer = typeof options.layer === "string" ? options.layer : "all";
  if (layer === "library-bindings") {
    const result = liftTypeScriptLibraryBindingsPaths(workspaceRoot, sourcePaths);
    return success(request.id, {
      kind: "ir-document",
      ir: [...result.libraryBindings, ...result.libraryRefusals],
      callEdges: [],
      diagnostics: result.diagnostics,
      opacityReport: result.opacityReport,
      refusals: result.refusals,
    });
  }
  const result = liftTypeScriptSourcePaths(workspaceRoot, sourcePaths);
  if (layer === "verify") {
    return success(request.id, normalizeTypeScriptSourceVerifyDocument(result));
  }
  return success(request.id, {
    kind: "ir-document",
    ir: result.declarations,
    callEdges: [],
    diagnostics: result.diagnostics,
    opacityReport: result.opacityReport,
    refusals: result.refusals,
  });
}

function compileRpc(request: JsonRpcRequest): Record<string, unknown> {
  const params = request.params ?? {};
  const ir = params.ir;
  if (!Array.isArray(ir)) {
    return errorResponse(request.id ?? null, -32602, "ir must be an array of function-contract mementos");
  }
  const body = compileTypeScriptSourceIr(ir as FunctionContractMemento[]);
  return success(request.id, { kind: "compiled-formula", body });
}

function recognizeRpc(request: JsonRpcRequest): Record<string, unknown> {
  const params = request.params ?? {};
  const projectRoot = typeof params.project_root === "string" ? params.project_root : "";
  const sourcePaths = Array.isArray(params.source_paths)
    ? params.source_paths.filter((path): path is string => typeof path === "string")
    : [];
  try {
    return success(request.id, recognizeTypeScriptSources({
      project_root: projectRoot,
      source_paths: sourcePaths,
    }));
  } catch (error) {
    return errorResponse(request.id ?? null, -32602, (error as Error).message);
  }
}

function success(id: unknown, result: unknown): Record<string, unknown> {
  return { jsonrpc: "2.0", id: id ?? null, result };
}

function errorResponse(id: unknown, code: number, message: string): Record<string, unknown> {
  return { jsonrpc: "2.0", id: id ?? null, error: { code, message } };
}

function write(value: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

main();
