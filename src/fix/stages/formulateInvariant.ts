/**
 * C1: Invariant formulator.
 *
 * Given a BugSignal and BugLocus, produces a Z3-checkable InvariantClaim.
 * Oracle #1 fires inside this function — every returned InvariantClaim has
 * been verified SAT by Z3. Principle-match path is tried before LLM path.
 */

import { readFileSync, existsSync } from "fs";
import { basename, join, dirname } from "path";
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
import { enumeratePrincipleFiles } from "../../principleEnumeration.js";
import type { RecognizeResult } from "./recognize.js";
import { getPromptStore } from "../../llm/promptStore.js";

// ---------------------------------------------------------------------------
// Prompt fragments (better-prompts artifacts).
//
// Each named const here is a static prompt fragment that lives ALSO as a
// bp artifact under the namespace shown in the comment above it. The literal
// is the source-of-record; bp.get() returns it byte-identically until the
// fragment is evolved. Day 0: nothing changes. Day N: any single fragment
// can be evolved without touching the surrounding assembly.
//
// Discriminator pattern: ISO date that bumps each time the literal here is
// edited. A bump tells bp "this is a new revision; stop returning the
// previously-evolved body, the source-of-record has advanced."
// ---------------------------------------------------------------------------

// bp namespace: c1.persona
const C1_PERSONA = `You are a formal verification expert. You will produce an SMT-LIB assertion
that expresses the VIOLATION STATE (the negation of the desired invariant).
The assertion must be satisfiable (Z3 check-sat returns "sat") — this proves the bug is reachable.`;

// bp namespace: c1.cross_llm_agreement
const C1_CROSS_LLM_AGREEMENT = `# What happens to your output

A second model will independently derive its own invariant from the same bug
report. The two are compared via cross-LLM agreement: SMT-equivalence first,
prose-similarity fallback. **Convergent phrasing matters.** If you choose
unusual variable names, exotic SMT constructs, or rare prose synonyms, the
second model will pick something different and the loop will retry.

Stick to canonical forms within your invariant's kind. There are six kinds.
**You must declare which one your invariant is.**`;

// bp namespace: c1.kind.taint
const C1_KIND_TAINT = `## kind: "taint"
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
"untrusted X must not flow to dangerous Y".`;

// bp namespace: c1.kind.other
const C1_KIND_OTHER = `## kind: "other"
Use only when none of the five above fit. The loop will route to the
behavioral verification path (regression test must fail on original /
pass on fixed). Be precise about why none of the five fit.`;

// bp namespace: c1.quiet_part
const C1_QUIET_PART = `# The quiet part

Two models will look at the same bug and emit invariants. If both emit
\`(declare-const b Int) (assert (= b 0))\` for division-by-zero, the
SMT-equivalence check passes instantly and the loop continues. If one
emits \`(assert (= b 0))\` and the other emits \`(assert (not (> b 0)))\`,
the equivalence check has to reason about \`b ≤ 0 ≠ b = 0\` and might
fail. The canonical examples above are the shapes both models should pick.
Pick them.`;

// bp namespace: c1.kind.arithmetic
const C1_KIND_ARITHMETIC = `## kind: "arithmetic"
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
than array length", "the sum must fit in 32-bit signed range".`;

// bp namespace: c1.kind.set_uniqueness
const C1_KIND_SET_UNIQUENESS = `## kind: "set_uniqueness"
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
once", "occurs more than once" — those phrasings vary across models.`;

// bp namespace: c1.kind.cardinality
const C1_KIND_CARDINALITY = `## kind: "cardinality"
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
relation.`;

// bp namespace: c1.kind.order.intro
const C1_KIND_ORDER_INTRO = `## kind: "order"
The violation is about pairwise ordering: "elements should be sorted but
i < j with a[i] > a[j]", "events are out of expected sequence". Use Bool
predicates over the violation pair, not Int sequences.`;

// bp namespace: c1.kind.order.canonical_prose
const C1_KIND_ORDER_CANONICAL_PROSE = `Canonical prose: "the result must include the K most recent entries",
"elements must be in descending chronological order",
"the query must use descending ordering for the relevant column".`;

// bp namespace: c1.kind.order.polarity_convention
const C1_KIND_ORDER_POLARITY_CONVENTION = `**Polarity convention (load-bearing — read before writing SMT):**

The path-checker that runs during verify scans the source line for
\`asc(\` / \`desc(\` and pins ALL Bool bindings using this rule:

  - \`desc(...)\` found on path → binding pinned to **true**
  - \`asc(...)\` found on path → binding pinned to **false**

So "true" means "the correct/spec-compliant ordering is used". The constant
name MUST reflect the spec state (what is true when the code is correct),
not the bug state (what is true when the code is broken). If the constant
name implies the WRONG polarity, future readers and future LLMs will emit
inverted polarity and the verify results will be wrong.

Canonical SMT shape (spec-flavored constant, asserted true):
\`\`\`
(declare-const result_returns_k_most_recent Bool)
(assert (= result_returns_k_most_recent true))
\`\`\`
Polarity walkthrough (asc/desc dogfood example):
- Bug code (\`.orderBy(asc(schema.invocations.date))\`):
    path-checker pins \`result_returns_k_most_recent = false\`
    negated-invariant = \`(not (= result_returns_k_most_recent true))\` = true
    Z3 SAT (false satisfies both pin and negated-invariant) → **violated** ✓
- Fixed code (\`.orderBy(desc(schema.invocations.date))\`):
    path-checker pins \`result_returns_k_most_recent = true\`
    negated-invariant = \`(not (= result_returns_k_most_recent true))\` = false
    Z3 UNSAT → **holds** ✓

BAD (bug-flavored name — DO NOT use this shape):
\`\`\`
declarations: ["(declare-const recent_invocations_excluded_by_asc_limit Bool)"]
assertion: "(assert (= recent_invocations_excluded_by_asc_limit true))"
\`\`\`
Why bad: the constant name says "excluded" (the bug condition), but the
assertion says it equals true. The path-checker is name-agnostic and pins
on asc/desc presence. This creates a naming mismatch that confuses future
LLMs and human readers: they see a bug-flavored name asserted true and
assume the polarity is correct when the convention is inverted.
Use a spec-flavored name (what is true when the code is correct) so the
naming convention is consistent with the path-checker's pin rule.`;
import type { InvestigateReport } from "./investigate.js";

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
  // Task #134: principle library is partitioned. Walk every partition
  // (loadAllPartitions=true) and locate by filename — a C1 lookup by id
  // doesn't know which language partition the principle landed in.
  const principlesDir = findPrinciplesDir();
  if (!existsSync(principlesDir)) return null;
  const { jsonPaths } = enumeratePrincipleFiles(principlesDir, {
    loadAllPartitions: true,
  });
  const filename = `${principleName}.json`;
  const jsonPath = jsonPaths.find((p) => basename(p) === filename);
  if (!jsonPath) return null;
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

  // Task #134: principle library is partitioned. Walk every partition
  // (loadAllPartitions=true) — same rationale as recognize.ts: principle
  // evaluation runs against any incoming locus regardless of project
  // language, and we want every applicable principle's matches in the table.
  const { dslPaths } = enumeratePrincipleFiles(principlesDir, {
    loadAllPartitions: true,
  });

  const t0 = Date.now();
  let evaluatedCount = 0;
  for (const dslPath of dslPaths) {
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
        `[C1] principle ${basename(dslPath)} evaluation skipped: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }
  logger.detail(`[C1] populated principleMatches from ${evaluatedCount}/${dslPaths.length} DSL files in ${Date.now() - t0}ms`);
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

/**
 * Read the full source of the locus file, preferring the SAST-recorded path
 * (resolved via locus.primaryNode → files table) and falling back to
 * locus.file when that lookup is empty (novel-LLM-path tests use synthetic
 * primaryNode IDs that are not in the SAST graph).
 *
 * Returns the empty string when neither path resolves a readable file. The
 * source_expr substring-match gate treats empty source as "cannot validate"
 * and allows whatever the LLM emits — matching the v1 contract that gating
 * is a guarantee tightener, not a fabrication when context is missing.
 */
function readLocusSource(db: Db, locus: BugLocus): string {
  // Path 1: SAST-recorded path (canonical, present when nodes were indexed).
  try {
    const fileIdRow = db
      .select({ fileId: nodes.fileId })
      .from(nodes)
      .where(eq(nodes.id, locus.primaryNode))
      .get();
    if (fileIdRow?.fileId !== undefined) {
      const fileRow = db
        .select({ path: filesTable.path })
        .from(filesTable)
        .where(eq(filesTable.id, fileIdRow.fileId))
        .get();
      if (fileRow?.path && existsSync(fileRow.path)) {
        return readFileSync(fileRow.path, "utf-8");
      }
    }
  } catch {
    // fall through to path-2 fallback
  }
  // Path 2: locus.file (synthetic-node novel-LLM-path tests).
  try {
    if (locus.file && existsSync(locus.file)) {
      return readFileSync(locus.file, "utf-8");
    }
  } catch {
    // ignore — empty source disables the substring gate (documented contract).
  }
  return "";
}

async function buildLlmPrompt(signal: BugSignal, locus: BugLocus, db: Db, locusSource: string, investigate?: InvestigateReport, projectRoot?: string): Promise<string> {
  // Resolve evolvable prompt fragments at their interpolation sites.
  // Each fragment is a bp artifact named after its position; bp.get returns
  // the literal byte-identically until the fragment is evolved. When
  // projectRoot is unavailable (test contexts, dry-run), the literal is
  // used directly — same byte sequence either way on day 0.
  const fetch = async (key: string, literal: string): Promise<string> =>
    projectRoot
      ? (await getPromptStore(projectRoot).get(key, literal, "2026-04-28")).body
      : literal;
  const c1Persona = await fetch("c1.persona", C1_PERSONA);
  const c1CrossLlmAgreement = await fetch("c1.cross_llm_agreement", C1_CROSS_LLM_AGREEMENT);
  const c1KindTaint = await fetch("c1.kind.taint", C1_KIND_TAINT);
  const c1KindOther = await fetch("c1.kind.other", C1_KIND_OTHER);
  const c1QuietPart = await fetch("c1.quiet_part", C1_QUIET_PART);
  const c1KindArithmetic = await fetch("c1.kind.arithmetic", C1_KIND_ARITHMETIC);
  const c1KindSetUniqueness = await fetch("c1.kind.set_uniqueness", C1_KIND_SET_UNIQUENESS);
  const c1KindCardinality = await fetch("c1.kind.cardinality", C1_KIND_CARDINALITY);
  const c1KindOrderIntro = await fetch("c1.kind.order.intro", C1_KIND_ORDER_INTRO);
  const c1KindOrderCanonicalProse = await fetch("c1.kind.order.canonical_prose", C1_KIND_ORDER_CANONICAL_PROSE);
  const c1KindOrderPolarityConvention = await fetch(
    "c1.kind.order.polarity_convention",
    C1_KIND_ORDER_POLARITY_CONVENTION,
  );
  // Gather source context around locus from the already-loaded full source.
  let sourceContext = "(source not available)";
  if (locusSource) {
    const lines = locusSource.split("\n");
    // When locus.line is at the file top (≤5), the substrate resolved to a
    // SourceFile node rather than a function body — the ±3-line window shows
    // only imports, misleading the LLM into picking import strings as
    // source_expr anchors. Show the full file (capped at 200 lines) so the
    // LLM can use the investigate block + its own reading to locate the
    // actual bug site and pick a call-expression-shaped source_expr.
    let start: number;
    let end: number;
    if (locus.line <= 5) {
      start = 0;
      end = Math.min(lines.length, 200);
    } else {
      start = Math.max(0, locus.line - 3);
      end = Math.min(lines.length, locus.line + 2);
    }
    sourceContext = lines.slice(start, end).map((l, i) => `${start + i + 1}: ${l}`).join("\n");
  }

  // Investigate-derived evidence (when symptom-only flow ran). The reasoning
  // section below cites these directly so the LLM sees the upstream chain
  // and can self-calibrate on whether the locus is decisive or speculative.
  const investigateBlock = investigate
    ? `\nInvestigate's analysis (the upstream stage that scanned the project to find this locus):
- Primary location: ${investigate.primaryLocation.file}${investigate.primaryLocation.function ? ` (${investigate.primaryLocation.function})` : ""} — ${investigate.primaryLocation.confidence} confidence
  Rationale: ${investigate.primaryLocation.rationale}
- Root-cause hypothesis: ${investigate.rootCauseHypothesis}
- Fix hypothesis (shape, not exact text): ${investigate.fixHypothesis}
- Other candidate locations Investigate considered: ${investigate.candidateLocations.length}${investigate.candidateLocations.length > 0 ? `\n${investigate.candidateLocations.map((c) => `  - ${c.file}${c.function ? ` (${c.function})` : ""} — ${c.confidence}`).join("\n")}` : ""}
`
    : "";

  return `[STAGE:C1] formulateInvariant
${c1Persona}

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
${investigateBlock}
# Invariant strength: the Goldilocks zone

The invariant you write is the only formal gate between a real fix and a
placebo. Z3 will accept any patch that satisfies your invariant. If the
invariant is too narrow, downstream patches can satisfy it WITHOUT
addressing the root cause — the loop ships a placebo. If the invariant is
too broad, it false-positives on legitimate code paths and the loop never
ships at all.

## Mandatory self-check before you write SMT

Before you commit your invariant to JSON, ask yourself ONE question:

  *"Can a downstream JavaScript post-processing step (a .sort, a .filter,
  a .slice on already-fetched data) satisfy this invariant WITHOUT changing
  the upstream code that decided what data was fetched?"*

If yes, your invariant is too narrow. It admits a placebo patch — a sort
applied to data that was already truncated by the bug satisfies the local
invariant ("the result is sorted") while leaving the truncation in place.
The loop will ship the placebo, oracle #9a (test must reproduce at scale)
will reject it, the loop will fail.

The fix is to widen the invariant's scope to quantify over the data flow,
not the consequent state. A correctly-scoped invariant references the
SOURCE of the data ("the data fetched from storage includes…") rather than
the result of the consumer's transformation ("the value passed to the
function is…"). The data-source invariant cannot be satisfied by a
downstream sort/filter; the consequent-state invariant trivially can.

Worked examples below make this concrete. Apply the self-check after
reading them and before writing your SMT.

Reason about three calibration tiers BEFORE you write SMT. Worked examples:

## Example 1 — division-by-zero in \`function divide(a, b) { return a / b }\`
- Too narrow: "result of \`a / b\` is finite when \`b !== 0\`"
  Why narrow: only quantifies over this exact expression. A patch that
  catches NaN downstream and returns 0 satisfies it without guarding b.
- Too broad: "all integer divisions in this codebase guard against zero"
  Why broad: false-positive on \`x / 2\` where 2 is a literal — no guard
  needed. The principle would fire on legitimate code.
- Right: "for every call to \`divide(a, b)\`, b !== 0 must be reachable
  before the division executes"
  Why right: forces the fix at the data flow into the divisor. Catches
  the bug at every call site without false-positive on b-as-literal.

## Example 2 — telemetry-feeding query orders ASC instead of DESC
(this is the kind of bug that motivated this prompt rewrite — a placebo
fix at the consumer site can sort the truncated-old data and satisfy a
narrow invariant while leaving the root cause intact)
- Too narrow: "the exemplar passed to evolve is the most-recent failing
  invocation"
  Why narrow: a JS-side \`telemetry.sort((a,b) => b.date - a.date)\` after
  the query satisfies it. But the query already truncated to oldest 25;
  the sort has no recent data to surface. Placebo passes.
- Too broad: "every \`forRevision\` call returns desc-ordered rows"
  Why broad: an audit/history surface that legitimately wants chronological
  order would now be a violation. False positive.
- Right: "data REACHING the evolve meta-prompt includes the K most-recent
  invocations from the revision's full history (not just the most-recent
  K of those already returned)"
  Why right: forces the fix at the data source for the evolve call path.
  An audit caller with a different consumer doesn't trigger it.

## The principle (generalize from the examples)

A strong invariant constrains the smallest scope of code such that **no
narrower patch can satisfy the invariant while leaving the symptom intact**.
That scope is rarely the bug expression itself; it is usually the data
flow into the bug site.

Three questions to answer in your prose description (cited verbatim by
downstream stages):
1. **Where in the data flow** must the invariant hold for the symptom to
   actually be prevented? (Not just where the bug fires, but where the
   data shape is decided.)
2. **What patches at narrower scopes** would technically satisfy the
   invariant without addressing the root cause? Name at least one
   placebo and explain why your invariant rejects it.
3. **What contexts** would your invariant false-positive on if you
   stated it more broadly? Name at least one legitimate caller and
   explain why your scope respects it.

If you cannot answer (2) or (3), your invariant is either too narrow
(no placebos to reject) or too broad (no legitimate callers to
respect). Re-formulate.

${c1CrossLlmAgreement}

# Choose the kind that matches the bug

You will set \`"kind"\` in your JSON output to exactly one of:

${c1KindArithmetic}

${c1KindSetUniqueness}

${c1KindCardinality}

${c1KindOrderIntro}

${c1KindOrderPolarityConvention}

${c1KindOrderCanonicalProse}

${c1KindTaint}

${c1KindOther}

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

   == source_expr CONTRACT (load-bearing for downstream verification) ==

   For EVERY binding you emit, \`source_expr\` MUST be a verbatim substring
   of the locus file's source code AND it must appear on EXACTLY ONE LINE
   in that file (so it uniquely locates the bug site).

   Downstream verification substring-searches the patched file for this
   exact string to populate real binding line numbers. If the substring
   appears on more than one line (for example, a bare function name that
   shows up on the import line AND the bug call site), the binding collapses
   to the FIRST occurrence (the import line), not the bug site. That produces
   wrong geometry and the invariant decays on every subsequent verify run.

   UNIQUENESS RULE: a bare identifier like \`"asc"\` is UNACCEPTABLE when
   it appears in more than one place in the file (e.g. the import line
   \`import { asc, desc } from "drizzle-orm"\` at line 1 AND the bug site
   \`.orderBy(asc(schema.invocations.date))\` at line 120). The binding
   maps to line 1, not line 120 — wrong geometry.

   The RIGHT value is the smallest substring that UNIQUELY IDENTIFIES the
   bug location. Prefer:
   - A full call expression with arguments: \`"asc(schema.invocations.date)"\`
   - A property access chain: \`"schema.invocations.date"\`
   - A method-call expression: \`".orderBy(asc("\`

   A bare identifier is acceptable ONLY when it appears on exactly one line
   in the entire file.

   Examples of VALID source_expr (verbatim substrings, unique location):
   - \`"asc(schema.invocations.date)"\`  ← call expression, unique
   - \`"len < 15"\`                       ← comparison, unique
   - \`"buf[i]"\`                         ← indexed access, unique
   - \`"memo->buffer"\`                   ← field access, unique
   - \`"divisor"\`                        ← identifier, IF it appears only once

   Examples of INVALID source_expr:
   - \`"asc"\`  ← appears on import line AND multiple call sites; NOT unique
   - \`"feedback rows fetched by repositories.ts for evolve"\` ← PROSE, not code
   - \`"the buffer length"\`              ← PROSE, not code
   - \`"the user input before sanitization"\` ← PROSE, not code
   - \`"divisor parameter of divide"\`   ← PROSE, not code

   If you cannot identify a verbatim substring in the locus file that
   captures the SMT constant's binding, take this escape hatch:

   - widen the smt_declaration to a Bool predicate (which doesn't need
     a source-code anchor — the predicate name encodes the meaning).

   You MUST emit at least one binding. An invariant with \`"bindings": []\`
   has no per-binding contract to enforce and is rejected at C1 exit; do
   not return one. The Bool-predicate escape hatch above is the correct
   shape for invariants whose constants do not have source-code anchors.

   Prose \`source_expr\` is a generation bug. The validator runs an exact
   substring check against the locus file before this invariant proceeds
   to oracle #1; if any binding's source_expr is not in the source, the
   prompt is replayed with sharper feedback once and then the formulation
   fails hard. Pick a real substring or use the Bool-predicate escape hatch.

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

${c1QuietPart}`;
}

const VALID_KINDS: ReadonlySet<InvariantKindLabel> = new Set([
  "arithmetic", "set_uniqueness", "cardinality", "order", "taint", "other",
]);

// ---------------------------------------------------------------------------
// source_expr substring-match gate (task #142 — same architectural pattern
// as #140's grammar-aware C5: convert an open prompt to a multiple-choice one
// by ruling out every answer outside the legal grammar — here, the grammar of
// source_expr is "verbatim substring of the locus file's source code").
// ---------------------------------------------------------------------------

/**
 * Check that every binding's `source_expr` is a verbatim substring of the
 * locus file's source AND that it appears on exactly one distinct line (so
 * it uniquely locates the bug site rather than collapsing to an earlier
 * occurrence like an import line).
 *
 * Returns the list of offending bindings (empty when all bindings pass).
 * Each offender carries a `reason` discriminator ("not-found" | "ambiguous")
 * and, for the "ambiguous" case, the line numbers where the substring appears.
 *
 * Empty bindings array is the documented escape hatch (intent-level
 * invariant) and passes trivially. Empty `locusSource` (when the locus
 * file could not be read) also passes — the gate is a guarantee
 * tightener, not a fabrication when context is unavailable.
 *
 * Trims `source_expr` before comparison to absorb stray whitespace from
 * LLM tokenization without giving up substring fidelity (the orchestrator's
 * downstream substring-search at flush time uses the same trim).
 */
export function findInvalidSourceExprBindings(
  bindings: SmtBindingRef[],
  locusSource: string,
): Array<{ smt_constant: string; source_expr: string; reason?: "not-found" | "ambiguous"; lines?: number[] }> {
  if (bindings.length === 0) return [];
  if (locusSource.length === 0) return [];
  const sourceLines = locusSource.split("\n");
  const offenders: Array<{ smt_constant: string; source_expr: string; reason?: "not-found" | "ambiguous"; lines?: number[] }> = [];
  for (const b of bindings) {
    const trimmed = (b.source_expr ?? "").trim();
    if (trimmed.length === 0) {
      // Empty source_expr is also a contract violation (every non-empty
      // bindings entry must anchor to source). The retry feedback below
      // calls this out specifically.
      offenders.push({ smt_constant: b.smt_constant, source_expr: b.source_expr ?? "", reason: "not-found" });
      continue;
    }
    if (!locusSource.includes(trimmed)) {
      offenders.push({ smt_constant: b.smt_constant, source_expr: b.source_expr, reason: "not-found" });
      continue;
    }
    // Multi-line uniqueness check: if the substring appears on more than one
    // distinct line, a binding to it can't be located — it collapses to the
    // first match (often an import line) rather than the actual bug site.
    // A bare identifier like "asc" that appears in imports, parameter names,
    // AND the bug call-expression is the canonical failure mode this catches.
    const matchingLines = sourceLines
      .map((line, idx) => (line.includes(trimmed) ? idx + 1 : 0))
      .filter((n) => n > 0);
    if (matchingLines.length > 1) {
      offenders.push({ smt_constant: b.smt_constant, source_expr: b.source_expr, reason: "ambiguous", lines: matchingLines });
    }
  }
  return offenders;
}

/**
 * Build a sharper feedback string for the C1 retry when the substring gate
 * caught prose or ambiguous source_expr. Includes the offending bindings
 * verbatim so the LLM can see exactly which constants need a real unique
 * substring (or an escape hatch).
 */
function buildSourceExprRetryFeedback(
  offenders: Array<{ smt_constant: string; source_expr: string; reason?: "not-found" | "ambiguous"; lines?: number[] }>,
): string {
  const lines = offenders.map((o) => {
    if (o.reason === "ambiguous" && o.lines && o.lines.length > 0) {
      return (
        `  - smt_constant="${o.smt_constant}", source_expr=${JSON.stringify(o.source_expr)}` +
        ` appears on lines ${o.lines.join(", ")} in the locus file — it is NOT unique.` +
        ` A bare identifier like this matches the import line, not the bug site.` +
        ` Emit a more specific substring that uniquely identifies the bug location,` +
        ` typically a full call expression with arguments (e.g., "asc(schema.invocations.date)"` +
        ` instead of "asc"), a property access chain, or a method-call expression.`
      );
    }
    return (
      `  - smt_constant="${o.smt_constant}", source_expr=${JSON.stringify(o.source_expr)}` +
      ` is NOT a verbatim substring of the locus file's source code.`
    );
  });
  return `\n\nPRIOR ATTEMPT VIOLATED THE source_expr CONTRACT:\n${lines.join("\n")}\n\n` +
    `For each offending binding, either:\n` +
    `  1. Pick a verbatim substring that uniquely identifies the bug location (appears on exactly ONE line) — prefer call expressions with arguments over bare identifiers.\n` +
    `  2. Widen the SMT declaration to a Bool predicate (which doesn't need a source-code anchor).\n` +
    `A bare identifier (like a function name) is only acceptable when it appears on exactly one line in the file.`;
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

/**
 * Public C1 entry point. Wraps the inner formulator and enforces the
 * "at least one binding" structural contract (#147a) at exit, regardless
 * of which path inside (C1m / principle-match / novel-LLM) produced the
 * claim. An InvariantClaim with `bindings.length === 0` has no per-
 * binding verbatim-substring contract (#142) to enforce and is therefore
 * structurally meaningless; refusing it here keeps the corpus clean and
 * prevents downstream stages from operating on nothing.
 */
export async function formulateInvariant(args: {
  signal: BugSignal;
  locus: BugLocus;
  db: Db;
  llm: LLMProvider;
  /**
   * Host project root, optional. When provided, the C1 prompt fragments
   * resolve via better-prompts (each named const here lives also as a bp
   * artifact, byte-identical day 0, evolvable day N). When absent, the
   * literal source-of-record is used directly — same content either way.
   */
  projectRoot?: string;
  logger?: FixLoopLogger;
  recognized?: RecognizeResult;
  investigateReport?: InvestigateReport;
  _fidelityVerifiers?: FidelityVerifiers;
}): Promise<InvariantClaim> {
  const claim = await formulateInvariantInner(args);
  if (claim.bindings.length === 0) {
    throw new InvariantFormulationFailed(
      `C1 must emit at least one binding (got bindings: []). An invariant ` +
        `with no bindings has no per-binding verbatim-substring contract to ` +
        `enforce and is structurally meaningless. Use a Bool-predicate escape ` +
        `hatch instead — declare a Bool constant whose name encodes the ` +
        `meaning, and bind it to a verbatim substring of the locus source.`,
    );
  }
  return claim;
}

async function formulateInvariantInner(args: {
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
  /**
   * Investigate's report when symptom-only bug-report flow ran. C1 cites
   * `rootCauseHypothesis` and `fixHypothesis` directly in the LLM prompt
   * so the formulated invariant is strong enough to rule out the root
   * cause — not just a narrow consequent-state claim a placebo patch can
   * satisfy. Undefined when Intake produced clean code references.
   */
  investigateReport?: InvestigateReport;
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

  // Read the full locus source ONCE up-front. Used both for the prompt's
  // ±3-line context and for the source_expr substring-match gate (task #142).
  const locusSource = readLocusSource(db, locus);

  const prompt = await buildLlmPrompt(signal, locus, db, locusSource, args.investigateReport, args.projectRoot);
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

  // -------------------------------------------------------------------------
  // Task #142: source_expr substring-match gate.
  //
  // C1's emitted bindings drive the orchestrator's persistence flush, which
  // substring-searches the patched file's content for each binding's
  // source_expr to populate real binding line numbers. Prose source_expr
  // never substring-matches and produces honest-zero geometry that decays
  // the invariant on every subsequent verify run.
  //
  // The fix is the same architectural pattern as #140's grammar-aware C5:
  // convert an open prompt to a multiple-choice one by ruling out every
  // answer outside the legal grammar (here, "verbatim substring of the
  // locus file's source"). The prompt now states the contract explicitly
  // (with valid + invalid examples + escape hatches); this gate enforces
  // it, with one sharpened-feedback retry before failing hard.
  // -------------------------------------------------------------------------
  let invalidBindings = findInvalidSourceExprBindings(bindings, locusSource);
  if (invalidBindings.length > 0) {
    logger.detail(
      `C1: source_expr gate caught prose bindings on first attempt — retrying once: ${
        invalidBindings.map((o) => o.smt_constant).join(", ")
      }`,
    );
    const exprRetryPrompt = prompt + buildSourceExprRetryFeedback(invalidBindings);
    let exprRetry: ReturnType<typeof validateLlmResponse>;
    try {
      exprRetry = await requestStructuredJson({
        prompt: exprRetryPrompt,
        llm,
        stage: "C1-source-expr-retry",
        model: getModelTier("C1"),
        logger,
        schemaCheck: validateLlmResponse,
      });
    } catch (err) {
      if (err instanceof InvariantFormulationFailed) throw err;
      throw new InvariantFormulationFailed(
        `source_expr gate retry LLM call failed — ${err instanceof Error ? err.message : String(err)}`,
      );
    }
    invalidBindings = findInvalidSourceExprBindings(exprRetry.bindings, locusSource);
    if (invalidBindings.length > 0) {
      throw new InvariantFormulationFailed(
        `source_expr contract violated after retry — bindings have prose, not verbatim source substrings: ${
          invalidBindings
            .map((o) => `${o.smt_constant}=${JSON.stringify(o.source_expr)}`)
            .join("; ")
        }`,
      );
    }
    formalExpression = exprRetry.formalExpression;
    bindings = exprRetry.bindings;
    description = exprRetry.description;
    citations = exprRetry.citations;
    novelKind = exprRetry.kind;
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
    investigateReport: args.investigateReport,
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

  // Task #142: re-apply the source_expr substring gate on the oracle-#1.5
  // retry response. The retry budget is already spent (single retry per
  // upstream constraint), so any prose source_expr here fails hard. Lower-
  // probability path (the LLM passed the gate on the first attempt and is
  // only retrying because fidelity failed) but the contract is universal.
  const retryInvalidBindings = findInvalidSourceExprBindings(retry.bindings, locusSource);
  if (retryInvalidBindings.length > 0) {
    throw new InvariantFormulationFailed(
      `source_expr contract violated on oracle-#1.5 retry — bindings have prose, not verbatim source substrings: ${
        retryInvalidBindings
          .map((o) => `${o.smt_constant}=${JSON.stringify(o.source_expr)}`)
          .join("; ")
      }`,
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
    investigateReport: args.investigateReport,
  });

  if (retryFidelity.passed) {
    retryClaim.effectiveKind = retryFidelity.invariantKind ?? null;
    return retryClaim;
  }

  throw new InvariantFormulationFailed(
    `fidelity: ${retryFidelity.failures.join("; ")}`,
  );
}
