import readline from "node:readline";

import { liftPath } from "../../../../implementations/typescript/src/lift/index.js";

const expectedAdapter = process.env.BUG_ZOO_TS_ADAPTER ?? "";
const contractName = "LookupRequest";

process.stdout.on("error", (error: NodeJS.ErrnoException) => {
  if (error.code === "EPIPE") {
    process.exit(0);
  }
  throw error;
});

const nullBoundaryIr = [
  {
    kind: "contract",
    symbol: "lookup",
    precondition: {
      kind: "atomic",
      name: "neq",
      args: [
        { kind: "var", name: "name" },
        { kind: "const", value: null, sort: { kind: "primitive", name: "Ref" } },
      ],
    },
  },
];

function hasLookupNameStringBoundary(pre: unknown): boolean {
  const text = JSON.stringify(pre);
  return (
    text.includes('"kind-of"') &&
    text.includes('"field"') &&
    text.includes('"name"') &&
    text.includes('"String"')
  );
}

function liftBoundary(workspaceRoot: string): unknown {
  const report = liftPath(workspaceRoot);
  const decl = report.decls.find(
    (candidate) => candidate.name === contractName && candidate.adapter === expectedAdapter,
  );
  if (!decl) {
    throw new Error(`missing ${expectedAdapter} ${contractName} contract in ${workspaceRoot}`);
  }
  if (!hasLookupNameStringBoundary(decl.pre)) {
    throw new Error(`${expectedAdapter} ${contractName} did not lift a name:string boundary`);
  }
  return {
    kind: "ir-document",
    ir: nullBoundaryIr,
    source: {
      adapter: decl.adapter,
      contract: decl.name,
      sourcePath: decl.sourcePath,
    },
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
