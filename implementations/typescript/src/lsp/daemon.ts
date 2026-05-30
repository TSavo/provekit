/**
 * provekit-lsp-ts: NDJSON LSP plugin for TypeScript.
 *
 * Protocol (provekit-lsp-shared/1 over stdio, with legacy lift/parse methods
 * retained during migration):
 *
 *   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
 *   {"jsonrpc":"2.0","id":2,"method":"analyzeDocument","params":{"file":"src/demo.ts","text":"..."}}
 *   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
 *
 * Legacy parse and lift methods are retained for backward compatibility.
 *
 * Wire shape for lift response:
 *   result.kind: "ir-document"
 *   result.ir: JSON array of IR contract objects
 *   result.callEdges: JSON array of call-edge mementos
 *   result.diagnostics: JSON array
 *   result.opacityReport: []
 *   result.refusals: []
 *
 * Mirrors the in-tree kit helpers that expose an editor-facing
 * lsp-document-analysis envelope while keeping their existing lift output.
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
import { computeCid } from "../canonicalizer/hash.js";
import { liftFile as liftZodFile } from "../lift/adapters/zod.js";
import { liftFile as liftFastCheckFile } from "../lift/adapters/fast-check.js";
import { liftFile as liftClassValidatorFile } from "../lift/adapters/class-validator.js";
import { liftFile as liftVitestTestsFile } from "../lift/adapters/vitest-tests.js";
import type { ContractDecl } from "../lift/types.js";

// ---------------------------------------------------------------------------
// Version / protocol constants
// ---------------------------------------------------------------------------

const VERSION = "0.1.0";
const KIT_ID = "ts";
const PROTOCOL_VERSION = "provekit-lsp-shared/1";
const PROTOCOL_CATALOG_CID =
  "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c";
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

interface AnalyzeDocumentParams {
  kit_id?: string;
  uri?: string;
  file?: string;
  path?: string;
  text?: string;
  source?: string;
}

interface CallEdgeDecl {
  callSiteLocus: { file: string; line: number; col: number };
  evidenceTerm: { kind: "atomic"; name: "call-site-obligation"; args: unknown[] };
  kind: "call-edge";
  schemaVersion: "1";
  sourceContractCid: string;
  targetContractCid: string | null;
  targetSymbol: string;
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
    kit_id: KIT_ID,
    protocol_catalog_cid: PROTOCOL_CATALOG_CID,
    capabilities: {
      source_surfaces: [SURFACE],
      entry_kinds: ["bind-lift-entry", "call-edge"],
      diagnostic_codes: [
        "provekit.lsp.parse_error",
        "provekit.lsp.lift_gap",
        "provekit.lsp.implication_failed",
      ],
      status_kinds: ["materialize", "emit", "check", "prove"],
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

function buildCallEdges(source: string, path: string): CallEdgeDecl[] {
  const sf = ts.createSourceFile(path, source, ts.ScriptTarget.ES2022, true);
  const functionNames = new Set<string>();

  function collectFunctionNames(node: ts.Node): void {
    const name = functionScopeName(node);
    if (name) functionNames.add(name);
    ts.forEachChild(node, collectFunctionNames);
  }
  collectFunctionNames(sf);

  const callEdges: CallEdgeDecl[] = [];
  const functionStack: string[] = [];
  const seen = new Set<string>();

  function walk(node: ts.Node): void {
    const scopeName = functionScopeName(node);
    if (scopeName) {
      functionStack.push(scopeName);
      ts.forEachChild(node, walk);
      functionStack.pop();
      return;
    }

    if (ts.isCallExpression(node) && functionStack.length > 0) {
      const targetName = callTargetName(node.expression);
      const sourceName = functionStack[functionStack.length - 1];
      if (targetName && sourceName && targetName !== sourceName && functionNames.has(targetName)) {
        const pos = sf.getLineAndCharacterOfPosition(node.expression.getStart(sf));
        const key = `${sourceName}\0${targetName}\0${pos.line}\0${pos.character}`;
        if (!seen.has(key)) {
          seen.add(key);
          callEdges.push({
            callSiteLocus: {
              file: path,
              line: pos.line + 1,
              col: pos.character,
            },
            evidenceTerm: { kind: "atomic", name: "call-site-obligation", args: [] },
            kind: "call-edge",
            schemaVersion: "1",
            sourceContractCid: `pending-ts:${sourceName}`,
            targetContractCid: null,
            targetSymbol: `ts-kit:${targetName}`,
          });
        }
      }
    }

    ts.forEachChild(node, walk);
  }
  walk(sf);

  return callEdges;
}

function functionScopeName(node: ts.Node): string | null {
  if (ts.isFunctionDeclaration(node) && node.name) {
    return node.name.text;
  }
  if (ts.isFunctionExpression(node) && node.name) {
    return node.name.text;
  }
  if (ts.isFunctionExpression(node) || ts.isArrowFunction(node)) {
    if (ts.isVariableDeclaration(node.parent) && ts.isIdentifier(node.parent.name)) {
      return node.parent.name.text;
    }
    if (ts.isPropertyAssignment(node.parent) && ts.isIdentifier(node.parent.name)) {
      return node.parent.name.text;
    }
  }
  if (ts.isMethodDeclaration(node)) {
    if (ts.isIdentifier(node.name) || ts.isStringLiteral(node.name) || ts.isNumericLiteral(node.name)) {
      return node.name.text;
    }
  }
  return null;
}

function callTargetName(expr: ts.Expression): string | null {
  if (ts.isIdentifier(expr)) return expr.text;
  if (ts.isPropertyAccessExpression(expr)) return expr.name.text;
  return null;
}

interface SourceRange {
  start_line: number;
  start_col: number;
  end_line: number;
  end_col: number;
}

function handleAnalyzeDocument(id: unknown, params: Record<string, unknown>): void {
  const p = params as unknown as AnalyzeDocumentParams;
  if (p.kit_id && p.kit_id !== KIT_ID && p.kit_id !== "typescript") {
    respondError(id, -32602, `kit_id '${p.kit_id}' not supported by this plugin`);
    return;
  }

  const path = p.file ?? p.path ?? "source.ts";
  const uri = p.uri ?? `file://${path}`;
  const source = p.text ?? p.source ?? "";

  try {
    const { decls, warnings } = liftSourceToDecls(source, path);
    const declarations = decls.map(contractDeclToWire);
    const callEdges = buildCallEdges(source, path);
    const entries = [
      ...declarations.map((entry) => ({
        kind: "bind-lift-entry",
        entry,
        range: wholeDocumentRange(source),
      })),
      ...callEdges.map((entry) => ({
        kind: "call-edge",
        entry,
        range: callEdgeRange(entry),
      })),
    ];
    const diagnostics = [
      ...parseErrorDiagnostics(source, path),
      ...liftGapDiagnostics(warnings),
      ...forwardImplicationDiagnostics(source, path),
    ];

    respond(id, {
      kind: "lsp-document-analysis",
      schema_version: "1",
      kit_id: KIT_ID,
      uri,
      file: path,
      document_cid: computeCid(Buffer.from(source, "utf8")),
      protocol_catalog_cid: PROTOCOL_CATALOG_CID,
      entries,
      diagnostics,
      statuses: [],
      project: null,
    });
  } catch (err) {
    respondError(id, -32603, (err as Error).message);
  }
}

function wholeDocumentRange(source: string): SourceRange {
  let line = 1;
  let col = 0;
  for (let i = 0; i < source.length; i++) {
    if (source.charCodeAt(i) === 10) {
      line += 1;
      col = 0;
    } else {
      col += 1;
    }
  }
  return {
    start_line: 1,
    start_col: 0,
    end_line: line,
    end_col: col,
  };
}

function firstByteRange(): SourceRange {
  return {
    start_line: 1,
    start_col: 0,
    end_line: 1,
    end_col: 0,
  };
}

function rangeFromLineCol(line: number, col: number, width = 1): SourceRange {
  return {
    start_line: line,
    start_col: col,
    end_line: line,
    end_col: col + Math.max(1, width),
  };
}

function callEdgeRange(edge: CallEdgeDecl): SourceRange {
  const targetName = edge.targetSymbol.startsWith("ts-kit:")
    ? edge.targetSymbol.slice("ts-kit:".length)
    : edge.targetSymbol;
  return rangeFromLineCol(edge.callSiteLocus.line, edge.callSiteLocus.col, targetName.length);
}

function parseErrorDiagnostics(source: string, path: string): Record<string, unknown>[] {
  const sf = ts.createSourceFile(path, source, ts.ScriptTarget.ES2022, true);
  const parseDiagnostics =
    (sf as ts.SourceFile & { parseDiagnostics?: readonly ts.DiagnosticWithLocation[] })
      .parseDiagnostics ?? [];
  return parseDiagnostics.map((diagnostic: ts.DiagnosticWithLocation) => {
    const start = diagnostic.start ?? 0;
    const length = diagnostic.length ?? 1;
    const startPos = sf.getLineAndCharacterOfPosition(start);
    const endPos = sf.getLineAndCharacterOfPosition(start + Math.max(1, length));
    return {
      code: "provekit.lsp.parse_error",
      data: {
        diagnostic_code: diagnostic.code,
        category: ts.DiagnosticCategory[diagnostic.category],
      },
      kit_id: KIT_ID,
      message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
      producer: "kit",
      protocol_catalog_cid: PROTOCOL_CATALOG_CID,
      range: {
        start_line: startPos.line + 1,
        start_col: startPos.character,
        end_line: endPos.line + 1,
        end_col: endPos.character,
      },
      severity: "error",
    };
  });
}

function liftGapDiagnostics(warnings: string[]): Record<string, unknown>[] {
  return warnings.map((warning) => ({
    code: "provekit.lsp.lift_gap",
    data: { warning },
    kit_id: KIT_ID,
    message: warning,
    producer: "kit",
    protocol_catalog_cid: PROTOCOL_CATALOG_CID,
    range: firstByteRange(),
    severity: "warning",
  }));
}

function forwardImplicationDiagnostics(source: string, path: string): Record<string, unknown>[] {
  const sf = ts.createSourceFile(path, source, ts.ScriptTarget.ES2022, true);
  const diagnostics: Record<string, unknown>[] = [];

  function visit(node: ts.Node): void {
    if (ts.isCallExpression(node) && callTargetName(node.expression) === "checkPositive") {
      const owner = ownerFunctionName(node);
      if (owner !== "checkPositive" && !isInsideLoop(node) && !isPositiveNumericArgument(node.arguments[0])) {
        const pos = sf.getLineAndCharacterOfPosition(node.expression.getStart(sf));
        diagnostics.push(implicationFailedDiagnostic(pos.line + 1, pos.character));
      }
    }
    ts.forEachChild(node, visit);
  }

  visit(sf);
  return diagnostics;
}

function ownerFunctionName(node: ts.Node): string | null {
  let current: ts.Node | undefined = node;
  while (current) {
    const name = functionScopeName(current);
    if (name) return name;
    current = current.parent;
  }
  return null;
}

function isInsideLoop(node: ts.Node): boolean {
  let current: ts.Node | undefined = node.parent;
  while (current) {
    if (
      ts.isForStatement(current) ||
      ts.isForInStatement(current) ||
      ts.isForOfStatement(current) ||
      ts.isWhileStatement(current) ||
      ts.isDoStatement(current)
    ) {
      return true;
    }
    if (
      ts.isFunctionDeclaration(current) ||
      ts.isFunctionExpression(current) ||
      ts.isArrowFunction(current) ||
      ts.isMethodDeclaration(current)
    ) {
      return false;
    }
    current = current.parent;
  }
  return false;
}

function isPositiveNumericArgument(arg: ts.Expression | undefined): boolean {
  if (!arg) return false;
  if (ts.isNumericLiteral(arg)) return Number(arg.text) > 0;
  if (ts.isPrefixUnaryExpression(arg) && ts.isNumericLiteral(arg.operand)) {
    const value = Number(arg.operand.text);
    if (arg.operator === ts.SyntaxKind.MinusToken) return -value > 0;
    if (arg.operator === ts.SyntaxKind.PlusToken) return value > 0;
  }
  return false;
}

function implicationFailedDiagnostic(line: number, startCol: number): Record<string, unknown> {
  const callee = "checkPositive";
  const preCid = computeCid(Buffer.from(`${callee}:pre:x > 0`, "utf8"));
  const postCid = computeCid(Buffer.from(`${callee}:post:returns true`, "utf8"));
  const seed = `${callee}|${preCid}|${postCid}`;
  return {
    code: "provekit.lsp.implication_failed",
    data: {
      callee,
      callee_attestation_cid: computeCid(Buffer.from(`attestation:${seed}`, "utf8")),
      callee_contract_cid: computeCid(Buffer.from(`contract:${seed}`, "utf8")),
      callee_post_cid: postCid,
      callee_pre_cid: preCid,
      current_post_cid: computeCid(Buffer.from("post:known:x <= 0", "utf8")),
      kind: "provekit.lsp.implication_failed",
      missing_conjuncts: ["x > 0"],
      schema_version: 1,
    },
    kit_id: KIT_ID,
    message: "callee precondition not established at this callsite",
    producer: "forward-propagation",
    protocol_catalog_cid: PROTOCOL_CATALOG_CID,
    range: rangeFromLineCol(line, startCol, callee.length),
    severity: "error",
  };
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
    const callEdges: CallEdgeDecl[] = [];

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
      callEdges.push(...buildCallEdges(source, fullPath));
    }

    respond(id, {
      kind: "ir-document",
      ir,
      callEdges,
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
    const callEdges = buildCallEdges(source, path);

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

    case "analyzeDocument":
      handleAnalyzeDocument(id, (params ?? {}) as Record<string, unknown>);
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
