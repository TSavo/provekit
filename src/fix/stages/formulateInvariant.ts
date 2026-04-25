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
import type { RecognizeResult } from "./recognize.js";

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
 * fix loop itself (C4 / oracle code); `provekit analyze` builds the SAST
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
 * capability, an INSERT collision: none of these are catastrophic at the
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

/**
 * The six invariant kinds the LLM can emit. Each has a per-kind canonical
 * SMT shape and prose vocabulary. Validators downstream use this directly
 * instead of guessing via classifyInvariantKind() heuristics.
 */
type InvariantKindLabel =
  | "arithmetic"        // Int/Real bindings, equalities/inequalities, no quantifiers
  | "set_uniqueness"    // distinct-clauses or paired-equality on set elements
  | "cardinality"       // counting occurrences (prefer Bool predicates over Int counts)
  | "order"             // pairwise relations on indexed elements
  | "taint"             // Bool predicates only, no concrete numeric values
  | "other";            // doesn't fit the above; routes to abstract by default

interface LlmInvariantResponse {
  description: string;
  kind: InvariantKindLabel;
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

  return `[STAGE:C1] formulateInvariant
You are a formal verification expert. You will produce an SMT-LIB assertion
that expresses the VIOLATION STATE (the negation of the desired invariant).
The assertion must be satisfiable (Z3 check-sat returns "sat") — this proves the bug is reachable.

# Inputs

Bug summary: ${signal.summary}
Failure description: ${signal.failureDescription}
Location: ${locus.file}:${locus.line}${locus.function ? ` in ${locus.function}` : ""}

Source context:
\`\`\`
${sourceContext}
\`\`\`

Data-flow ancestors of bug site: ${locus.dataFlowAncestors.length} nodes
Data-flow descendants of bug site: ${locus.dataFlowDescendants.length} nodes

# What happens to your output

A second model will independently derive its own invariant from the same bug
report. The two are compared via cross-LLM agreement: SMT-equivalence first,
prose-similarity fallback. **Convergent phrasing matters.** If you choose
unusual variable names, exotic SMT constructs, or rare prose synonyms, the
second model will pick something different and the loop will retry.

Stick to canonical forms within your invariant's kind. There are six kinds.
**You must declare which one your invariant is.**

# Choose the kind that matches the bug

You will set \`"kind"\` in your JSON output to exactly one of:

## kind: "arithmetic"
Numeric inequality, equality, range, or remainder relation. Variables are
\`Int\` or \`Real\`. The violation is a concrete numeric state.

Use when the bug is: division-by-zero, off-by-one, integer overflow,
range-bound violation, modulo-by-zero, NaN comparison.

Canonical SMT shape (division-by-zero):
\`\`\`
(declare-const b Int)
(assert (= b 0))
\`\`\`
Canonical prose: "the divisor must not be zero", "the index must be less
than array length", "the sum must fit in 32-bit signed range".

## kind: "set_uniqueness"
The violation is "two values that should be distinct are equal" OR "an array
should have only unique elements but does not". Use \`(distinct ...)\` or
paired-equality \`(= k1 k2)\` patterns.

Use when the bug is: duplicate keys, duplicate methods in HTTP Allow,
duplicate IDs, primary-key collision, set-as-list-without-dedup.

Canonical SMT shape (Bug-1 Express duplicate-methods, "distinct" form):
\`\`\`
(declare-const m1 Int)
(declare-const m2 Int)
(declare-const m3 Int)
(assert (not (distinct m1 m2 m3)))
\`\`\`
Each \`mN\` represents the value of the N-th element in the should-be-set.
The violation is "the values are NOT all distinct" → some pair is equal.

Alternative shape (paired-equality, when only two elements matter):
\`\`\`
(declare-const m1 Int)
(declare-const m2 Int)
(assert (= m1 m2))
\`\`\`
The violation is "two distinct positions hold the same value."

Bindings: each declared constant must map to a source expression. For Bug-1
the source_expr is the position in the array (e.g. "options[0]", "options[1]").

Canonical prose: "no two X share Y", "the methods in the Allow header must
be unique", "duplicate keys are forbidden". **AVOID** "appears at most
once", "occurs more than once" — those phrasings vary across models.

## kind: "cardinality"
The violation is about the COUNT of occurrences: "X must run at least
once but ran zero times", "Y must fire at most twice but fired three
times". **Prefer Bool predicates over Int counts**, because two LLMs
will pick different counter encodings (length vs counter vs witness).

Canonical SMT shape:
\`\`\`
(declare-const x_ran_at_least_once Bool)
(assert (= x_ran_at_least_once false))
\`\`\`
Canonical prose: "must run at least once", "must fire exactly once",
"cannot exceed N retries". The Bool-predicate name carries the cardinality
relation.

## kind: "order"
The violation is about pairwise ordering: "elements should be sorted but
i < j with a[i] > a[j]", "events are out of expected sequence". Use Bool
predicates over the violation pair, not Int sequences.

Canonical SMT shape:
\`\`\`
(declare-const out_of_order_pair_exists Bool)
(assert (= out_of_order_pair_exists true))
\`\`\`
Canonical prose: "must be sorted ascending", "events must occur in
chronological order", "the result must be monotonically increasing".

## kind: "taint"
The violation is "untrusted data reaches a dangerous sink without
sanitization". All bindings are Bool predicates ABOUT the data, not
the data itself.

Canonical SMT shape (shell-injection):
\`\`\`
(declare-const input_contains_shell_metachar Bool)
(declare-const input_was_sanitized Bool)
(assert (and input_contains_shell_metachar (not input_was_sanitized)))
\`\`\`
Canonical prose: "user input must be sanitized before reaching execSync",
"untrusted X must not flow to dangerous Y".

## kind: "other"
Use only when none of the five above fit. The loop will route to the
behavioral verification path (regression test must fail on original /
pass on fixed). Be precise about why none of the five fit.

# Universal rules (apply to every kind)

1. **smt_declarations**: declare every constant you use. The runner
   appends \`(check-sat)\` automatically; do not include it.

2. **Variable names** must map to source-code identifiers when the kind
   is arithmetic. \`b\`, \`a\`, \`x\`, \`count\`, \`index\`, etc. — names
   that appear literally in the source.

3. **No synthetic control-flow variables.** Forbidden:
   \`(declare-const throws Bool)\`,
   \`(declare-const guard_returns Int)\`,
   \`(declare-const code_after_reached Bool)\`. These cannot be verified
   post-fix because oracle #2 has no way to bind them to the patched
   code's path conditions.
   ✓ Good (arithmetic): \`(assert (= b 0))\` — b is a function parameter
   ✗ Bad: \`(assert (and (= b 0) (= throws false)))\` — \`throws\` is synthetic

4. **Bindings**: every smt_constant in your declarations must appear in
   the bindings list. \`source_expr\` is the literal source-code expression
   (\`"b"\`, \`"options.length"\`, \`"input"\`).

5. **Citations**: one citation per meaningful clause in your assertion.
   \`source_quote\` must be a verbatim or close-paraphrase excerpt from
   the bug report. The citations are checked by oracle #1.5 traceability
   verifier — every clause needs prose justification.

6. **Keep it small**: 2-5 constants maximum. Prefer the simplest assertion
   that captures the violation. Complexity invites disagreement with the
   second model.

# Output format

Respond with ONLY a JSON object via the Write tool (no prose, no markdown).
The object must have exactly these fields:

\`\`\`json
{
  "description": "one sentence: what invariant is being violated, in canonical prose for your kind",
  "kind": "arithmetic" | "set_uniqueness" | "cardinality" | "order" | "taint" | "other",
  "smt_declarations": ["(declare-const varName Sort)", "..."],
  "smt_violation_assertion": "(assert (...))",
  "bindings": [
    {"smt_constant": "varName", "source_expr": "literal source code", "sort": "Int" | "Bool" | "Real"}
  ],
  "citations": [
    {"smt_clause": "(= b 0)", "source_quote": "exact or close-paraphrase from bug report"}
  ]
}
\`\`\`

# The quiet part

Two models will look at the same bug and emit invariants. If both emit
\`(declare-const b Int) (assert (= b 0))\` for division-by-zero, the
SMT-equivalence check passes instantly and the loop continues. If one
emits \`(assert (= b 0))\` and the other emits \`(assert (not (> b 0)))\`,
the equivalence check has to reason about \`b ≤ 0 ≠ b = 0\` and might
fail. The canonical examples above are the shapes both models should pick.
Pick them.`;
}

const VALID_KINDS: ReadonlySet<InvariantKindLabel> = new Set([
  "arithmetic", "set_uniqueness", "cardinality", "order", "taint", "other",
]);

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
  kind: InvariantKindLabel | null;
} {
  if (typeof rawParsed !== "object" || rawParsed === null) {
    throw new InvariantFormulationFailed("LLM response is not an object");
  }
  const parsed = rawParsed as LlmInvariantResponse;

  if (!parsed.description || typeof parsed.description !== "string") {
    throw new InvariantFormulationFailed("LLM response missing 'description' field");
  }
  // `kind` is optional for backward compat with older stubs and pre-#99 prompts.
  // When present, must be one of the six valid labels; when absent, downstream
  // classifyInvariantKind() will heuristically classify via SMT structure +
  // prose keywords. New C1 prompt (#99) instructs the LLM to emit it.
  if (parsed.kind !== undefined && parsed.kind !== null) {
    if (typeof parsed.kind !== "string" || !VALID_KINDS.has(parsed.kind as InvariantKindLabel)) {
      throw new InvariantFormulationFailed(
        `LLM response 'kind' field is invalid (got ${JSON.stringify(parsed.kind)}; ` +
        `expected one of: arithmetic, set_uniqueness, cardinality, order, taint, other, OR omitted)`,
      );
    }
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

  // Validate each binding's smt_constant appears in declarations. Accept
  // declare-const, declare-fun, and declare-sort — set_uniqueness invariants
  // commonly use uninterpreted functions to express "method of i-th element"
  // without inventing concrete representations.
  const rawBindings = (parsed.bindings ?? []) as { smt_constant: string; source_expr: string; sort: string }[];
  const declaredConsts = new Set<string>();
  for (const decl of decls) {
    const m = decl.match(/\(declare-(?:const|fun|sort)\s+(\S+)/);
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

  return {
    formalExpression,
    bindings,
    description: parsed.description,
    citations,
    kind: (parsed.kind as InvariantKindLabel | undefined) ?? null,
  };
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
  /**
   * B3 mechanical-mode input. When `matched: true`, C1m runs: instantiate the
   * recognized principle's smt2Template with locus bindings, run oracle #1,
   * return InvariantClaim with `source: "library"`. No LLM call.
   *
   * If oracle #1 fails on the instantiated SMT, log and fall back to LLM mode
   * via the existing Path 1 / Path 2 flow (per spec).
   */
  recognized?: RecognizeResult;
  /** Dependency injection for fidelity verifiers — for testing only. */
  _fidelityVerifiers?: FidelityVerifiers;
}): Promise<InvariantClaim> {
  const { signal, locus, db, llm } = args;
  const logger = args.logger ?? createNoopLogger();

  // -------------------------------------------------------------------------
  // C1m: B3 recognized path. Mechanical instantiation, no LLM, no DB requery.
  // -------------------------------------------------------------------------
  if (args.recognized && args.recognized.matched) {
    const rec = args.recognized;
    const principle = rec.principle;
    const smtTemplate = principle.smt2Template;
    if (smtTemplate) {
      // Build a captureRows array compatible with buildBindings from rec.bindings.
      const captureRows = Object.entries(rec.bindings).map(([captureName, capturedNodeId]) => ({
        captureName,
        capturedNodeId,
      }));
      const placeholders = extractPlaceholders(smtTemplate);
      const bindings = placeholders.length > 0
        ? buildBindings(db, captureRows, placeholders)
        : buildBindings(db, captureRows, captureRows.length > 0 ? [captureRows[0]!.captureName] : []);
      const formalExpression = placeholders.length > 0
        ? substituteTemplate(smtTemplate, bindings)
        : smtTemplate;

      try {
        const { witness } = runOracleOne(formalExpression);
        logger.oracle({
          id: 1,
          name: "Z3 SAT check (C1m library)",
          passed: witness !== null,
          detail: `principle=${principle.id}`,
        });
        return {
          principleId: principle.id,
          description: principle.description ?? `library: ${principle.id}`,
          formalExpression,
          bindings,
          complexity: proofComplexity(formalExpression),
          witness,
          source: "library",
        };
      } catch (err) {
        // Oracle #1 failed on instantiated template. Log and fall through to
        // existing LLM-mode behavior, per spec.
        logger.detail(
          `[C1m] WARN: oracle #1 failed for principle '${principle.id}'; falling back to LLM mode: ${
            err instanceof Error ? err.message : String(err)
          }`,
        );
      }
    } else {
      logger.detail(
        `[C1m] WARN: principle '${principle.id}' has no smt2Template; falling back to LLM mode`,
      );
    }
  }

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
  let novelKind: InvariantKindLabel | null;
  try {
    ({ formalExpression, bindings, description, citations, kind: novelKind } = await requestStructuredJson({
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
    ...(novelKind ? { llmKind: novelKind } : {}),
  };
  logger.detail(`C1 LLM-emitted kind: ${novelKind ?? "(omitted; will fall back to heuristic)"}`);

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
    ...(retry.kind ? { llmKind: retry.kind } : {}),
  };
  logger.detail(`C1 retry LLM-emitted kind: ${retry.kind ?? "(omitted; will fall back to heuristic)"}`);

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
