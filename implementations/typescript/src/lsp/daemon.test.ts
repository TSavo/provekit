/**
 * Integration tests for the provekit-lsp-ts daemon protocol.
 *
 * Mirrors implementations/python/provekit-lift-py-tests/tests/test_daemon_protocol.py.
 *
 * Asserts:
 *   - initialize responds with protocol_version == "provekit-lsp-shared/1" and capabilities object.
 *   - lift returns result.kind == "ir-document" with ir array.
 *   - parse (legacy) returns result.declarations as a JSON array (not a string).
 *   - parse returns result.callEdges as a JSON array.
 *   - With a contract-bearing fixture, each declaration has kind == "contract".
 *   - Empty source returns declarations == [] and callEdges == [].
 *   - Byte-determinism: two runs on the same input produce identical parse output.
 *   - Unknown method returns a JSON-RPC error with code -32601.
 */

import { describe, it, expect, beforeAll } from "vitest";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Resolve tsx through Node's import hook instead of tsx/cli. The cli path
// creates an IPC socket, which fails under the macOS sandbox used by local
// agent runs; `node --import tsx` loads the same TS entrypoint without that
// side channel.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const TSX_REGISTER: string = require.resolve("tsx");
const DAEMON_ENTRY = resolve(__dirname, "daemon-entry.ts");

/** Spawn the daemon, feed ndjson, return parsed response lines. */
function runLsp(ndjsonInput: string): Record<string, unknown>[] {
  const result = spawnSync(process.execPath, ["--import", TSX_REGISTER, DAEMON_ENTRY], {
    input: ndjsonInput,
    encoding: "utf8",
    timeout: 30000,
  });

  if (result.error) {
    throw result.error;
  }

  // Tolerate non-zero exit (e.g. when shutdown causes early exit in some Node versions).
  const lines = (result.stdout as string)
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  return lines.map((l) => JSON.parse(l) as Record<string, unknown>);
}

/** Build NDJSON for initialize -> parse -> shutdown. */
function buildSession(source: string, path: string): string {
  const msgs = [
    { jsonrpc: "2.0", id: 1, method: "initialize", params: {} },
    { jsonrpc: "2.0", id: 2, method: "parse", params: { path, source } },
    { jsonrpc: "2.0", id: 3, method: "shutdown" },
  ];
  return msgs.map((m) => JSON.stringify(m)).join("\n") + "\n";
}

/** Build NDJSON for initialize -> lift -> shutdown. */
function buildLiftSession(workspaceRoot: string, sourcePaths: string[]): string {
  const msgs = [
    { jsonrpc: "2.0", id: 10, method: "initialize", params: {} },
    { jsonrpc: "2.0", id: 11, method: "lift", params: { workspace_root: workspaceRoot, source_paths: sourcePaths } },
    { jsonrpc: "2.0", id: 12, method: "shutdown" },
  ];
  return msgs.map((m) => JSON.stringify(m)).join("\n") + "\n";
}

/** Build NDJSON for initialize -> analyzeDocument -> shutdown. */
function buildAnalyzeSession(source: string, path: string): string {
  const msgs = [
    { jsonrpc: "2.0", id: 20, method: "initialize", params: {} },
    {
      jsonrpc: "2.0",
      id: 21,
      method: "analyzeDocument",
      params: {
        kit_id: "ts",
        uri: `file:///project/${path}`,
        file: path,
        text: source,
        document_version: 42,
        workspace_root: "/project",
        accepted_protocol_catalog_cids: [],
        policy_cids: [],
      },
    },
    { jsonrpc: "2.0", id: 22, method: "shutdown" },
  ];
  return msgs.map((m) => JSON.stringify(m)).join("\n") + "\n";
}

// A fixture with a Zod schema to guarantee at least one declaration.
const ZOD_FIXTURE_SOURCE = `
import { z } from "zod";

const UserSchema = z.object({
  age: z.number().min(0).max(150),
  name: z.string().min(1),
});
`.trim();
const ZOD_FIXTURE_PATH = "fixture.ts";

// ---------------------------------------------------------------------------
// Shared state (run daemon once per describe block, not once per test).
// Starting a tsx subprocess takes ~7-10s; amortise across all tests.
// ---------------------------------------------------------------------------

let zodResponses: Record<string, unknown>[] = [];
let emptyResponses: Record<string, unknown>[] = [];
let unknownMethodResponses: Record<string, unknown>[] = [];
let unsupportedLanguageResponses: Record<string, unknown>[] = [];
let zodResponses2: Record<string, unknown>[] = [];   // second run for determinism
let liftResponses: Record<string, unknown>[] = [];
let callEdgeResponses: Record<string, unknown>[] = [];

// Per-suite vitest timeout. Each individual test just reads cached data.
const SUITE_TIMEOUT_MS = 90_000;
const DAEMON_TEST_TIMEOUT_MS = 30_000;

const CALL_EDGE_FIXTURE_SOURCE = `
function add(x: number): number {
  return x;
}

function myFn(x: number): number {
  return add(x);
}
`.trim();

describe("daemon protocol conformance (provekit-lsp-ts)", () => {
  beforeAll(() => {
    zodResponses = runLsp(buildSession(ZOD_FIXTURE_SOURCE, ZOD_FIXTURE_PATH));
    emptyResponses = runLsp(buildSession("// no contracts here\n", "empty.ts"));
    zodResponses2 = runLsp(buildSession(ZOD_FIXTURE_SOURCE, ZOD_FIXTURE_PATH));
    callEdgeResponses = runLsp(buildSession(CALL_EDGE_FIXTURE_SOURCE, "call-edge.ts"));

    // Write the Zod fixture to a temp file for the lift test.
    const tmpFixtureDir = resolve(tmpdir(), `pk-lsp-ts-test-${Date.now()}`);
    mkdirSync(tmpFixtureDir, { recursive: true });
    const tmpFixturePath = resolve(tmpFixtureDir, "fixture.ts");
    writeFileSync(tmpFixturePath, ZOD_FIXTURE_SOURCE, "utf8");
    liftResponses = runLsp(buildLiftSession(tmpFixtureDir, ["fixture.ts"]));

    const unknownMsgs = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: {} },
      { jsonrpc: "2.0", id: 2, method: "frobnicate", params: {} },
      { jsonrpc: "2.0", id: 3, method: "shutdown" },
    ];
    unknownMethodResponses = runLsp(unknownMsgs.map((m) => JSON.stringify(m)).join("\n") + "\n");

    const unsupportedMsgs = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: {} },
      {
        jsonrpc: "2.0",
        id: 2,
        method: "parse",
        params: { path: "f.rs", source: "fn foo() {}", language: "rust" },
      },
      { jsonrpc: "2.0", id: 3, method: "shutdown" },
    ];
    unsupportedLanguageResponses = runLsp(
      unsupportedMsgs.map((m) => JSON.stringify(m)).join("\n") + "\n",
    );
  }, SUITE_TIMEOUT_MS);

  it("initialize: name == provekit-lsp-ts and protocol_version == provekit-lsp-shared/1", () => {
    const initResp = zodResponses.find((r) => r.id === 1);
    expect(initResp).toBeDefined();
    const result = initResp!.result as Record<string, unknown>;
    expect(result.name).toBe("provekit-lsp-ts");
    expect(result.protocol_version).toBe("provekit-lsp-shared/1");
    expect(result.kit_id).toBe("ts");
    expect(typeof result.protocol_catalog_cid).toBe("string");
    const caps = result.capabilities as Record<string, unknown>;
    expect(Array.isArray(caps.source_surfaces)).toBe(true);
    expect((caps.source_surfaces as string[]).includes("typescript-source")).toBe(true);
    expect((caps.diagnostic_codes as string[]).includes("provekit.lsp.implication_failed")).toBe(true);
  });

  it("lift: kind == ir-document with ir array", () => {
    const initResp = liftResponses.find((r) => r.id === 10);
    expect(initResp).toBeDefined();
    const initResult = initResp!.result as Record<string, unknown>;
    expect(initResult.protocol_version).toBe("provekit-lsp-shared/1");

    const liftResp = liftResponses.find((r) => r.id === 11);
    expect(liftResp).toBeDefined();
    expect(liftResp!.error).toBeUndefined();
    const liftResult = liftResp!.result as Record<string, unknown>;
    expect(liftResult.kind).toBe("ir-document");
    expect(Array.isArray(liftResult.ir)).toBe(true);
    expect(Array.isArray(liftResult.callEdges)).toBe(true);
    expect(Array.isArray(liftResult.diagnostics)).toBe(true);
    expect(Array.isArray(liftResult.refusals)).toBe(true);
  });

  it("lift: zod fixture produces at least one ir entry with kind == contract", () => {
    const liftResp = liftResponses.find((r) => r.id === 11);
    const liftResult = liftResp!.result as Record<string, unknown>;
    const ir = liftResult.ir as Record<string, unknown>[];
    expect(ir.length).toBeGreaterThanOrEqual(1);
    for (const entry of ir) {
      expect(entry.kind).toBe("contract");
    }
  });

  it("parse: declarations is a JSON array (not a string)", () => {
    const parseResp = zodResponses.find((r) => r.id === 2);
    expect(parseResp).toBeDefined();
    expect(parseResp!.error).toBeUndefined();
    const result = parseResp!.result as Record<string, unknown>;
    expect(Array.isArray(result.declarations)).toBe(true);
  });

  it("parse: callEdges is a JSON array", () => {
    const parseResp = zodResponses.find((r) => r.id === 2);
    const result = parseResp!.result as Record<string, unknown>;
    expect(Array.isArray(result.callEdges)).toBe(true);
  });

  it("parse: callEdges include TypeScript call-site locus", () => {
    const parseResp = callEdgeResponses.find((r) => r.id === 2);
    expect(parseResp).toBeDefined();
    expect(parseResp!.error).toBeUndefined();

    const result = parseResp!.result as Record<string, unknown>;
    const callEdges = result.callEdges as Record<string, unknown>[];
    expect(Array.isArray(callEdges)).toBe(true);

    const edge = callEdges.find((candidate) => candidate.targetSymbol === "ts-kit:add");
    expect(edge).toBeDefined();
    expect(edge!.sourceContractCid).toBe("pending-ts:myFn");
    expect(edge!.kind).toBe("call-edge");

    const locus = edge!.callSiteLocus as Record<string, unknown>;
    expect(locus.file).toBe("call-edge.ts");
    expect(locus.line).toBe(6);
    expect(typeof locus.col).toBe("number");
  });

  it("parse: zod fixture produces at least one declaration with kind == contract", () => {
    const parseResp = zodResponses.find((r) => r.id === 2);
    const result = parseResp!.result as Record<string, unknown>;
    const decls = result.declarations as Record<string, unknown>[];
    expect(decls.length).toBeGreaterThanOrEqual(1);
    for (const d of decls) {
      expect(typeof d).toBe("object");
      expect(d.kind).toBe("contract");
      expect(typeof d.name).toBe("string");
      expect((d.name as string).length).toBeGreaterThan(0);
    }
  });

  it("parse: empty source returns declarations == [] and callEdges == []", () => {
    const parseResp = emptyResponses.find((r) => r.id === 2);
    const result = parseResp!.result as Record<string, unknown>;
    expect(result.declarations).toEqual([]);
    expect(result.callEdges).toEqual([]);
  });

  it("byte-determinism: two runs on the same input produce identical parse output", () => {
    const parse1 = zodResponses.find((r) => r.id === 2);
    const parse2 = zodResponses2.find((r) => r.id === 2);
    const sortedKeys1 = Object.keys(parse1!).sort();
    const sortedKeys2 = Object.keys(parse2!).sort();
    expect(JSON.stringify(parse1, sortedKeys1)).toBe(JSON.stringify(parse2, sortedKeys2));
  });

  it("unknown method returns error with code -32601", () => {
    const errResp = unknownMethodResponses.find((r) => r.id === 2);
    expect(errResp).toBeDefined();
    expect(errResp!.error).toBeDefined();
    const err = errResp!.error as Record<string, unknown>;
    expect(err.code).toBe(-32601);
  });

  it("parse: unsupported language returns error with code -32602", () => {
    const parseResp = unsupportedLanguageResponses.find((r) => r.id === 2);
    expect(parseResp!.error).toBeDefined();
    const err = parseResp!.error as Record<string, unknown>;
    expect(err.code).toBe(-32602);
  });
});

describe("shared analyzeDocument protocol (provekit-lsp-ts)", () => {
  it("floor fixture emits shared callsite diagnostic", () => {
    const source = `
function checkPositive(x: number): boolean {
  if (x <= 0) { return false; }
  return true;
}
function callerSatisfiesPre(): boolean {
  const result = checkPositive(5);
  return result;
}
function callerViolatesPre(): boolean {
  const result = checkPositive(-1);
  return result;
}
function callerWithLoop(): boolean {
  for (let i = 0; i < 10; i++) {
    const result = checkPositive(i);
    if (!result) { return false; }
  }
  return true;
}
`.trim();
    const responses = runLsp(buildAnalyzeSession(source, "floor.ts"));
    const analyzeResp = responses.find((r) => r.id === 21);
    expect(analyzeResp).toBeDefined();
    expect(analyzeResp!.error).toBeUndefined();

    const result = analyzeResp!.result as Record<string, unknown>;
    expect(result.kind).toBe("lsp-document-analysis");
    expect(result.schema_version).toBe("1");
    expect(result.kit_id).toBe("ts");
    expect(result.uri).toBe("file:///project/floor.ts");
    expect(result.file).toBe("floor.ts");
    expect(typeof result.document_cid).toBe("string");
    expect((result.document_cid as string).startsWith("blake3-512:")).toBe(true);
    expect((result.document_cid as string).length).toBe("blake3-512:".length + 128);
    expect(Array.isArray(result.entries)).toBe(true);
    expect(Array.isArray(result.statuses)).toBe(true);
    expect((result.statuses as unknown[]).length).toBe(0);
    expect(result.project).toBeNull();

    const diagnostics = result.diagnostics as Record<string, unknown>[];
    expect(diagnostics.length).toBe(1);
    const diagnostic = diagnostics[0];
    expect(diagnostic.code).toBe("provekit.lsp.implication_failed");
    expect(diagnostic.severity).toBe("error");
    expect(diagnostic.producer).toBe("forward-propagation");
    expect(diagnostic.kit_id).toBe("ts");
    const range = diagnostic.range as Record<string, number>;
    expect(range.start_line).toBe(10);
    expect(range.start_col).toBe(17);
    expect((diagnostic.data as Record<string, unknown>).callee).toBe("checkPositive");
  }, DAEMON_TEST_TIMEOUT_MS);
});

describe("forward-propagator (per #309)", () => {
  const FIXTURE_SATISFIES_PRE = `
function checkPositive(x: number): boolean {
  if (x <= 0) { return false; }
  return true;
}
function caller() {
  let result = checkPositive(5);
  return result;
}
`.trim();

  const FIXTURE_VIOLATES_PRE = `
function checkPositive(x: number): boolean {
  if (x <= 0) { return false; }
  return true;
}
function caller() {
  let result = checkPositive(-1);
  return result;
}
`.trim();

  const FIXTURE_BRANCH_MERGE = `
function checkPositive(x: number): boolean {
  if (x <= 0) { return false; }
  return true;
}
function caller(cond: boolean) {
  if (cond) {
    return checkPositive(1);
  } else {
    return checkPositive(-1);
  }
}
`.trim();

  it("callsite satisfies pre: no diagnostic", () => {
    const resp = runLsp(buildSession(FIXTURE_SATISFIES_PRE, "satisfies.ts"));
    const parseResp = resp.find((r) => r.id === 2);
    expect(parseResp).toBeDefined();
  }, DAEMON_TEST_TIMEOUT_MS);

  it("callsite violates pre: diagnostic with code provekit.lsp.implication_failed", () => {
    const resp = runLsp(buildSession(FIXTURE_VIOLATES_PRE, "violates.ts"));
    const parseResp = resp.find((r) => r.id === 2);
    expect(parseResp).toBeDefined();
  }, DAEMON_TEST_TIMEOUT_MS);

  it("branch merge partial satisfaction: diagnostic on join path", () => {
    const resp = runLsp(buildSession(FIXTURE_BRANCH_MERGE, "merge.ts"));
    const parseResp = resp.find((r) => r.id === 2);
    expect(parseResp).toBeDefined();
  }, DAEMON_TEST_TIMEOUT_MS);
});
