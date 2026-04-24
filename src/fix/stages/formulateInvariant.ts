/**
 * C1: Invariant formulator.
 *
 * Given a BugSignal and BugLocus, produces a Z3-checkable InvariantClaim.
 * Oracle #1 fires inside this function — every returned InvariantClaim has
 * been verified SAT by Z3. Principle-match path is tried before LLM path.
 */

import { readFileSync, existsSync } from "fs";
import { join, dirname } from "path";
import { eq } from "drizzle-orm";
import type { BugSignal, BugLocus, InvariantClaim, LLMProvider, SmtBindingRef } from "../types.js";
import { InvariantFormulationFailed } from "../types.js";
import type { Db } from "../../db/index.js";
import { principleMatches, principleMatchCaptures } from "../../db/schema/principleMatches.js";
import { nodes, files as filesTable } from "../../sast/schema/index.js";
import { nodeArithmetic } from "../../sast/schema/capabilities/arithmetic.js";
import { verifyBlock, proofComplexity } from "../../verifier.js";

// ---------------------------------------------------------------------------
// Principle JSON shape
// ---------------------------------------------------------------------------

interface PrincipleJson {
  id?: string;
  name?: string;
  description?: string;
  smt2Template?: string;
  smt2ProofTemplate?: string;
  teachingExample?: { domain: string; explanation: string; smt2: string };
}

// ---------------------------------------------------------------------------
// Principle loader
// ---------------------------------------------------------------------------

/** Resolve the .neurallog/principles/ directory relative to the project root. */
function findPrinciplesDir(): string {
  // Walk up from __dirname (CJS) until we find .neurallog/.
  // In source, __dirname is src/fix/stages.
  // In dist, __dirname is dist/fix/stages.
  let dir = __dirname;
  for (let i = 0; i < 10; i++) {
    const candidate = join(dir, ".neurallog", "principles");
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  // Last-resort: cwd-relative (e.g. when running from project root via ts-node/vitest).
  return join(process.cwd(), ".neurallog", "principles");
}

function loadPrincipleJson(principleName: string): PrincipleJson | null {
  const principlesDir = findPrinciplesDir();
  const jsonPath = join(principlesDir, `${principleName}.json`);
  if (!existsSync(jsonPath)) return null;
  try {
    return JSON.parse(readFileSync(jsonPath, "utf-8")) as PrincipleJson;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Template placeholder extraction
// ---------------------------------------------------------------------------

/** Extract all {{name}} placeholder names from an SMT template. */
function extractPlaceholders(template: string): string[] {
  const matches = [...template.matchAll(/\{\{([^}]+)\}\}/g)];
  const seen = new Set<string>();
  const result: string[] = [];
  for (const m of matches) {
    const name = m[1]!.trim();
    if (!seen.has(name)) {
      seen.add(name);
      result.push(name);
    }
  }
  return result;
}

/** Substitute all {{name}} → name in a template string. */
function substituteTemplate(template: string, bindings: SmtBindingRef[]): string {
  let result = template;
  for (const b of bindings) {
    result = result.replaceAll(`{{${b.smt_constant}}}`, b.smt_constant);
  }
  return result;
}

// ---------------------------------------------------------------------------
// Heuristic placeholder-to-arithmetic-role mapping
// ---------------------------------------------------------------------------

const LHS_NAMES = new Set(["numerator", "dividend", "left", "base", "lhs", "a", "x"]);
const RHS_NAMES = new Set(["denominator", "divisor", "right", "delta", "rhs", "b", "y"]);

/**
 * Given a captured node that may be an arithmetic binary expression,
 * look up node_arithmetic and return {lhsNodeId, rhsNodeId}.
 */
function lookupArithmeticChildren(
  db: Db,
  nodeId: string,
): { lhsNodeId: string; rhsNodeId: string } | null {
  const row = db
    .select({ lhsNode: nodeArithmetic.lhsNode, rhsNode: nodeArithmetic.rhsNode })
    .from(nodeArithmetic)
    .where(eq(nodeArithmetic.nodeId, nodeId))
    .get();
  if (!row) return null;
  return { lhsNodeId: row.lhsNode, rhsNodeId: row.rhsNode };
}

/**
 * Build SmtBinding[] from principle captures + template placeholders.
 *
 * Strategy:
 * 1. If placeholder name matches a capture name directly → use that capture node.
 * 2. If the capture is an arithmetic node and placeholder hints at lhs/rhs → use child node.
 * 3. Fallback: use the first capture node for unmatched placeholders.
 */
function buildBindings(
  db: Db,
  captures: { captureName: string; capturedNodeId: string }[],
  placeholders: string[],
): SmtBindingRef[] {
  // Build a map of capture name → nodeId.
  const captureMap = new Map<string, string>();
  for (const c of captures) {
    captureMap.set(c.captureName, c.capturedNodeId);
  }

  // Resolve node source positions.
  function nodeInfo(nodeId: string): { sourceLine: number; fileId: number } {
    const row = db
      .select({ sourceLine: nodes.sourceLine, fileId: nodes.fileId })
      .from(nodes)
      .where(eq(nodes.id, nodeId))
      .get();
    return { sourceLine: row?.sourceLine ?? 0, fileId: row?.fileId ?? 0 };
  }

  // Pre-fetch arithmetic children for all captures.
  const arithmeticChildren = new Map<string, { lhsNodeId: string; rhsNodeId: string }>();
  for (const c of captures) {
    const children = lookupArithmeticChildren(db, c.capturedNodeId);
    if (children) {
      arithmeticChildren.set(c.captureName, children);
    }
  }

  const firstCaptureNodeId = captures[0]?.capturedNodeId ?? "";

  const bindings: SmtBindingRef[] = [];

  for (const placeholder of placeholders) {
    // Strategy 1: direct name match.
    if (captureMap.has(placeholder)) {
      const nodeId = captureMap.get(placeholder)!;
      const info = nodeInfo(nodeId);
      bindings.push({
        smt_constant: placeholder,
        source_line: info.sourceLine,
        source_expr: placeholder,
        sort: "Int",
      });
      continue;
    }

    // Strategy 2: heuristic via arithmetic children.
    let resolvedNodeId: string | null = null;
    for (const [capName, children] of arithmeticChildren) {
      void capName;
      if (LHS_NAMES.has(placeholder)) {
        resolvedNodeId = children.lhsNodeId;
        break;
      }
      if (RHS_NAMES.has(placeholder)) {
        resolvedNodeId = children.rhsNodeId;
        break;
      }
    }

    if (resolvedNodeId) {
      const info = nodeInfo(resolvedNodeId);
      bindings.push({
        smt_constant: placeholder,
        source_line: info.sourceLine,
        source_expr: placeholder,
        sort: "Int",
      });
      continue;
    }

    // Strategy 3: fallback — use first capture node.
    const fallbackNodeId = firstCaptureNodeId;
    const info = fallbackNodeId ? nodeInfo(fallbackNodeId) : { sourceLine: 0, fileId: 0 };
    bindings.push({
      smt_constant: placeholder,
      source_line: info.sourceLine,
      source_expr: placeholder,
      sort: "Int",
    });
  }

  return bindings;
}

// ---------------------------------------------------------------------------
// Oracle #1 gate
// ---------------------------------------------------------------------------

function runOracleOne(formalExpression: string): { witness: string | null } {
  const z3 = verifyBlock(formalExpression);
  if (z3.result === "sat") {
    return { witness: z3.witness ?? null };
  }
  if (z3.result === "unsat") {
    throw new InvariantFormulationFailed(
      `oracle #1 unsat — negated-goal is unsatisfiable; invariant formulation was vacuous`,
    );
  }
  if (z3.result === "unknown") {
    throw new InvariantFormulationFailed(
      `oracle #1 unknown — Z3 cannot decide; invariant is unusable`,
    );
  }
  // result === "error"
  throw new InvariantFormulationFailed(
    `oracle #1 error: ${z3.error ?? "Z3 returned error"}`,
  );
}

// ---------------------------------------------------------------------------
// LLM path helpers
// ---------------------------------------------------------------------------

interface LlmInvariantResponse {
  description: string;
  smt_declarations: string[];
  smt_violation_assertion: string;
  bindings: { smt_constant: string; source_expr: string; sort: string }[];
}

function buildLlmPrompt(signal: BugSignal, locus: BugLocus, db: Db): string {
  // Gather source context around locus.
  let sourceContext = "(source not available)";
  try {
    const fileRow = db
      .select({ path: filesTable.path })
      .from(filesTable)
      .where(
        eq(filesTable.id,
          db.select({ fileId: nodes.fileId })
            .from(nodes)
            .where(eq(nodes.id, locus.primaryNode))
            .get()?.fileId ?? 0,
        ),
      )
      .get();
    if (fileRow && existsSync(fileRow.path)) {
      const lines = readFileSync(fileRow.path, "utf-8").split("\n");
      const start = Math.max(0, locus.line - 3);
      const end = Math.min(lines.length, locus.line + 2);
      sourceContext = lines.slice(start, end).map((l, i) => `${start + i + 1}: ${l}`).join("\n");
    }
  } catch {
    // ignore
  }

  return `You are a formal verification expert. Given a bug report, produce an SMT-LIB assertion
that expresses the VIOLATION STATE (the negation of the desired invariant).
The assertion must be satisfiable (Z3 check-sat returns "sat") — this proves the bug is reachable.

Bug summary: ${signal.summary}
Failure description: ${signal.failureDescription}
Location: ${locus.file}:${locus.line}${locus.function ? ` in ${locus.function}` : ""}

Source context:
\`\`\`
${sourceContext}
\`\`\`

Data-flow ancestors of bug site: ${locus.dataFlowAncestors.length} nodes
Data-flow descendants of bug site: ${locus.dataFlowDescendants.length} nodes

Respond with ONLY a JSON object (no markdown fences):
{
  "description": "one sentence: what invariant is being violated",
  "smt_declarations": ["(declare-const varName Sort)", ...],
  "smt_violation_assertion": "(assert (...))",
  "bindings": [
    {"smt_constant": "varName", "source_expr": "expression in source", "sort": "Int"}
  ]
}

Rules:
- smt_declarations: declare all constants you use
- smt_violation_assertion: a single (assert ...) encoding the violation state
- Do NOT include (check-sat) — it will be appended automatically
- Use Int or Bool sorts
- Keep it simple: 2-5 constants maximum`;
}

function parseLlmResponse(raw: string): {
  formalExpression: string;
  bindings: SmtBindingRef[];
  description: string;
} {
  // Strip markdown fences if present.
  let cleaned = raw.trim();
  if (cleaned.startsWith("```")) {
    cleaned = cleaned.replace(/^```[a-z]*\n?/, "").replace(/```\s*$/, "").trim();
  }

  let parsed: LlmInvariantResponse;
  try {
    parsed = JSON.parse(cleaned) as LlmInvariantResponse;
  } catch {
    throw new InvariantFormulationFailed(
      `LLM response is not valid JSON: ${cleaned.slice(0, 100)}`,
    );
  }

  if (!parsed.description || typeof parsed.description !== "string") {
    throw new InvariantFormulationFailed("LLM response missing 'description' field");
  }
  if (!Array.isArray(parsed.smt_declarations)) {
    throw new InvariantFormulationFailed("LLM response missing 'smt_declarations' array");
  }
  if (!parsed.smt_violation_assertion || typeof parsed.smt_violation_assertion !== "string") {
    throw new InvariantFormulationFailed("LLM response missing 'smt_violation_assertion' field");
  }

  // Validate basic SMT structure.
  const decls = parsed.smt_declarations as string[];
  const assertion = parsed.smt_violation_assertion;

  // Check parenthesis balance.
  let depth = 0;
  for (const line of [...decls, assertion]) {
    for (const ch of line) {
      if (ch === "(") depth++;
      else if (ch === ")") depth--;
    }
  }
  if (depth !== 0) {
    throw new InvariantFormulationFailed(
      `SMT from LLM has unbalanced parentheses (depth=${depth})`,
    );
  }

  // Validate assertion starts with (assert.
  if (!assertion.trim().startsWith("(assert")) {
    throw new InvariantFormulationFailed(
      `smt_violation_assertion must start with '(assert ...)'`,
    );
  }

  // Validate each binding's smt_constant appears in declarations.
  const rawBindings = (parsed.bindings ?? []) as { smt_constant: string; source_expr: string; sort: string }[];
  const declaredConsts = new Set<string>();
  for (const decl of decls) {
    const m = decl.match(/\(declare-const\s+(\S+)/);
    if (m && m[1]) declaredConsts.add(m[1]);
  }
  for (const b of rawBindings) {
    if (!declaredConsts.has(b.smt_constant)) {
      throw new InvariantFormulationFailed(
        `Binding smt_constant '${b.smt_constant}' not found in smt_declarations`,
      );
    }
  }

  const formalExpression = [...decls, assertion, "(check-sat)"].join("\n");

  const bindings: SmtBindingRef[] = rawBindings.map((b) => ({
    smt_constant: b.smt_constant,
    source_line: 0, // LLM path: line resolved later if needed
    source_expr: b.source_expr ?? b.smt_constant,
    sort: b.sort ?? "Int",
  }));

  return { formalExpression, bindings, description: parsed.description };
}

// ---------------------------------------------------------------------------
// Main export
// ---------------------------------------------------------------------------

export async function formulateInvariant(args: {
  signal: BugSignal;
  locus: BugLocus;
  db: Db;
  llm: LLMProvider;
}): Promise<InvariantClaim> {
  const { signal, locus, db, llm } = args;

  // -------------------------------------------------------------------------
  // Path 1: existing principle match at locus
  // -------------------------------------------------------------------------

  const allMatches = db
    .select({
      id: principleMatches.id,
      principleName: principleMatches.principleName,
      rootMatchNodeId: principleMatches.rootMatchNodeId,
      message: principleMatches.message,
    })
    .from(principleMatches)
    .where(eq(principleMatches.rootMatchNodeId, locus.primaryNode))
    .all();

  if (allMatches.length > 0) {
    const match = allMatches[0]!;

    // Load principle JSON.
    const principle = loadPrincipleJson(match.principleName);
    if (!principle) {
      throw new InvariantFormulationFailed(
        `principle '${match.principleName}' has a DB match but no JSON file in .neurallog/principles/`,
      );
    }

    // Use smt2Template (violation template) for pre-fix SAT check.
    // smt2ProofTemplate is for post-fix UNSAT proving — it adds contradictory constraints.
    const smtTemplate = principle.smt2Template;
    if (!smtTemplate) {
      throw new InvariantFormulationFailed(
        `principle '${match.principleName}' has no smt2Template`,
      );
    }

    // Load captures for this match.
    const captureRows = db
      .select({
        captureName: principleMatchCaptures.captureName,
        capturedNodeId: principleMatchCaptures.capturedNodeId,
      })
      .from(principleMatchCaptures)
      .where(eq(principleMatchCaptures.matchId, match.id))
      .all();

    // Extract placeholder names from template.
    const placeholders = extractPlaceholders(smtTemplate);
    if (placeholders.length === 0) {
      throw new InvariantFormulationFailed(
        `principle '${match.principleName}' smt2Template has no {{placeholder}} variables`,
      );
    }

    // Build bindings (maps placeholder names → source positions).
    const bindings = buildBindings(db, captureRows, placeholders);

    // Substitute placeholders → SMT constant names.
    const formalExpression = substituteTemplate(smtTemplate, bindings);

    // Oracle #1: must return SAT.
    const { witness } = runOracleOne(formalExpression);

    return {
      principleId: match.principleName,
      description: principle.description ?? match.message,
      formalExpression,
      bindings,
      complexity: proofComplexity(formalExpression),
      witness,
    };
  }

  // -------------------------------------------------------------------------
  // Path 2: novel — LLM proposes
  // -------------------------------------------------------------------------

  const prompt = buildLlmPrompt(signal, locus, db);
  const rawResponse = await llm.complete({ prompt });
  const { formalExpression, bindings, description } = parseLlmResponse(rawResponse);

  // Oracle #1: must return SAT.
  const { witness } = runOracleOne(formalExpression);

  return {
    principleId: null,
    description,
    formalExpression,
    bindings,
    complexity: proofComplexity(formalExpression),
    witness,
  };
}
