/**
 * Integration tests for the provekit-lsp-ts daemon protocol.
 *
 * Mirrors implementations/python/provekit-lift-py-tests/tests/test_daemon_protocol.py.
 *
 * Asserts:
 *   - initialize responds with name == "provekit-lsp-ts" and capabilities == ["parse"].
 *   - parse returns result.declarations as a JSON array (not a string).
 *   - parse returns result.callEdges as a JSON array.
 *   - With a contract-bearing fixture, each declaration has kind == "contract".
 *   - Empty source returns declarations == [] and callEdges == [].
 *   - Byte-determinism: two runs on the same input produce identical parse output.
 *   - Unknown method returns a JSON-RPC error with code -32601.
 */

import { describe, it, expect, beforeAll } from "vitest";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Resolve tsx/cli the same way the bin launchers do — works with pnpm's
// node_modules/.pnpm symlink layout without needing an explicit repo root path.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const TSX_CLI: string = require.resolve("tsx/cli");
const DAEMON_ENTRY = resolve(__dirname, "daemon-entry.ts");

/** Spawn the daemon, feed ndjson, return parsed response lines. */
function runLsp(ndjsonInput: string): Record<string, unknown>[] {
  const result = spawnSync(process.execPath, [TSX_CLI, DAEMON_ENTRY], {
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

// Per-suite vitest timeout. Each individual test just reads cached data.
const SUITE_TIMEOUT_MS = 60_000;

describe("daemon protocol conformance (provekit-lsp-ts)", () => {
  beforeAll(() => {
    zodResponses = runLsp(buildSession(ZOD_FIXTURE_SOURCE, ZOD_FIXTURE_PATH));
    emptyResponses = runLsp(buildSession("// no contracts here\n", "empty.ts"));
    zodResponses2 = runLsp(buildSession(ZOD_FIXTURE_SOURCE, ZOD_FIXTURE_PATH));

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

  it("initialize: name == provekit-lsp-ts and capabilities includes parse", () => {
    const initResp = zodResponses.find((r) => r.id === 1);
    expect(initResp).toBeDefined();
    const result = initResp!.result as Record<string, unknown>;
    expect(result.name).toBe("provekit-lsp-ts");
    expect(Array.isArray(result.capabilities)).toBe(true);
    expect(result.capabilities).toContain("parse");
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
