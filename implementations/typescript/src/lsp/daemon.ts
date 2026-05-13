/**
 * provekit-lsp-ts: NDJSON LSP plugin for TypeScript.
 *
 * Protocol (provekit-lift/1 over stdio):
 *
 *   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
 *   {"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":"...","source_paths":[...]}}
 *   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
 *
 * Legacy parse method is retained for backward compatibility.
 *
 * Wire shape for lift response:
 *   result.kind: "ir-document"
 *   result.ir: JSON array of IR contract objects
 *   result.callEdges: [] (TS lifter cannot compute contract CIDs; follow-up #756)
 *   result.diagnostics: JSON array
 *   result.opacityReport: []
 *   result.refusals: []
 *
 * Mirrors implementations/go/provekit-lift-go/rpc.go.
 *
 * JCS gotcha: do NOT pass IrFormula objects through JSON.stringify and embed
 * the resulting string. JSON.parse them first so the response contains a
 * JSON array, not a JSON-encoded string (the linkerd's extract_array_field
 * handles the string fallback but the canonical shape is a native array).
 */

import { createInterface } from "node:readline";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import ts from "typescript";
import { liftFile as liftZodFile } from "../lift/adapters/zod.js";
import { liftFile as liftFastCheckFile } from "../lift/adapters/fast-check.js";
import { liftFile as liftClassValidatorFile } from "../lift/adapters/class-validator.js";
import { liftFile as liftVitestTestsFile } from "../lift/adapters/vitest-tests.js";
import type { ContractDecl } from "../lift/types.js";

// ---------------------------------------------------------------------------
// Version / protocol constants
// ---------------------------------------------------------------------------

const VERSION = "0.1.0";
const PROTOCOL_VERSION = "provekit-lift/1";
const SURFACE = "typescript-source";

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

interface RpcRequest {
  jsonrpc: string;
  id: unknown;
  method: string;
  params?: Record<string, unknown>;
}

interface ParseParams {
  path: string;
  source: string;
  language?: string;
}

interface LiftParams {
  workspace_root?: string;
  source_paths?: string[];
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

/**
 * Convert a ContractDecl to the wire-format declaration object.
 * The linkerd parses `kind == "contract"` with `name`, `outBinding`,
 * and optional `pre`/`post`/`inv` fields (IrFormula objects).
 */
function contractDeclToWire(decl: ContractDecl): Record<string, unknown> {
  const obj: Record<string, unknown> = {
    kind: "contract",
    name: decl.name,
    outBinding: decl.outBinding,
  };
  if (decl.pre !== undefined) obj.pre = decl.pre;
  if (decl.post !== undefined) obj.post = decl.post;
  if (decl.inv !== undefined) obj.inv = decl.inv;
  return obj;
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

let _sendImpl: (obj: unknown) => void = (obj) => {
  const line = JSON.stringify(obj);
  process.stdout.write(line + "\n");
};

/** Override in tests to capture output without writing to process.stdout. */
export function _setSendImpl(fn: (obj: unknown) => void): void {
  _sendImpl = fn;
}

function send(obj: unknown): void {
  _sendImpl(obj);
}

function respond(id: unknown, result: unknown): void {
  send({ jsonrpc: "2.0", id, result });
}

function respondError(id: unknown, code: number, message: string): void {
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

function handleInitialize(id: unknown): void {
  respond(id, {
    name: "provekit-lsp-ts",
    version: VERSION,
    protocol_version: PROTOCOL_VERSION,
    capabilities: {
      authoring_surfaces: [SURFACE],
      ir_version: "v1.1.0",
      emits_signed_mementos: false,
    },
  });
}

function liftSourceToDecls(
  source: string,
  path: string,
): { decls: ContractDecl[]; warnings: string[] } {
  const sf = ts.createSourceFile(path, source, ts.ScriptTarget.ES2022, true);
  const decls: ContractDecl[] = [];
  const warnings: string[] = [];

  const z = liftZodFile(sf, path);
  decls.push(...z.decls);
  for (const w of z.warnings) warnings.push(`zod: ${w.itemName}: ${w.reason}`);

  const f = liftFastCheckFile(sf, path);
  decls.push(...f.decls);
  for (const w of f.warnings) warnings.push(`fast-check: ${w.itemName}: ${w.reason}`);

  const cv = liftClassValidatorFile(sf, path);
  decls.push(...cv.decls);
  for (const w of cv.warnings) warnings.push(`class-validator: ${w.itemName}: ${w.reason}`);

  const vt = liftVitestTestsFile(sf, path);
  decls.push(...vt.decls);
  for (const w of vt.warnings) warnings.push(`vitest-tests: ${w.itemName}: ${w.reason}`);

  return { decls, warnings };
}

function handleLift(id: unknown, params: Record<string, unknown>): void {
  const p = params as unknown as LiftParams;
  const workspaceRoot = p.workspace_root ?? ".";
  const sourcePaths = p.source_paths ?? [];

  if (!Array.isArray(sourcePaths) || sourcePaths.length === 0) {
    respondError(id, -32602, "lift: source_paths must be a non-empty array");
    return;
  }

  try {
    const ir: ReturnType<typeof contractDeclToWire>[] = [];
    const diagnostics: unknown[] = [];

    for (const sp of sourcePaths) {
      const fullPath = resolve(workspaceRoot, sp);
      let source: string;
      try {
        source = readFileSync(fullPath, "utf8");
      } catch {
        // File not readable: skip with a diagnostic entry.
        diagnostics.push({ kind: "read-error", path: fullPath });
        continue;
      }

      const { decls } = liftSourceToDecls(source, fullPath);
      for (const decl of decls) ir.push(contractDeclToWire(decl));
    }

    respond(id, {
      kind: "ir-document",
      ir,
      callEdges: [],
      diagnostics,
      opacityReport: [],
      refusals: [],
    });
  } catch (err) {
    respondError(id, -32603, (err as Error).message);
  }
}

function handleParse(id: unknown, params: Record<string, unknown>): void {
  const p = params as unknown as ParseParams;
  const path = p.path ?? "";
  const source = p.source ?? "";
  const language = p.language ?? "typescript";

  if (language !== "typescript" && language !== "ts") {
    respondError(id, -32602, `language '${language}' not supported by this plugin`);
    return;
  }

  try {
    const { decls, warnings } = liftSourceToDecls(source, path);

    // Convert ContractDecl[] to wire-format array.
    const declarations = decls.map(contractDeclToWire);

    // TS lifter does not emit call edges. The linkerd treats absent/empty
    // callEdges as an empty list (same as the zig lifter).
    const callEdges: unknown[] = [];

    respond(id, { declarations, callEdges, warnings });
  } catch (err) {
    respondError(id, -32603, (err as Error).message);
  }
}

function handleShutdown(id: unknown): void {
  respond(id, null);
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

/**
 * Process a single NDJSON request line.
 * Returns `true` to continue, `false` to stop (shutdown received).
 */
export function handleRequest(line: string): boolean {
  let req: RpcRequest;
  try {
    req = JSON.parse(line) as RpcRequest;
  } catch {
    // Malformed JSON: skip.
    return true;
  }

  const { id, method, params } = req;

  switch (method) {
    case "initialize":
      handleInitialize(id);
      return true;

    case "lift":
      handleLift(id, (params ?? {}) as Record<string, unknown>);
      return true;

    case "parse":
      handleParse(id, (params ?? {}) as Record<string, unknown>);
      return true;

    case "shutdown":
      handleShutdown(id);
      return false;

    default:
      respondError(id, -32601, `method '${method}' not found`);
      return true;
  }
}

/**
 * Run the LSP daemon main loop (NDJSON over stdio).
 * Reads one JSON line per request; writes one JSON line per response.
 */
export function main(): void {
  const rl = createInterface({ input: process.stdin, terminal: false });
  let active = true;

  rl.on("line", (line: string) => {
    if (!active) return;
    const cont = handleRequest(line.trim());
    if (!cont) {
      active = false;
      rl.close();
    }
  });

  rl.on("close", () => {
    // stdin closed (pipe closed after shutdown or EOF).
  });
}
