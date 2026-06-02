import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const KIT_DECLARATION_RPC_METHOD = "provekit.plugin.kit_declaration";
const TYPESCRIPT_SOURCE_DIR = join(process.cwd(), "implementations/typescript");
const RPC_TEST_TIMEOUT_MS = 30_000;

function runRpc(requests: Array<Record<string, unknown>>): Array<Record<string, any>> {
  const input = `${requests.map((request) => JSON.stringify(request)).join("\n")}\n`;
  const completed = spawnSync("npx", ["tsx", "src/lift/typescript-source/bin.ts", "--rpc"], {
    cwd: TYPESCRIPT_SOURCE_DIR,
    input,
    encoding: "utf8",
  });

  expect(completed.status, completed.stderr).toBe(0);
  return completed.stdout
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as Record<string, any>);
}

describe("typescript-source kit_declaration RPC", () => {
  it("returns the empirically declared TypeScript source surface", () => {
    const responses = runRpc([
      { jsonrpc: "2.0", id: 1, method: "initialize" },
      { jsonrpc: "2.0", id: 2, method: KIT_DECLARATION_RPC_METHOD },
      { jsonrpc: "2.0", id: 3, method: "shutdown" },
    ]);

    const declaration = responses.find((response) => response.id === 2);
    expect(declaration).toBeDefined();
    expect(declaration).not.toHaveProperty("error");
    expect(declaration!.result).toEqual({
      kit: {
        id: "typescript-source",
        language: "typescript",
        version: "0.1.0-draft",
      },
      rpc: {
        methods: [
          { name: "initialize", required: true },
          { name: KIT_DECLARATION_RPC_METHOD, required: true },
          { name: "lift", required: true },
          { name: "compile", required: false },
          { name: "provekit.plugin.recognize", required: false },
          { name: "shutdown", required: false },
        ],
      },
      proofResolution: { strategy: "npm" },
      effectKinds: ["concept:panic-freedom"],
      effectLeaves: [
        {
          surface: "typescript-source",
          local: "ts:throw",
          concept: "concept:panic-freedom.leaf.runtime-failure-site",
        },
      ],
      guardPredicates: [],
      controlCarriers: [],
      residueCategories: [],
    });
  }, RPC_TEST_TIMEOUT_MS);

  it("returns a deterministic kit_declaration response", () => {
    const [first, second] = runRpc([
      { jsonrpc: "2.0", id: 7, method: KIT_DECLARATION_RPC_METHOD },
      { jsonrpc: "2.0", id: 8, method: KIT_DECLARATION_RPC_METHOD },
    ]);

    expect(first).not.toHaveProperty("error");
    expect(second).not.toHaveProperty("error");
    expect(first.result).toEqual(second.result);
  }, RPC_TEST_TIMEOUT_MS);

  it("keeps initialize separate from kit_declaration content", () => {
    const initialize = runRpc([{ jsonrpc: "2.0", id: 1, method: "initialize" }])[0];

    expect(initialize).not.toHaveProperty("error");
    expect(initialize.result.name).toBe("provekit-lift-typescript-source");
    expect(initialize.result).not.toHaveProperty("effectKinds");
    expect(initialize.result).not.toHaveProperty("effectLeaves");
    expect(initialize.result).not.toHaveProperty("kit");
  }, RPC_TEST_TIMEOUT_MS);
});
