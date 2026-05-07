import readline from "node:readline";

import { discoverBoundary, nullBoundaryIr } from "./ts-boundary-discovery.js";

const expectedAdapter = process.env.BUG_ZOO_TS_ADAPTER ?? "";

process.stdout.on("error", (error: NodeJS.ErrnoException) => {
  if (error.code === "EPIPE") {
    process.exit(0);
  }
  throw error;
});

function liftBoundary(workspaceRoot: string): unknown {
  const discovery = discoverBoundary(expectedAdapter, workspaceRoot);
  return {
    kind: "ir-document",
    ir: nullBoundaryIr,
    source: discovery,
  };
}

const rl = readline.createInterface({ input: process.stdin, output: process.stdout });

rl.on("line", (line: string) => {
  let id: unknown = null;
  try {
    const request = JSON.parse(line);
    id = request.id;
    if (request.method === "initialize") {
      process.stdout.write(
        JSON.stringify({
          jsonrpc: "2.0",
          id,
          result: {
            name: "bug-zoo-typescript-boundary-lifter",
            version: "0",
            capabilities: ["bug-zoo-boundary"],
          },
        }) + "\n",
      );
      return;
    }
    if (request.method === "lift") {
      const workspaceRoot = request.params?.workspace_root;
      if (typeof workspaceRoot !== "string") {
        throw new Error("lift params.workspace_root must be a string");
      }
      process.stdout.write(
        JSON.stringify({
          jsonrpc: "2.0",
          id,
          result: liftBoundary(workspaceRoot),
        }) + "\n",
      );
      return;
    }
    if (request.method === "shutdown") {
      rl.close();
      return;
    }
    throw new Error(`unsupported method ${request.method}`);
  } catch (error) {
    process.stdout.write(
      JSON.stringify({
        jsonrpc: "2.0",
        id,
        error: { code: -32000, message: error instanceof Error ? error.message : String(error) },
      }) + "\n",
    );
  }
});
