/**
 * C1: Invariant formulator.
 *
 * Given a BugSignal and BugLocus, produces a Z3-checkable InvariantClaim.
 * Oracle #1 fires inside this function — every returned InvariantClaim has
 * been verified SAT by Z3. Principle-match path is tried before LLM path.
 */

import { readFileSync, existsSync, readdirSync } from "fs";
import { join, dirname } from "path";
import { eq, or, and, lte, gte } from "drizzle-orm";
import type { BugSignal, BugLocus, InvariantClaim, LLMProvider, SmtBindingRef, InvariantCitation } from "../types.js";
import { InvariantFormulationFailed } from "../types.js";
import { createNoopLogger, type FixLoopLogger } from "../logger.js";
import { requestStructuredJson } from "../llm/structuredOutput.js";
import { getModelTier } from "../modelTiers.js";
import type { Db } from "../../db/index.js";
import { principleMatches, principleMatchCaptures } from "../../db/schema/principleMatches.js";
import { nodes, files as filesTable } from "../../sast/schema/index.js";
import { nodeArithmetic } from "../../sast/schema/capabilities/arithmetic.js";
import { verifyBlock, proofComplexity } from "../../verifier.js";
import { runInvariantFidelity, type FidelityVerifiers } from "../invariantFidelity.js";
import { evaluatePrinciple } from "../../dsl/evaluator.js";

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

/** Resolve the .provekit/principles/ directory relative to the project root. */
function findPrinciplesDir(): string {
  // Walk up from __dirname (CJS) until we find .provekit/.
  // In source, __dirname is src/fix/stages.
  // In dist, __dirname is dist/fix/stages.
  let dir = __dirname;
  for (let i = 0; i < 10; i++) {
    const candidate = join(dir, ".provekit", "principles");
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  // Last-resort: cwd-relative (e.g. when running from project root via ts-node/vitest).
  return join(process.cwd(), ".provekit", "principles");
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

/**
 * Pitch-leak 6 Win 2: warm the principleMatches table.
 *
 * The C1 short-circuit at "Path 1" below queries principleMatches for a row
 * at locus.primaryNode. In production, that table is populated only by the
 * fix loop itself (C4 / oracle code) — `provekit analyze` builds the SAST
 * but does NOT evaluate principles. So Path 1 was effectively dormant in
 * real-LLM runs even though every test that touched it passed.
 *
 * This helper closes the gap: when principleMatches has zero rows for the
 * locus's file, we evaluate every .dsl in .provekit/principles/ against
 * args.db once. Each evaluation inserts rows for every match in the indexed
 * SAST graph. Subsequent calls return immediately because the file already
 * has rows. Wall-time cost on a fresh db: ~50-200ms total for the canonical
 * library (single SQL query per principle, no LLM, no Z3).
 *
 * Failure modes are deliberately swallowed: a malformed DSL, a missing
 * capability, an INSERT collision — none of these are catastrophic at the
 * C1 entry, because Path 1 is opportunistic. If population fails, Path 2
 * (LLM-novel) takes over with the same cost as before.
 */
function ensurePrincipleMatchesPopulated(db: Db, locusFileId: number, logger: FixLoopLogger): void {
  // Cheap existence check: does any row exist for this file?
  const existing = db
    .select({ id: principleMatches.id })
    .from(principleMatches)
    .where(eq(principleMatches.fileId, locusFileId))
    .limit(1)
    .all();
  if (existing.length > 0) return;

  // Empty for this file. Evaluate every DSL in the principles dir.
  // We re-evaluate everything (not just for one file) because evaluatePrinciple
  // is whole-DB scoped and re-running it when partial populations exist would
  // duplicate rows. The cost is bounded: <20 principles, <1ms each on small
  // databases, ~50-200ms total in practice.
  const principlesDir = findPrinciplesDir();
  if (!existsSync(principlesDir)) return;

  let dslFiles: string[];
  try {
    dslFiles = readdirSync(principlesDir).filter((f) => f.endsWith(".dsl"));
  } catch {
    return;
  }

  const t0 = Date.now();
  let evaluatedCount = 0;
  for (const dslFile of dslFiles) {
    const dslPath = join(principlesDir, dslFile);
    let dslSource: string;
    try {
      dslSource = readFileSync(dslPath, "utf-8");
    } catch {
      continue;
    }
    try {
      evaluatePrinciple(db, dslSource);
      evaluatedCount++;
    } catch (err) {
      // DSL eval errors (compile errors, missing capabilities) are non-fatal:
      // we just won't have rows from this principle. Log at detail level so
      // a curious operator can find it.
      logger.detail(
        `[C1] principle ${dslFile} evaluation skipped: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }
  logger.detail(`[C1] populated principleMatches from ${evaluatedCount}/${dslFiles.length} DSL files in ${Date.now() - t0}ms`);
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

/**
 * Kind-agnostic by construction: SAT-checking the violation SMT works whether
 * the invariant is concrete (Int/Real declarations + arithmetic constraints)
 * or abstract (Bool-only, taint-style). Z3 will return SAT for any
 * non-contradictory formula — for an abstract `(declare-const tainted Bool)
 * (assert tainted)`, the trivial witness `(model (define-fun tainted () Bool true))`
 * proves the bug shape is consistent. The discriminator between concrete and
 * abstract lives in oracle #2 (post-fix verification), not here.
 */
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
  citations?: { smt_clause: string; source_quote: string }[];
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
  ],
  "citations": [
    {"smt_clause": "(= b 0)", "source_quote": "exact or close-paraphrase quote from the bug report that justifies this clause"}
  ]
}

Rules:
- smt_declarations: declare all constants you use
- smt_violation_assertion: a single (assert ...) encoding the violation state
- Do NOT include (check-sat) — it will be appended automatically
- Use Int or Bool sorts
- Keep it simple: 2-5 constants maximum
- citations: one entry per meaningful clause in smt_violation_assertion; each source_quote must be a verbatim or close-paraphrase excerpt from the bug report above

CRITICAL — invariant must be VERIFIABLE post-fix:
- ONLY use SMT constants that map directly to source-code INPUT variables
  (function parameters, named locals). Each binding's source_expr must be a
  literal substring of source code (e.g., "b", "a", "x").
- DO NOT introduce SYMBOLIC CONTROL-FLOW VARIABLES (e.g., "throws Bool",
  "guard_returns Int", "code_after_reached Bool"). These cannot be verified
  against the post-fix code: oracle #2's path-condition extraction can only
  rebind source variables to dominating guards, not synthetic booleans.
- Express the violation as constraints on input variables only.
  Good: "(assert (= b 0))"   ← b is a function parameter; oracle #2 can verify
  Bad:  "(assert (and (= b 0) (= throws false)))"  ← oracle #2 cannot verify "throws"
- If the bug only manifests under specific control flow, encode that as
  constraints on the input variables that REACH the bug site, not as a
  synthetic flag.`;
}

/**
 * Validate + transform a parsed LLM JSON response into invariant components.
 * Throws InvariantFormulationFailed on any structural / semantic violation.
 * Used as the schemaCheck for requestStructuredJson.
 */
function validateLlmResponse(rawParsed: unknown): {
  formalExpression: string;
  bindings: SmtBindingRef[];
  description: string;
  citations: InvariantCitation[] | null;
} {
  if (typeof rawParsed !== "object" || rawParsed === null) {
    throw new InvariantFormulationFailed("LLM response is not an object");
  }
  const parsed = rawParsed as LlmInvariantResponse;

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

  // Parse citations (optional field — fidelity check requires them but parse is lenient)
  let citations: InvariantCitation[] | null = null;
  if (Array.isArray(parsed.citations)) {
    citations = (parsed.citations as { smt_clause: string; source_quote: string }[])
      .filter((c) => typeof c.smt_clause === "string" && typeof c.source_quote === "string")
      .map((c) => ({ smt_clause: c.smt_clause, source_quote: c.source_quote }));
  }

  return { formalExpression, bindings, description: parsed.description, citations };
}

// ---------------------------------------------------------------------------
// Main export
// ---------------------------------------------------------------------------

export async function formulateInvariant(args: {
  signal: BugSignal;
  locus: BugLocus;
  db: Db;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Dependency injection for fidelity verifiers — for testing only. */
  _fidelityVerifiers?: FidelityVerifiers;
}): Promise<InvariantClaim> {
  const { signal, locus, db, llm } = args;
  const logger = args.logger ?? createNoopLogger();

  // -------------------------------------------------------------------------
  // Path 1: existing principle match at locus
  // -------------------------------------------------------------------------

  // Pitch-leak 6 Win 2: ensure principleMatches has been populated for the
  // locus's file. In production, `provekit analyze` does not run the DSL
  // evaluator, so the table is empty until the fix loop populates it on
  // demand. This call is a no-op when rows already exist.
  const locusFileRow = db
    .select({ fileId: nodes.fileId })
    .from(nodes)
    .where(eq(nodes.id, locus.primaryNode))
    .get();
  if (locusFileRow?.fileId !== undefined) {
    ensurePrincipleMatchesPopulated(db, locusFileRow.fileId, logger);
  }

  // Exact match: principle matched exactly at locus.primaryNode.
  let allMatches = db
    .select({
      id: principleMatches.id,
      principleName: principleMatches.principleName,
      rootMatchNodeId: principleMatches.rootMatchNodeId,
      message: principleMatches.message,
    })
    .from(principleMatches)
    .where(eq(principleMatches.rootMatchNodeId, locus.primaryNode))
    .all();

  // Span-containment fallback: locate() can pick a child node while the DSL
  // captures a parent (e.g. the full BinaryExpression vs. just the operator).
  // If no exact match, look for a principle match whose rootMatchNode's span
  // CONTAINS locus.primaryNode's span — i.e. the locus is inside the match.
  if (allMatches.length === 0) {
    const locusNode = db
      .select({ sourceStart: nodes.sourceStart, sourceEnd: nodes.sourceEnd, fileId: nodes.fileId })
      .from(nodes)
      .where(eq(nodes.id, locus.primaryNode))
      .get();

    if (locusNode) {
      // Alias principleMatches root node to get its span.
      const matchNodes = db
        .select({
          id: principleMatches.id,
          principleName: principleMatches.principleName,
          rootMatchNodeId: principleMatches.rootMatchNodeId,
          message: principleMatches.message,
          matchNodeStart: nodes.sourceStart,
          matchNodeEnd: nodes.sourceEnd,
        })
        .from(principleMatches)
        .innerJoin(nodes, eq(nodes.id, principleMatches.rootMatchNodeId))
        .where(
          and(
            eq(nodes.fileId, locusNode.fileId),
            lte(nodes.sourceStart, locusNode.sourceStart),
            gte(nodes.sourceEnd, locusNode.sourceEnd),
          ),
        )
        .all();

      allMatches = matchNodes.map((r) => ({
        id: r.id,
        principleName: r.principleName,
        rootMatchNodeId: r.rootMatchNodeId,
        message: r.message,
      }));
    }
  }

  if (allMatches.length > 0) {
    const match = allMatches[0]!;

    // Load principle JSON.
    const principle = loadPrincipleJson(match.principleName);
    if (!principle) {
      throw new InvariantFormulationFailed(
        `principle '${match.principleName}' has a DB match but no JSON file in .provekit/principles/`,
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
    //
    // Pitch-leak 6 Win 2: a template with zero {{...}} tokens is treated as
    // already-concrete SMT (no substitution needed). This is the case for
    // abstract Bool-style principles (shell-injection, taint patterns) whose
    // template is a literal "constants + assert + check-sat" sequence rather
    // than a parameterized formula. Previously this threw and forced the
    // novel-LLM path; now we accept it and produce a binding for the first
    // capture so downstream stages still have somewhere to anchor source
    // positions in audit output.
    const placeholders = extractPlaceholders(smtTemplate);

    // Build bindings (maps placeholder names → source positions).
    // For zero-placeholder templates we still build a binding for the
    // primary capture to keep the bindings array non-empty (downstream
    // oracles dereference bindings[0] when reporting witness positions).
    const bindings = placeholders.length > 0
      ? buildBindings(db, captureRows, placeholders)
      : buildBindings(db, captureRows, captureRows.length > 0 ? [captureRows[0]!.captureName] : []);

    // Substitute placeholders → SMT constant names.
    // For zero-placeholder templates this is the identity transform.
    const formalExpression = placeholders.length > 0
      ? substituteTemplate(smtTemplate, bindings)
      : smtTemplate;

    // Oracle #1: must return SAT.
    const { witness } = runOracleOne(formalExpression);
    logger.oracle({ id: 1, name: "Z3 SAT check (principle)", passed: witness !== null, detail: `principle=${match.principleName}` });

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
  logger.detail(`C1 LLM prompt (novel path): ${prompt.slice(0, 200)}...`);
  let formalExpression: string;
  let bindings: SmtBindingRef[];
  let description: string;
  let citations: InvariantCitation[] | null;
  try {
    ({ formalExpression, bindings, description, citations } = await requestStructuredJson({
      prompt,
      llm,
      stage: "C1",
      model: getModelTier("C1"),
      logger,
      schemaCheck: validateLlmResponse,
    }));
  } catch (err) {
    if (err instanceof InvariantFormulationFailed) throw err;
    // Wrap parser errors / agent IO errors as InvariantFormulationFailed so
    // the orchestrator's graceful-skip path sees a typed error.
    throw new InvariantFormulationFailed(err instanceof Error ? err.message : String(err));
  }

  // Oracle #1: must return SAT.
  const { witness } = runOracleOne(formalExpression);
  logger.oracle({ id: 1, name: "Z3 SAT check (novel)", passed: witness !== null, detail: witness ? `witness obtained` : "SAT returned no witness" });

  const firstClaim: InvariantClaim = {
    principleId: null,
    description,
    formalExpression,
    bindings,
    complexity: proofComplexity(formalExpression),
    witness,
    citations,
  };

  // Oracle #1.5: fidelity check — runs only on novel-LLM path.
  const fidelity = await runInvariantFidelity({
    invariant: firstClaim,
    signal,
    llm,
    logger,
    _verifiers: args._fidelityVerifiers,
  });

  if (fidelity.passed) {
    // Stamp the post-demotion kind so downstream SMT-using oracles (especially
    // oracle #2) route by the authoritative classification rather than the
    // surface SMT shape, which lies when the LLM encodes Bool taint as Int.
    firstClaim.effectiveKind = fidelity.invariantKind ?? null;
    return firstClaim;
  }

  // One retry: feed failure detail back into the prompt.
  const retryFeedback = `\n\nPRIOR ATTEMPT FAILED ORACLE #1.5 FIDELITY CHECK:\n${fidelity.failures.join("\n")}\n\nFix the issues and produce a faithful invariant that passes all three fidelity verifiers:\n1. cross-LLM agreement (your invariant must semantically agree with an independently-derived one)\n2. traceability (every SMT clause must cite a quote from the bug report)\n3. adversarial fixtures (5 positive + 5 negative test cases must classify correctly)`;

  const retryPrompt = prompt + retryFeedback;
  logger.detail(`C1 retry prompt (oracle #1.5 failed): ${retryPrompt.slice(0, 300)}...`);

  let retry: ReturnType<typeof validateLlmResponse>;
  try {
    retry = await requestStructuredJson({
      prompt: retryPrompt,
      llm,
      stage: "C1-retry",
      model: getModelTier("C1"),
      logger,
      schemaCheck: validateLlmResponse,
    });
  } catch (err: unknown) {
    if (err instanceof InvariantFormulationFailed) throw err;
    const msg = err instanceof Error ? err.message : String(err);
    throw new InvariantFormulationFailed(
      `fidelity: retry LLM call failed — ${msg}. Original failures: ${fidelity.failures.join("; ")}`,
    );
  }

  // Oracle #1 on retry: must still be SAT.
  const { witness: retryWitness } = runOracleOne(retry.formalExpression);
  logger.oracle({ id: 1, name: "Z3 SAT check (novel retry)", passed: retryWitness !== null, detail: retryWitness ? `witness obtained` : "SAT returned no witness" });

  const retryClaim: InvariantClaim = {
    principleId: null,
    description: retry.description,
    formalExpression: retry.formalExpression,
    bindings: retry.bindings,
    complexity: proofComplexity(retry.formalExpression),
    witness: retryWitness,
    citations: retry.citations,
  };

  // Oracle #1.5 on retry: if still failing, throw.
  const retryFidelity = await runInvariantFidelity({
    invariant: retryClaim,
    signal,
    llm,
    logger,
    _verifiers: args._fidelityVerifiers,
  });

  if (retryFidelity.passed) {
    retryClaim.effectiveKind = retryFidelity.invariantKind ?? null;
    return retryClaim;
  }

  throw new InvariantFormulationFailed(
    `fidelity: ${retryFidelity.failures.join("; ")}`,
  );
}
