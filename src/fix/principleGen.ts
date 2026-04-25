/**
 * C6: Happy-path principle generation (existing capabilities path).
 *
 * Exports:
 *   tryExistingCapabilities — asks LLM to express invariant in current DSL
 *   proposeWithCapability   — substrate-extension path (delegates to capabilityGen.ts)
 *   runAdversarialValidation — oracle #6 (shared by both paths)
 */

import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");
import * as ts from "typescript";
import { openDb } from "../db/index.js";
import { buildSASTAndReturnHandles, buildSASTForFile } from "../sast/builder.js";
import { parseDSL } from "../dsl/parser.js";
import { compileProgram, CompileError } from "../dsl/compiler.js";
import {
  listCapabilities,
  registerCapability,
  unregisterCapability,
} from "../sast/capabilityRegistry.js";
import { listRelations } from "../dsl/relationRegistry.js";
import { nodes as nodesTable, files as filesTable } from "../sast/schema/nodes.js";
import { eq } from "drizzle-orm";
import type { Db } from "../db/index.js";
import type {
  BugSignal,
  InvariantClaim,
  FixCandidate,
  PrincipleCandidate,
  LLMProvider,
  OverlayHandle,
} from "./types.js";
import type { CapabilitySpec } from "./types.js";
import { proposeCapabilitySpec, runSubstrateOracles } from "./capabilityGen.js";
import { requestStructuredJson } from "./llm/structuredOutput.js";
import { getModelTier } from "./modelTiers.js";

// ---------------------------------------------------------------------------
// Internal result shape for tryExistingCapabilities
// ---------------------------------------------------------------------------

export type ExistingCapAttempt =
  | { kind: "ok"; principles: PrincipleCandidate[] }
  | { kind: "capability_gap"; gap: string }
  | { kind: "non_codifiable" };

// ---------------------------------------------------------------------------
// Adversarial fixture for oracle #6
// ---------------------------------------------------------------------------

/** A single fixture for adversarial validation. */
export interface AdversarialFixture {
  /** TypeScript source of the fixture file. */
  source: string;
  /**
   * Expected match count:
   *   - false-positive test: 0 expected (good code should NOT match)
   *   - false-negative test: >0 expected (buggy code MUST match)
   */
  expectedMatchCount: number;
}

// ---------------------------------------------------------------------------
// Prompt helpers
// ---------------------------------------------------------------------------

function describeCapabilities(): string {
  const caps = listCapabilities();
  if (caps.length === 0) return "(no capabilities registered)";
  return caps.map((c) => {
    const cols = Object.values(c.columns).map((col) => {
      const sort = col.sort ?? (col.isNodeRef ? "node" : "?");
      const enumSuffix = col.kindEnum ? ` in [${col.kindEnum.join(", ")}]` : "";
      return `    ${col.dslName} (${sort})${enumSuffix}`;
    }).join("\n");
    return `- ${c.dslName}\n${cols}`;
  }).join("\n");
}

function describeRelations(): string {
  const rels = listRelations();
  if (rels.length === 0) return "(no relations registered)";
  return rels.map((r) =>
    `- ${r.name}(${r.paramTypes.join(", ")})`
  ).join("\n");
}

function loadExemplar(): string {
  const exemplarPath = join(__dirname, "..", "..", ".provekit", "principles", "division-by-zero.dsl");
  try {
    return readFileSync(exemplarPath, "utf-8");
  } catch {
    return "(exemplar not available)";
  }
}

export function buildPrinciplePrompt(
  signal: BugSignal,
  invariant: InvariantClaim,
  fix: FixCandidate,
): string {
  const exemplar = loadExemplar();

  return `You are a static-analysis rule author. Given a bug and its fix, produce a reusable DSL principle that catches this bug class.

Bug summary: ${signal.summary}
Invariant violated: ${invariant.description}
Bug class hint: ${signal.bugClassHint ?? "(none)"}
Fix description: ${fix.patch.description}

=== THREE NAME-SPACES IN THE DSL. DO NOT MIX THEM. ===

A. CAPABILITIES: These are schema tables. Reference their columns as
   capabilityName.columnName (e.g., arithmetic.op, narrows.target_node).
   NEVER use a capability name as a predicate name in a require clause.
   Available capabilities (from the runtime registry):
${describeCapabilities()}

B. BUILT-IN RELATIONS: These ARE callable in where clauses inside require
   clauses. They take two arguments (varRef or varDeref).
   Available relations (from the runtime registry):
${describeRelations()}
   Example: where same_value($guard.narrows.target_node, $div.arithmetic.rhs_node)

C. USER-DEFINED PREDICATES: You can declare your own with
   predicate name($arg: node) { match ... }
   Then call them from a require no clause. Use this for reusable match logic.
   The predicate name MUST NOT collide with any capability or relation name above.

=== EXEMPLAR: your output should be of similar shape ===

${exemplar}

=== END EXEMPLAR ===

=== MULTI-SHAPE PRINCIPLES (PITCH-LEAK 3 LAYER 1) ===

The same bug class often manifests in multiple syntactic forms. Example:
the "division-by-zero" bug class can appear as:
  - canonical:  a / b           (binary "/" with unguarded rhs)
  - alternate:  a % b           (modulo also throws on zero)
  - alternate:  Math.floor(a/b) (wrapped in a call expression)

Each alternate shape matches a DIFFERENT AST pattern but is the SAME bug.
Without alternates, the principle library only catches the canonical form;
real-world variants slip past.

You MUST produce 1 to 3 principles for this bug class:
- the FIRST principle is the canonical shape that matches the reported bug.
- ADDITIONAL principles (0, 1, or 2 more) are syntactically distinct shapes
  of the SAME bug class. Add an alternate shape ONLY if it is materially
  different in AST structure. Do NOT pad with redundant variants.
- ALL shapes share the same \`bugClassId\` slug. Names must be unique.

If you can express the invariant using ONLY the capabilities above, respond:
{
  "kind": "principles",
  "bugClassId": "division-by-zero",
  "principles": [
    {
      "name": "UniquePrincipleName",
      "dslSource": "<full DSL source>",
      "smtTemplate": "<SMT-LIB template with {{placeholders}}>",
      "teachingExample": {
        "domain": "arithmetic",
        "explanation": "...",
        "smt2": "(declare-const x Int)\\n(assert (= x 0))\\n(check-sat)"
      }
    }
  ]
}

The "principles" array MUST contain 1 to 3 entries. Each entry has its own
DSL source, SMT template, and teaching example. The bugClassId at the top
applies to all entries. Each principle is compiled and adversarially
validated independently; valid shapes are kept, invalid shapes are dropped.

If you CANNOT express it without a new capability, respond:
{
  "kind": "needs_capability",
  "missing_predicate": "description of the missing structural predicate"
}

If the invariant is non-codifiable as a static rule (e.g., too runtime-specific), respond:
{
  "kind": "non_codifiable",
  "reason": "..."
}

Legacy single-principle response (still accepted for backward compatibility):
{ "kind": "principle", "name": "...", "dslSource": "...", "smtTemplate": "...",
  "teachingExample": {...} }
This is wrapped into a length-1 principles array with bugClassId derived from
\`bugClassHint\` or the principle name. Prefer the "principles" array form.

DSL syntax (exact format, no variations):
  predicate predicateName($arg: node) {
    match $inner: node where capabilityName.columnName == "value"
  }

  principle PrincipleName {
    match $var: node where capabilityName.columnName == "value"
    require no $guard: predicateName($var)
      where RELATION($guard.capabilityName.columnName, $var.capabilityName.columnName)
    report violation {
      at $var
      captures { captureName: $var }
      message "human readable message"
    }
  }

Notes:
- Each match clause: $varName: node where cap.col == "value"
- Multiple match clauses: only the first line uses "match", subsequent lines start with $varName directly
- require clause: require no $guard: predicateName($arg)
- where clause on require: uses a BUILT-IN RELATION from section B above, not a capability name
- report block: "report violation {" or "report warning {" or "report info {"
- captures block is REQUIRED and must have at least one entry
- message is a quoted string

FINAL CHECK before producing output: review the CAPABILITIES list in section A above.
If you reference a name in a require no predicate position, that name MUST be declared
as a predicate name(...) { ... } in the same principle file.
NEVER reference a capability name (section A) as a predicate name.

Rules:
- DSL must be valid (parseable) and use ONLY registered capabilities/relations above.
- Do NOT output anything outside the JSON object.`;
}

function buildAdversarialPrompt(
  principleDescription: string,
  dslSource: string,
): string {
  return `You are a security-minded adversary reviewing a static-analysis principle.

Principle description: ${principleDescription}
DSL source:
\`\`\`
${dslSource}
\`\`\`

Generate TypeScript fixture files to stress-test this principle.
Produce exactly 3 false-positive fixtures (benign code that should NOT match) and
3 false-negative fixtures (buggy code that MUST match).

Respond with ONLY a JSON object (no markdown fences):
{
  "false_positives": [
    { "source": "// TypeScript code here\\nfunction ok() { return 1; }" },
    ...
  ],
  "false_negatives": [
    { "source": "// TypeScript code with the bug\\nfunction bad() { return x / 0; }" },
    ...
  ]
}

Rules:
- Each source must be valid TypeScript (may have simple type errors but must parse).
- false_positives: code that a careless rule would flag but SHOULD NOT be flagged.
- false_negatives: code that DOES contain the bug pattern the principle targets.
- Do NOT output anything outside the JSON object.`;
}

// ---------------------------------------------------------------------------
// Adversarial validation runner — Oracle #6
// ---------------------------------------------------------------------------

/** Migrate a fresh DB. Reuses the same migrations path as other tests. */
function openFreshDb(dbPath: string): Db {
  const db = openDb(dbPath);
  try {
    migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  } catch {
    // migrations may already be applied
  }
  return db;
}

/** Validate adversarial fixtures from a parsed LLM JSON object. Returns null on shape failure. */
function validateAdversarialFixtures(rawParsed: unknown): {
  falsePositives: { source: string }[];
  falseNegatives: { source: string }[];
} | null {
  if (typeof rawParsed !== "object" || rawParsed === null) return null;
  const parsed = rawParsed as {
    false_positives?: { source: string }[];
    false_negatives?: { source: string }[];
  };
  const fps = parsed.false_positives;
  const fns = parsed.false_negatives;
  if (!Array.isArray(fps) || !Array.isArray(fns)) return null;
  return { falsePositives: fps, falseNegatives: fns };
}

/**
 * Oracle #6: adversarial validation.
 * Generates fixtures via a DIFFERENT model tier, builds SAST for each,
 * and checks that the principle:
 *   - Does NOT match false-positive fixtures (benign code)
 *   - DOES match false-negative fixtures (buggy code)
 *
 * Pass threshold: at least passThreshold of each set behaves correctly.
 * Default: 2/3.
 *
 * For substrate-extension principles, the capability table is empty unless
 * the extractor runs. Provide `preRunExtractor` to populate the capability
 * table after buildSASTForFile and before DSL evaluation. The callback
 * receives the fixture Db and source file path.
 */
export async function runAdversarialValidation(
  dslSource: string,
  principleDescription: string,
  llm: LLMProvider,
  db: Db,
  opts: {
    proposerModel?: "haiku" | "sonnet" | "opus";
    /** Minimum fraction of each direction that must pass. Default: 0.667 (2/3). */
    passThreshold?: number;
    /**
     * Optional extractor callback for substrate-extension principles.
     * Called after buildSASTForFile on the fixture DB to populate the
     * new capability table before DSL evaluation.
     * Receives (fixtureDb, srcPath) — same DB that was passed to buildSASTForFile.
     */
    preRunExtractor?: (fixtureDb: Db, srcPath: string) => void;
  } = {},
): Promise<{
  passed: boolean;
  evidence: string;
  validatorModel: string;
}> {
  const proposerModel = opts.proposerModel ?? "sonnet";
  // Use a different model tier for adversarial validation.
  const validatorModel: "haiku" | "sonnet" =
    proposerModel === "haiku" ? "sonnet" : "haiku";
  const passThreshold = opts.passThreshold ?? 0.667;

  let parsedRaw: unknown;
  try {
    parsedRaw = await requestStructuredJson<unknown>({
      prompt: buildAdversarialPrompt(principleDescription, dslSource),
      llm,
      stage: "C6-adversarial",
      model: validatorModel,
    });
  } catch (err) {
    // JSON parse failures + agent-write failures both surface here. Use
    // "malformed" so the evidence string is stable across both paths and
    // matches existing test expectations.
    return {
      passed: false,
      evidence: `adversarial LLM response was malformed: ${err instanceof Error ? err.message : String(err)}`,
      validatorModel,
    };
  }

  const parsed = validateAdversarialFixtures(parsedRaw);
  if (!parsed) {
    return {
      passed: false,
      evidence: `adversarial LLM response was malformed`,
      validatorModel,
    };
  }

  const { falsePositives, falseNegatives } = parsed;
  if (falsePositives.length === 0 || falseNegatives.length === 0) {
    return {
      passed: false,
      evidence: `adversarial LLM returned empty fixture sets`,
      validatorModel,
    };
  }

  // Parse + compile the principle once (not inside the fixture loop).
  let queryFns: ReturnType<typeof compileProgram>;
  try {
    const program = parseDSL(dslSource);
    queryFns = compileProgram(program.nodes);
  } catch (err) {
    return {
      passed: false,
      evidence: `DSL compile error during adversarial: ${err instanceof Error ? err.message : String(err)}`,
      validatorModel,
    };
  }

  if (queryFns.size === 0) {
    return {
      passed: false,
      evidence: `DSL compiled to 0 principles`,
      validatorModel,
    };
  }

  // Run a fixture: build SAST, run all principles, return total match count.
  const runFixture = (source: string): number => {
    const tmpDir = mkdtempSync(join(tmpdir(), "provekit-adversarial-"));
    try {
      // Init a bare git repo (builder requires git-tracked files).
      const GIT_ID = ["-c", "user.name=test", "-c", "user.email=test@test"];
      execFileSync("git", [...GIT_ID, "init", tmpDir], { stdio: "pipe" });
      execFileSync("git", [...GIT_ID, "-C", tmpDir, "config", "commit.gpgsign", "false"], { stdio: "pipe" });

      const srcPath = join(tmpDir, "fixture.ts");
      writeFileSync(srcPath, source, "utf-8");
      execFileSync("git", [...GIT_ID, "-C", tmpDir, "add", "fixture.ts"], { stdio: "pipe" });
      execFileSync("git", [...GIT_ID, "-C", tmpDir, "commit", "-m", "fixture"], { stdio: "pipe" });

      const dbPath = join(tmpDir, "sast.db");
      const fixtureDb = openFreshDb(dbPath);
      buildSASTForFile(fixtureDb, srcPath);

      // For substrate-extension principles: run the extractor to populate the
      // capability table before evaluating the DSL.
      if (opts.preRunExtractor) {
        try {
          opts.preRunExtractor(fixtureDb, srcPath);
        } catch {
          // Extractor errors don't fail the fixture — the DSL will just return 0 matches.
        }
      }

      let totalMatches = 0;
      for (const [, queryFn] of queryFns) {
        try {
          const rows = queryFn(fixtureDb);
          totalMatches += rows.length;
        } catch {
          // compile error at query time — count as 0 matches
        }
      }
      return totalMatches;
    } catch {
      return -1; // error running fixture
    } finally {
      try { rmSync(tmpDir, { recursive: true, force: true }); } catch { /* ignore */ }
    }
  };

  // Check false-positives: each must match 0 times.
  let fpPassed = 0;
  for (const fp of falsePositives) {
    const count = runFixture(fp.source);
    if (count === 0) fpPassed++;
  }

  // Check false-negatives: each must match at least once.
  let fnPassed = 0;
  for (const fn of falseNegatives) {
    const count = runFixture(fn.source);
    if (count > 0) fnPassed++;
  }

  const fpRate = fpPassed / falsePositives.length;
  const fnRate = fnPassed / falseNegatives.length;

  const passed = fpRate >= passThreshold && fnRate >= passThreshold;
  const evidence =
    `false-positive pass: ${fpPassed}/${falsePositives.length} (${(fpRate * 100).toFixed(0)}%); ` +
    `false-negative pass: ${fnPassed}/${falseNegatives.length} (${(fnRate * 100).toFixed(0)}%)`;

  void db; // db passed for potential future latentSiteMatches lookups

  return { passed, evidence, validatorModel };
}

// ---------------------------------------------------------------------------
// latentSiteMatches helper
// ---------------------------------------------------------------------------

/**
 * Run the compiled principle against the live SAST DB.
 * Returns top-N node locations (file + line) without writing to principle_matches.
 */
function findLatentSiteMatches(
  dslSource: string,
  db: Db,
  maxResults = 20,
): { nodeId: string; file: string; line: number }[] {
  try {
    const program = parseDSL(dslSource);
    const queryFns = compileProgram(program.nodes);
    const results: { nodeId: string; file: string; line: number }[] = [];
    for (const [, queryFn] of queryFns) {
      const rows = queryFn(db);
      for (const row of rows) {
        if (results.length >= maxResults) break;
        // Look up file path and line for this node.
        const nodeRow = db
          .select({ sourceLine: nodesTable.sourceLine, fileId: nodesTable.fileId })
          .from(nodesTable)
          .where(eq(nodesTable.id, row.atNodeId))
          .get();
        const fileRow = nodeRow
          ? db
              .select({ path: filesTable.path })
              .from(filesTable)
              .where(eq(filesTable.id, nodeRow.fileId))
              .get()
          : null;
        results.push({
          nodeId: row.atNodeId,
          file: fileRow?.path ?? "",
          line: nodeRow?.sourceLine ?? 0,
        });
      }
    }
    return results;
  } catch {
    return [];
  }
}

// ---------------------------------------------------------------------------
// LLM response parser for principle proposal
// ---------------------------------------------------------------------------

/** A single shape (one entry of the multi-shape `principles` array). */
interface PrincipleShape {
  name: string;
  dslSource: string;
  smtTemplate: string;
  teachingExample: { domain: string; explanation: string; smt2: string };
}

type PrincipleProposalResponse =
  | {
      kind: "principles";
      bugClassId: string;
      principles: PrincipleShape[];
    }
  | { kind: "needs_capability"; missing_predicate: string }
  | { kind: "non_codifiable"; reason?: string };

/**
 * Validate one shape entry. Returns null on shape failure.
 */
function validateShape(raw: unknown): PrincipleShape | null {
  if (typeof raw !== "object" || raw === null) return null;
  const s = raw as Record<string, unknown>;
  const name = s["name"];
  const dslSource = s["dslSource"];
  const smtTemplate = s["smtTemplate"];
  const teachingExample = s["teachingExample"] as
    | { domain: string; explanation: string; smt2: string }
    | undefined;
  if (
    typeof name !== "string" ||
    typeof dslSource !== "string" ||
    typeof smtTemplate !== "string" ||
    !teachingExample ||
    typeof teachingExample.domain !== "string"
  ) {
    return null;
  }
  return { name, dslSource, smtTemplate, teachingExample };
}

/**
 * Slugify a principle name to a stable bug-class-id when the LLM didn't supply one.
 * Lower-cases, replaces non-alphanumerics with hyphens, collapses + trims.
 */
function slugifyBugClassId(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80) || "unknown-bug-class";
}

function validatePrincipleProposal(rawParsed: unknown, bugClassHint?: string): PrincipleProposalResponse | null {
  try {
    if (typeof rawParsed !== "object" || rawParsed === null) return null;
    const p = rawParsed as Record<string, unknown>;
    const kind = p["kind"];

    // Multi-shape response (preferred form per pitch-leak 3 layer 1).
    if (kind === "principles") {
      const arr = p["principles"];
      if (!Array.isArray(arr) || arr.length === 0) return null;
      const shapes: PrincipleShape[] = [];
      for (const entry of arr) {
        const shape = validateShape(entry);
        if (shape !== null) shapes.push(shape);
      }
      if (shapes.length === 0) return null;
      const rawId = p["bugClassId"];
      const bugClassId =
        typeof rawId === "string" && rawId.length > 0
          ? slugifyBugClassId(rawId)
          : slugifyBugClassId(bugClassHint ?? shapes[0].name);
      return { kind: "principles", bugClassId, principles: shapes };
    }

    // Legacy single-principle response — wrap into length-1 array.
    if (kind === "principle") {
      const shape = validateShape(p);
      if (shape === null) return null;
      const bugClassId = slugifyBugClassId(bugClassHint ?? shape.name);
      return { kind: "principles", bugClassId, principles: [shape] };
    }

    if (kind === "needs_capability") {
      const missing_predicate = p["missing_predicate"];
      if (typeof missing_predicate !== "string") return null;
      return { kind: "needs_capability", missing_predicate };
    }
    if (kind === "non_codifiable") {
      return { kind: "non_codifiable" };
    }
    return null;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Main export: tryExistingCapabilities
// ---------------------------------------------------------------------------

/**
 * Ask the LLM to express the invariant using existing capabilities.
 *
 * Pitch-leak 3 layer 1: the LLM may return 1-3 alternative syntactic shapes
 * for the same bug class. Each shape is compiled + adversarially validated
 * INDEPENDENTLY; valid shapes are kept, failing shapes are dropped (with a
 * warning), and the canonical shape is preserved at index 0. The whole call
 * is treated as non_codifiable only if NO shapes survive validation; if even
 * one survives we return the surviving set.
 *
 * Capability-gap routing: if the FIRST (canonical) shape compile fails with
 * unknown-capability, we route to the substrate path. We don't probe alts
 * for capability gaps — alts assume the canonical's capability set.
 *
 * Returns:
 *   { kind: "ok", principles } — at least one principle compiled + adversarial passed
 *   { kind: "capability_gap", gap } — needs new capability
 *   { kind: "non_codifiable" } — cannot be expressed as a static rule
 */
export async function tryExistingCapabilities(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  db: Db;
  llm: LLMProvider;
}): Promise<ExistingCapAttempt> {
  const { signal, invariant, fixCandidate, db, llm } = args;

  let parsedRaw: unknown;
  try {
    parsedRaw = await requestStructuredJson<unknown>({
      prompt: buildPrinciplePrompt(signal, invariant, fixCandidate),
      llm,
      stage: "C6-principleProposal",
      model: getModelTier("C6-principleProposal"),
    });
  } catch (err) {
    console.warn(`[C6] LLM call/parse failed: ${err instanceof Error ? err.message : String(err)}`);
    return { kind: "non_codifiable" };
  }

  const proposal = validatePrincipleProposal(parsedRaw, signal.bugClassHint);
  if (!proposal) {
    console.warn(`[C6] LLM response malformed or could not be parsed`);
    return { kind: "non_codifiable" };
  }

  if (proposal.kind === "non_codifiable") {
    return { kind: "non_codifiable" };
  }

  if (proposal.kind === "needs_capability") {
    return { kind: "capability_gap", gap: proposal.missing_predicate };
  }

  // proposal.kind === "principles". Validate each shape independently.
  const accepted: PrincipleCandidate[] = [];
  for (let i = 0; i < proposal.principles.length; i++) {
    const shape = proposal.principles[i];
    const isCanonical = i === 0;

    // Compile.
    try {
      const program = parseDSL(shape.dslSource);
      compileProgram(program.nodes);
    } catch (err) {
      if (err instanceof CompileError) {
        const msg = err.message.toLowerCase();
        if (msg.includes("unknown capability") || msg.includes("unknown column")) {
          // Capability gap on the canonical shape → route to substrate path.
          if (isCanonical) {
            return { kind: "capability_gap", gap: err.message };
          }
          // Alt shapes are ignored if they reference a missing capability.
          console.warn(`[C6] alt shape "${shape.name}" references unknown capability; dropping`);
          continue;
        }
      }
      console.warn(
        `[C6] DSL compile error for shape "${shape.name}": ${err instanceof Error ? err.message : String(err)}`,
      );
      // Canonical compile failure (non-capability-gap) → non-codifiable for the bundle.
      if (isCanonical) {
        return { kind: "non_codifiable" };
      }
      continue;
    }

    // Oracle #6: adversarial validation, per shape.
    const adversarial = await runAdversarialValidation(
      shape.dslSource,
      invariant.description,
      llm,
      db,
      { proposerModel: "sonnet" },
    );

    if (!adversarial.passed) {
      console.warn(
        `[C6] adversarial validation failed for shape "${shape.name}": ${adversarial.evidence}`,
      );
      // For the canonical shape we still allow alts to potentially survive,
      // but if NO shape survives we'll return non_codifiable below.
      continue;
    }

    const latentSiteMatches = findLatentSiteMatches(shape.dslSource, db);

    accepted.push({
      kind: "principle",
      name: shape.name,
      bugClassId: proposal.bugClassId,
      dslSource: shape.dslSource,
      smtTemplate: shape.smtTemplate,
      teachingExample: shape.teachingExample,
      adversarialValidation: [
        {
          validatorModel: adversarial.validatorModel,
          result: "pass",
          evidence: adversarial.evidence,
        },
      ],
      latentSiteMatches,
    });
  }

  if (accepted.length === 0) {
    return { kind: "non_codifiable" };
  }

  return { kind: "ok", principles: accepted };
}

// ---------------------------------------------------------------------------
// Substrate extractor builder for adversarial validation
// ---------------------------------------------------------------------------

/**
 * Build a `preRunExtractor` callback from a CapabilitySpec.extractorTs.
 * The callback is used by runAdversarialValidation to populate the capability
 * table after building SAST for each fixture.
 *
 * The extractor is transpiled to CJS (same as capabilityExecutor.ts), written
 * to node_modules/.cache/, and async-imported. The result is a synchronous
 * callback that calls the pre-loaded extractor function.
 *
 * Returns null if transpile or import fails.
 */
async function buildSubstrateAdversarialExtractor(
  spec: CapabilitySpec,
): Promise<((fixtureDb: Db, srcPath: string) => void) | null> {
  // Transpile extractorTs to CJS
  let transpiled: string;
  try {
    const result = ts.transpileModule(spec.extractorTs, {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
        esModuleInterop: true,
        skipLibCheck: true,
      },
    });
    transpiled = result.outputText;
  } catch {
    return null;
  }

  // Determine cache dir. Use process.cwd() which is the project root when
  // running under vitest (same assumption as capabilityExecutor.ts resolveProjectRoot).
  const cacheDir = join(process.cwd(), "node_modules", ".cache");

  // Write transpiled JS to a stable path (not tmpdir — adversarial runs many fixtures)
  const jsPath = join(cacheDir, `provekit-adversarial-extractor-${spec.capabilityName}.cjs`);
  try {
    mkdirSync(cacheDir, { recursive: true });
    writeFileSync(jsPath, transpiled);
  } catch {
    return null;
  }

  // Detect extractor function name
  const fnMatch = /export\s+function\s+(\w+)\s*\(/.exec(spec.extractorTs);
  const fnName = fnMatch?.[1] ?? ("extract" + spec.capabilityName.charAt(0).toUpperCase() + spec.capabilityName.slice(1));

  // Async import (dynamic import works in both CJS and ESM contexts)
  let extractorFn: ((...args: unknown[]) => void) | undefined;
  try {
    const { pathToFileURL } = await import("url");
    const mod = await import(pathToFileURL(jsPath).href) as Record<string, unknown>;
    const candidate = mod[fnName] ?? mod["default"];
    if (typeof candidate !== "function") return null;
    extractorFn = candidate as (...args: unknown[]) => void;
  } catch {
    return null;
  }

  const capturedExtractorFn = extractorFn;

  return (fixtureDb: Db, srcPath: string): void => {
    // Apply the migration (create capability table in fixtureDb) if needed
    try {
      const stmts = spec.migrationSql
        .split(";")
        .map((s) => s.trim())
        .filter((s) => s.length > 0);
      for (const stmt of stmts) {
        try { fixtureDb.$client.exec(stmt); } catch { /* table may already exist */ }
      }
    } catch { /* ignore */ }

    // Build SAST handles for the extractor
    try {
      const { sourceFile, nodeIdByNode } = buildSASTAndReturnHandles(fixtureDb, srcPath);
      fixtureDb.transaction((tx) => {
        capturedExtractorFn(tx, sourceFile, nodeIdByNode);
      });
    } catch { /* extractor errors don't fail adversarial — DSL will return 0 matches */ }
  };
}

// ---------------------------------------------------------------------------
// Substrate-extension path: proposeWithCapability
// ---------------------------------------------------------------------------

/**
 * Propose a new capability + principle when existing capabilities are insufficient.
 * Runs oracles #14/#16/#17/#18 before adversarial validation.
 * Returns null if any oracle fails or adversarial validation fails.
 */
export async function proposeWithCapability(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  db: Db;
  llm: LLMProvider;
  gap: string;
  overlay?: OverlayHandle;
}): Promise<PrincipleCandidate | null> {
  const { signal, invariant, fixCandidate, db, llm, gap } = args;

  // 1. Ask LLM to propose a CapabilitySpec + principle.
  const proposal = await proposeCapabilitySpec({
    signal,
    invariant,
    fixCandidate,
    gap,
    llm,
    overlay: args.overlay,
  });

  if (!proposal) {
    console.warn(`[C6] Capability spec proposal returned null`);
    return null;
  }

  const { capabilitySpec, dslSource, name, smtTemplate, teachingExample } = proposal;

  // 2. Run substrate oracles #14/#16/#17 (before touching the registry).
  const substrateResult = await runSubstrateOracles(capabilitySpec);
  if (!substrateResult.passed) {
    console.warn(`[C6] Substrate oracle failed: ${substrateResult.reason}`);
    return null;
  }

  // 3. Oracle #18: register capability temporarily, verify compile behavior.
  // The principle MUST fail to compile WITHOUT the new capability.
  try {
    const program = parseDSL(dslSource);
    compileProgram(program.nodes);
    // If compile succeeded WITHOUT the new capability, the principle doesn't need it.
    console.warn(`[C6] Oracle #18 failed: principle compiles without the proposed capability (gratuitous)`);
    return null;
  } catch (err) {
    if (!(err instanceof CompileError)) {
      // Unexpected error (parse error etc.) — treat as non-codifiable.
      console.warn(`[C6] Oracle #18: unexpected error before registration: ${err instanceof Error ? err.message : String(err)}`);
      return null;
    }
    const msg = err.message.toLowerCase();
    if (!msg.includes("unknown capability") && !msg.includes("unknown column")) {
      // Fails for a different reason — not a capability gap.
      console.warn(`[C6] Oracle #18: compile failed for unexpected reason: ${err.message}`);
      return null;
    }
    // Expected: compile fails with unknown capability. Proceed.
  }

  // Register the capability temporarily (use try/finally for cleanup).
  // Build a minimal CapabilityDescriptor from the spec for compile-checking.
  // NOTE: We build an in-memory descriptor with no actual Drizzle table — just enough for compilation.
  const tempDescriptor = buildTempDescriptor(capabilitySpec);
  registerCapability(tempDescriptor);

  try {
    // Verify compile succeeds WITH the new capability.
    try {
      const program = parseDSL(dslSource);
      compileProgram(program.nodes);
    } catch (err) {
      console.warn(`[C6] Oracle #18: compile still failed after registering capability: ${err instanceof Error ? err.message : String(err)}`);
      return null;
    }

    // Oracle #6: adversarial validation WITH the capability temporarily registered.
    // For substrate-extension principles: also run the extractor against each fixture
    // so the capability table is populated before DSL evaluation.
    const substrateExtractor = await buildSubstrateAdversarialExtractor(capabilitySpec);
    const adversarial = await runAdversarialValidation(
      dslSource,
      invariant.description,
      llm,
      db,
      {
        proposerModel: "sonnet",
        preRunExtractor: substrateExtractor ?? undefined,
      },
    );

    if (!adversarial.passed) {
      console.warn(`[C6] Adversarial validation failed for substrate path: ${adversarial.evidence}`);
      return null;
    }

    const latentSiteMatches = findLatentSiteMatches(dslSource, db);

    const principle: PrincipleCandidate = {
      kind: "principle_with_capability",
      name,
      // Substrate-extension path: single canonical shape. bugClassId derives
      // from bugClassHint when available, else from the principle name.
      bugClassId: slugifyBugClassId(signal.bugClassHint ?? name),
      dslSource,
      smtTemplate,
      teachingExample,
      adversarialValidation: [
        {
          validatorModel: adversarial.validatorModel,
          result: "pass",
          evidence: adversarial.evidence,
        },
      ],
      latentSiteMatches,
      capabilitySpec,
    };

    return principle;
  } finally {
    unregisterCapability(capabilitySpec.capabilityName);
  }
}

// ---------------------------------------------------------------------------
// Temp capability descriptor builder (for Oracle #18 compile check only)
// ---------------------------------------------------------------------------

/**
 * Build a minimal CapabilityDescriptor from a CapabilitySpec for compile-time validation.
 * The table and column drizzle references are stub objects — sufficient for the compiler
 * to resolve capability/column names but NOT for actual SQL execution.
 */
function buildTempDescriptor(
  spec: CapabilitySpec,
): import("../sast/capabilityRegistry.js").CapabilityDescriptor {
  // Parse column names from schemaTs by looking for column definitions.
  // This is a structural heuristic: look for .column() calls or simple field names.
  // We always include "node_id" as the mandatory FK column.
  const columnNames = extractColumnNamesFromSchemaTs(spec.schemaTs);
  if (!columnNames.includes("node_id")) {
    columnNames.unshift("node_id");
  }

  // Build a stub Drizzle table-like object.
  const stubTableName = `node_${spec.capabilityName}`;
  const stubTable = buildStubDrizzleTable(stubTableName);

  // Build column descriptors.
  const columns: Record<string, import("../sast/capabilityRegistry.js").CapabilityColumn> = {};
  for (const colName of columnNames) {
    columns[colName] = {
      dslName: colName,
      drizzleColumn: buildStubDrizzleColumn(colName),
      isNodeRef: colName === "node_id",
      nullable: colName !== "node_id",
    };
  }

  return {
    dslName: spec.capabilityName,
    table: stubTable as any,
    columns,
  };
}

/**
 * Extract column names from schemaTs source by pattern-matching common patterns.
 * Handles: `colName: text("col_name")`, `colName: integer("col_name")`, etc.
 */
function extractColumnNamesFromSchemaTs(schemaTs: string): string[] {
  const names: string[] = [];
  // Match TypeScript object keys that look like column definitions.
  const fieldPattern = /^\s+(\w+)\s*:/gm;
  let m: RegExpExecArray | null;
  while ((m = fieldPattern.exec(schemaTs)) !== null) {
    const name = m[1];
    if (name && !["default", "references", "primaryKey", "notNull", "unique"].includes(name)) {
      names.push(name);
    }
  }
  // Deduplicate.
  return [...new Set(names)];
}

/** Build a minimal stub object that satisfies sqlTableName() in compiler.ts. */
function buildStubDrizzleTable(tableName: string): object {
  return {
    _: { name: tableName },
  };
}

/** Build a minimal stub object that satisfies sqlColName() in compiler.ts. */
function buildStubDrizzleColumn(colName: string): object {
  // The compiler uses column.name (the SQL name). We use snake_case of dslName.
  const sqlName = colName.replace(/([A-Z])/g, "_$1").toLowerCase().replace(/^_/, "");
  return { name: sqlName };
}
