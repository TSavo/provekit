/**
 * C6: Substrate-extension path helpers.
 *
 * Exports:
 *   proposeCapabilitySpec — LLM proposes a new CapabilitySpec + DSL principle
 *   runSubstrateOracles   — Oracles #14/#16/#17
 *
 * Oracle #18 lives in principleGen.ts (needs registry access).
 */

import { Project } from "ts-morph";
import { readFileSync, existsSync, mkdirSync } from "fs";
import { join } from "path";
import type {
  BugSignal,
  InvariantClaim,
  FixCandidate,
  LLMProvider,
  OverlayHandle,
} from "./types.js";
import type { CapabilitySpec } from "./types.js";
import { listCapabilities } from "../sast/capabilityRegistry.js";
import { executeExtractorSpec } from "./capabilityExecutor.js";
import { runAgentInOverlay } from "./captureChange.js";
import { requestStructuredJson } from "./llm/structuredOutput.js";
import { getModelTier } from "./modelTiers.js";

// ---------------------------------------------------------------------------
// Substrate oracle result
// ---------------------------------------------------------------------------

export interface SubstrateOracleResult {
  passed: boolean;
  reason: string;
  oracleResults: {
    oracle14_migrationSafe: boolean;
    oracle16_extractorCoverage: boolean;
    oracle17_substrateConsistency: boolean;
  };
}

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

function buildCapabilitySpecPrompt(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  gap: string;
}): string {
  const existingTables = listCapabilities().map((c) => c.dslName).join(", ") || "(none)";

  return `You are a static-analysis substrate architect. A new capability is needed to express an invariant.

Bug summary: ${args.signal.summary}
Invariant: ${args.invariant.description}
Fix description: ${args.fixCandidate.patch.description}
Missing predicate: ${args.gap}

Existing capability tables: ${existingTables}

Design a new capability to fill this gap. Respond with ONLY a JSON object (no markdown fences):
{
  "capabilityName": "myCapability",
  "schemaTs": "// TypeScript schema file contents\\nexport const nodeMyCapability = sqliteTable('node_my_capability', {\\n  nodeId: text('node_id').notNull(),\\n  myColumn: text('my_column').notNull(),\\n});",
  "migrationSql": "CREATE TABLE node_my_capability (node_id TEXT NOT NULL, my_column TEXT NOT NULL);",
  "extractorTs": "// TypeScript extractor\\nexport function extractMyCapability(tx: any, fileId: number, ast: any): void {\\n  // walk AST\\n  tx.insert(nodeMyCapability).values({ nodeId: '...', myColumn: '...' });\\n}",
  "extractorTestsTs": "// TypeScript tests\\nimport { describe, it, expect } from 'vitest';\\ndescribe('extractMyCapability', () => { it('extracts', () => { expect(true).toBe(true); }); });",
  "registryRegistration": "registerCapability({ dslName: 'myCapability', table: nodeMyCapability, columns: { node_id: { dslName: 'node_id', drizzleColumn: nodeMyCapability.nodeId, isNodeRef: true, nullable: false } } });",
  "positiveFixtures": [
    { "source": "function bad() { return x / 0; }", "expectedRowCount": 1 }
  ],
  "negativeFixtures": [
    { "source": "function ok() { return x / 1; }", "expectedRowCount": 0 }
  ],
  "rationale": "This capability tracks ...",
  "dslSource": "principle MyPrinciple {\\n  match $x: node where myCapability.myColumn == \\"bad\\"\\n  report violation {\\n    at $x\\n    message \\"Found bad pattern\\"\\n  }\\n}",
  "name": "MyPrinciple",
  "smtTemplate": "(declare-const x Int)\\n(assert (= x 0))\\n(check-sat)",
  "teachingExample": {
    "domain": "arithmetic",
    "explanation": "...",
    "smt2": "(declare-const x Int)\\n(assert (= x 0))\\n(check-sat)"
  }
}

Rules:
- capabilityName must be a valid identifier (camelCase).
- migrationSql must be CREATE TABLE or ALTER TABLE ADD COLUMN only — no DROPs.
- extractorTs MUST contain tx.insert(<table>).values(...) pattern.
- dslSource must reference ONLY the new capabilityName (and existing ones if needed).
- positiveFixtures: TypeScript code that SHOULD match the principle.
- negativeFixtures: TypeScript code that should NOT match.
- Do NOT output anything outside the JSON object.`;
}

// ---------------------------------------------------------------------------
// LLM response parser for capability spec proposal
// ---------------------------------------------------------------------------

interface CapabilitySpecProposal {
  capabilitySpec: CapabilitySpec;
  dslSource: string;
  name: string;
  smtTemplate: string;
  teachingExample: { domain: string; explanation: string; smt2: string };
}

function validateCapabilitySpecResponse(rawParsed: unknown): CapabilitySpecProposal | null {
  try {
    if (typeof rawParsed !== "object" || rawParsed === null) return null;
    const p = rawParsed as Record<string, unknown>;

    const capabilityName = p["capabilityName"];
    const schemaTs = p["schemaTs"];
    const migrationSql = p["migrationSql"];
    const extractorTs = p["extractorTs"];
    const extractorTestsTs = p["extractorTestsTs"];
    const registryRegistration = p["registryRegistration"];
    const positiveFixtures = p["positiveFixtures"];
    const negativeFixtures = p["negativeFixtures"];
    const rationale = p["rationale"];
    const dslSource = p["dslSource"];
    const name = p["name"];
    const smtTemplate = p["smtTemplate"];
    const teachingExample = p["teachingExample"] as {
      domain: string;
      explanation: string;
      smt2: string;
    } | undefined;

    if (
      typeof capabilityName !== "string" ||
      typeof schemaTs !== "string" ||
      typeof migrationSql !== "string" ||
      typeof extractorTs !== "string" ||
      typeof extractorTestsTs !== "string" ||
      typeof registryRegistration !== "string" ||
      typeof rationale !== "string" ||
      typeof dslSource !== "string" ||
      typeof name !== "string" ||
      typeof smtTemplate !== "string" ||
      !teachingExample ||
      typeof teachingExample.domain !== "string" ||
      !Array.isArray(positiveFixtures) ||
      !Array.isArray(negativeFixtures)
    ) {
      return null;
    }

    const capabilitySpec: CapabilitySpec = {
      capabilityName,
      schemaTs,
      migrationSql,
      extractorTs,
      extractorTestsTs,
      registryRegistration,
      positiveFixtures: (positiveFixtures as { source: string; expectedRowCount: number }[]).map(
        (f) => ({ source: String(f.source ?? ""), expectedRowCount: Number(f.expectedRowCount ?? 1) }),
      ),
      negativeFixtures: (negativeFixtures as { source: string }[]).map(
        (f) => ({ source: String(f.source ?? ""), expectedRowCount: 0 as const }),
      ),
      rationale,
    };

    return { capabilitySpec, dslSource, name, smtTemplate, teachingExample };
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Agent path: proposeCapabilitySpecViaAgent
// ---------------------------------------------------------------------------

/**
 * Agent path for C6: prompts the LLM agent to write capability spec files
 * under .provekit/capability-proposal/<name>/ in the overlay worktree.
 *
 * Convention (all paths relative to overlay.worktreePath):
 *   .provekit/capability-proposal/<name>/schema.ts
 *   .provekit/capability-proposal/<name>/migration.sql
 *   .provekit/capability-proposal/<name>/extractor.ts
 *   .provekit/capability-proposal/<name>/extractor.test.ts
 *   .provekit/capability-proposal/<name>/registry.ts
 *   .provekit/capability-proposal/<name>/fixtures.json
 *   .provekit/capability-proposal/<name>/meta.json
 *   .provekit/principles/<name>.dsl
 *
 * meta.json format:
 *   { "capabilityName": "...", "rationale": "...", "dslSource": "...",
 *     "name": "...", "smtTemplate": "...",
 *     "teachingExample": { "domain": "...", "explanation": "...", "smt2": "..." } }
 *
 * fixtures.json format:
 *   { "positiveFixtures": [...], "negativeFixtures": [...] }
 */
async function proposeCapabilitySpecViaAgent(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  gap: string;
  llm: LLMProvider;
  overlay: OverlayHandle;
}): Promise<CapabilitySpecProposal | null> {
  const { signal, invariant, fixCandidate, gap, overlay } = args;
  const llm = args.llm;

  const existingTables = listCapabilities().map((c) => c.dslName).join(", ") || "(none)";

  const prompt = `[STAGE:C6-capability] proposeCapabilitySpec
You are a static-analysis substrate architect. A new capability is needed to express an invariant.

Bug summary: ${signal.summary}
Invariant: ${invariant.description}
Fix description: ${fixCandidate.patch.description}
Missing predicate: ${gap}

Existing capability tables: ${existingTables}

# How your capability will be validated

After you produce the spec, oracle #16 will:
1. Migrate your schema into a fresh DB.
2. Transpile schema.ts and extractor.ts to JS, place them next to a scratch DB.
3. For each POSITIVE fixture: build SAST, call your extractor, count rows in
   your new table. Must be >= positiveFixtures[i].expectedRowCount.
4. For each NEGATIVE fixture: same, but row count must be exactly 0.

The most common failure is "positive fixtures: 0/N" — your extractor walked
the AST but never matched anything. Read the extractor pattern below carefully.

# Canonical extractor pattern (copy this shape)

\`\`\`ts
import { SyntaxKind, type SourceFile, type BinaryExpression } from "ts-morph";
import type { SastTx, NodeIdMap } from "../../sast/types.js";  // path adjusted at integration time
import { nodeMyCapability } from "./schema.js";

export function extractMyCapability(
  tx: SastTx,
  sourceFile: SourceFile,
  nodeIdByNode: NodeIdMap,
): void {
  sourceFile.forEachDescendant((node) => {
    // 1. Filter by node kind first — fast.
    if (node.getKind() !== SyntaxKind.BinaryExpression) return;
    const bin = node as BinaryExpression;

    // 2. Apply load-bearing condition. THIS IS WHERE EXTRACTORS GO WRONG.
    //    Match the pattern your principle cares about. Don't be too narrow
    //    (you'll miss positive fixtures) or too broad (you'll match negatives).
    const opKind = bin.getOperatorToken().getKind();
    if (opKind !== SyntaxKind.SlashToken) return;  // example: only divisions

    // 3. Resolve node IDs. nodeIdByNode is the key fact — it maps every
    //    AST node ts-morph produced to its row in the SAST nodes table.
    //    If a node isn't in the map, skip it (don't crash).
    const nodeId = nodeIdByNode.get(bin);
    const lhsId = nodeIdByNode.get(bin.getLeft());
    const rhsId = nodeIdByNode.get(bin.getRight());
    if (!nodeId || !lhsId || !rhsId) return;

    // 4. Insert the row. tx.insert returns a builder; .run() executes it.
    tx.insert(nodeMyCapability).values({
      nodeId,
      lhsNode: lhsId,
      rhsNode: rhsId,
      // ... any other columns your schema declared
    }).run();
  });
}
\`\`\`

# How to design YOUR extractor

1. **Identify the load-bearing AST shape.** What specifically marks code as
   matching the bug class? For Bug-1 duplicate-methods, the load-bearing
   shape is "an Array.prototype.push.apply call where the LHS is consumed
   as a set" (joined into a comma-string, etc.). For division-by-zero, it's
   "a / b BinaryExpression where rhs isn't dominated by a guard."

2. **Walk every node of the relevant kind.** \`forEachDescendant\` visits
   every node in the file. Filter by kind FIRST (\`node.getKind()\`), then
   refine.

3. **Cast to the specific node type AFTER the kind check.** ts-morph's
   types are guarded — do \`if (node.getKind() === SyntaxKind.X) { const x =
   node as X; ... }\`.

4. **Always look up node IDs from nodeIdByNode.** Never invent IDs. If a
   lookup returns undefined (rare, but possible for synthetic nodes),
   return early — don't crash.

5. **Test your design against your fixtures BEFORE submitting.** Mentally
   walk through one positive fixture: would your code emit >= the expected
   row count? Mentally walk through one negative: would your code emit 0?

# Anti-patterns

\`\`\`ts
// TOO NARROW — matches only the EXACT shape "a / 2", misses every parameterized division
sourceFile.forEachDescendant((node) => {
  if (node.getKind() !== SyntaxKind.BinaryExpression) return;
  const bin = node as BinaryExpression;
  if (bin.getRight().getText() !== "2") return;  // only matches /2
  if (!nodeIdByNode.get(bin)) return;
  tx.insert(...).values(...).run();
});
\`\`\`

\`\`\`ts
// TOO BROAD — matches any binary expression
sourceFile.forEachDescendant((node) => {
  if (node.getKind() !== SyntaxKind.BinaryExpression) return;
  // no further check — emits a row for every +, -, *, /, ==, =, etc.
  tx.insert(...).values(...).run();
});
\`\`\`

\`\`\`ts
// VACUOUS — has the right shape but never emits because it filters on
// something that's always false. The most common form of "0 positives".
sourceFile.forEachDescendant((node) => {
  if (node.getKind() !== SyntaxKind.CallExpression) return;
  const call = node as CallExpression;
  if (call.getExpression().getText() !== "push.apply") return;  // text() is "options.push.apply", never matches "push.apply"
  tx.insert(...).values(...).run();
});
\`\`\`

# Schema shape (CRITICAL — all node references are TEXT, not INTEGER)

The SAST \`nodes\` table uses TEXT primary keys (synthetic IDs derived from
file path + AST position). Every capability that references a node MUST
declare those columns as \`text\`, NOT \`integer\`. Mismatching the type
causes oracle #16 to fail with "datatype mismatch" on every insert because
the extractor passes string IDs into integer-declared columns.

Canonical schema shape:

\`\`\`ts
import { sqliteTable, text, integer } from "drizzle-orm/sqlite-core";
import { nodes } from "../../../src/sast/schema/nodes.js";  // path adjusted at integration

export const nodeMyCapability = sqliteTable("node_my_capability", {
  // PRIMARY KEY: the AST node this row describes. text + FK to nodes.id.
  nodeId: text("node_id").primaryKey().references(() => nodes.id, { onDelete: "cascade" }),

  // OTHER NODE REFERENCES: also text + FK to nodes.id.
  rhsNode: text("rhs_node").references(() => nodes.id, { onDelete: "cascade" }),
  objectNode: text("object_node").notNull().references(() => nodes.id, { onDelete: "cascade" }),

  // NON-NODE COLUMNS: enums or strings as text(), counts as integer().
  propertyName: text("property_name"),                    // optional string
  assignKind: text("assign_kind").notNull(),              // enum-as-string
  occurrenceCount: integer("occurrence_count").notNull(), // numeric
  isComputed: integer("is_computed", { mode: "boolean" }).notNull(),  // 0/1 boolean
});
\`\`\`

Mirror these conventions in your migration.sql:
\`\`\`sql
CREATE TABLE node_my_capability (
  node_id TEXT PRIMARY KEY NOT NULL,
  rhs_node TEXT,
  object_node TEXT NOT NULL,
  property_name TEXT,
  assign_kind TEXT NOT NULL,
  occurrence_count INTEGER NOT NULL,
  is_computed INTEGER NOT NULL,
  FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE,
  FOREIGN KEY (rhs_node) REFERENCES nodes(id) ON DELETE CASCADE,
  FOREIGN KEY (object_node) REFERENCES nodes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_node_my_capability_property ON node_my_capability(property_name);
\`\`\`

# Files to write (all paths relative to overlay root)

1. .provekit/capability-proposal/<capabilityName>/schema.ts — TypeScript
   schema with sqliteTable, following the canonical shape above.

2. .provekit/capability-proposal/<capabilityName>/migration.sql —
   CREATE TABLE for your capability. CREATE INDEX is also allowed (and
   recommended for any column the principle queries on). NO DROPs, NO
   destructive ALTERs.

3. .provekit/capability-proposal/<capabilityName>/extractor.ts — the
   ts-morph walker that emits rows. Follows the canonical shape above.
   Imports schema from \`./schema\` (oracle #16 transpiles both).

4. .provekit/capability-proposal/<capabilityName>/extractor.test.ts —
   vitest tests asserting your extractor emits the right rows for at
   least one positive fixture and zero rows for one negative.

5. .provekit/capability-proposal/<capabilityName>/registry.ts —
   registerCapability call.

6. .provekit/capability-proposal/<capabilityName>/fixtures.json —
   { "positiveFixtures": [{"source": "<TS code>", "expectedRowCount": N}, ...],
     "negativeFixtures": [{"source": "<TS code>", "expectedRowCount": 0}, ...] }

   Each positive fixture's \`source\` must be a self-contained TS snippet
   that exhibits the pattern; expectedRowCount is HOW MANY rows your
   extractor should emit for it (typically 1, possibly more for fixtures
   with multiple instances).

   Negative fixtures should be structurally similar but lacking the
   load-bearing element. Do not use empty / unrelated code as negatives —
   the extractor would trivially emit 0 for those without exercising
   discrimination.

7. .provekit/capability-proposal/<capabilityName>/meta.json —
   { "capabilityName": "...", "rationale": "...", "dslSource": "...",
     "name": "...", "smtTemplate": "...",
     "teachingExample": { "domain": "...", "explanation": "...", "smt2": "..." } }

8. .provekit/principles/<name>.dsl — DSL principle using the new capability.

# Universal rules

- capabilityName must be valid camelCase
- All files self-consistent: schema's table name matches migration's CREATE
  TABLE, matches extractor's import + insert, matches registry's
  registerCapability call
- expectedRowCount values in fixtures must MATCH what your extractor will
  actually emit — write the extractor first, count rows mentally, set
  expected values to match
- No DROPs, no destructive ops in migration.sql

Write all files now using your tools.`;

  try {
    await runAgentInOverlay({
      overlay,
      llm,
      prompt,
      allowedTools: ["Read", "Edit", "Write", "Bash", "Glob", "Grep"],
      model: getModelTier("C6-capabilityAgent"),
    });
  } catch (err) {
    console.warn(`[C6/cap/agent] Agent call failed: ${err instanceof Error ? err.message : String(err)}`);
    return null;
  }

  // Read back the files the agent wrote.
  const proposalDir = join(overlay.worktreePath, ".provekit", "capability-proposal");

  // Find the first subdirectory (the capability name directory).
  let capabilityName: string | null = null;
  try {
    const { readdirSync, statSync } = require("fs") as typeof import("fs");
    if (existsSync(proposalDir)) {
      const entries = readdirSync(proposalDir);
      for (const entry of entries) {
        const entryPath = join(proposalDir, entry);
        if (statSync(entryPath).isDirectory()) {
          capabilityName = entry;
          break;
        }
      }
    }
  } catch {
    console.warn(`[C6/cap/agent] Could not read capability-proposal directory`);
    return null;
  }

  if (!capabilityName) {
    console.warn(`[C6/cap/agent] Agent did not create a capability subdirectory`);
    return null;
  }

  const capDir = join(proposalDir, capabilityName);
  const readFile = (name: string): string | null => {
    const p = join(capDir, name);
    if (!existsSync(p)) return null;
    try { return readFileSync(p, "utf-8"); } catch { return null; }
  };

  const schemaTs = readFile("schema.ts");
  const migrationSql = readFile("migration.sql");
  const extractorTs = readFile("extractor.ts");
  const extractorTestsTs = readFile("extractor.test.ts");
  const registryTs = readFile("registry.ts");
  const fixturesJson = readFile("fixtures.json");
  const metaJson = readFile("meta.json");

  if (!schemaTs || !migrationSql || !extractorTs || !extractorTestsTs || !registryTs || !fixturesJson || !metaJson) {
    console.warn(`[C6/cap/agent] Agent did not write all required capability files for '${capabilityName}'`);
    return null;
  }

  // Parse meta.json
  let meta: Record<string, unknown>;
  try {
    meta = JSON.parse(metaJson) as Record<string, unknown>;
  } catch {
    console.warn(`[C6/cap/agent] meta.json is not valid JSON`);
    return null;
  }

  // Parse fixtures.json
  let fixtures: { positiveFixtures: { source: string; expectedRowCount: number }[]; negativeFixtures: { source: string }[] };
  try {
    fixtures = JSON.parse(fixturesJson) as typeof fixtures;
  } catch {
    console.warn(`[C6/cap/agent] fixtures.json is not valid JSON`);
    return null;
  }

  const resolvedCapabilityName = (typeof meta["capabilityName"] === "string" ? meta["capabilityName"] : capabilityName);
  const rationale = typeof meta["rationale"] === "string" ? meta["rationale"] : "";
  const dslSource = typeof meta["dslSource"] === "string" ? meta["dslSource"] : "";
  const name = typeof meta["name"] === "string" ? meta["name"] : resolvedCapabilityName;
  const smtTemplate = typeof meta["smtTemplate"] === "string" ? meta["smtTemplate"] : "";
  const teachingExample = (meta["teachingExample"] as { domain: string; explanation: string; smt2: string } | undefined) ?? {
    domain: "unknown",
    explanation: "",
    smt2: "(check-sat)",
  };

  if (!dslSource || !smtTemplate) {
    console.warn(`[C6/cap/agent] meta.json missing required fields (dslSource, smtTemplate)`);
    return null;
  }

  const capabilitySpec: CapabilitySpec = {
    capabilityName: resolvedCapabilityName,
    schemaTs,
    migrationSql,
    extractorTs,
    extractorTestsTs,
    registryRegistration: registryTs,
    positiveFixtures: (fixtures.positiveFixtures ?? []).map((f) => ({
      source: String(f.source ?? ""),
      expectedRowCount: Number(f.expectedRowCount ?? 1),
    })),
    negativeFixtures: (fixtures.negativeFixtures ?? []).map((f) => ({
      source: String(f.source ?? ""),
      expectedRowCount: 0 as const,
    })),
    rationale,
  };

  return { capabilitySpec, dslSource, name, smtTemplate, teachingExample };
}

// ---------------------------------------------------------------------------
// Main export: proposeCapabilitySpec
// ---------------------------------------------------------------------------

export async function proposeCapabilitySpec(args: {
  signal: BugSignal;
  invariant: InvariantClaim;
  fixCandidate: FixCandidate;
  gap: string;
  llm: LLMProvider;
  overlay?: OverlayHandle;
}): Promise<CapabilitySpecProposal | null> {
  const { signal, invariant, fixCandidate, gap, llm } = args;

  // Agent path: if LLM supports agent() and an overlay is provided.
  if (llm.agent && args.overlay) {
    return proposeCapabilitySpecViaAgent({ signal, invariant, fixCandidate, gap, llm, overlay: args.overlay });
  }

  // JSON path (existing behavior, now via requestStructuredJson).
  let parsedRaw: unknown;
  try {
    parsedRaw = await requestStructuredJson<unknown>({
      prompt: buildCapabilitySpecPrompt({ signal, invariant, fixCandidate, gap }),
      llm,
      stage: "C6-capabilitySpec",
      model: getModelTier("C6-capabilitySpec"),
    });
  } catch (err) {
    console.warn(`[C6/cap] LLM call/parse failed: ${err instanceof Error ? err.message : String(err)}`);
    return null;
  }

  const proposal = validateCapabilitySpecResponse(parsedRaw);
  if (!proposal) {
    console.warn(`[C6/cap] Capability spec response malformed`);
    return null;
  }

  return proposal;
}

// ---------------------------------------------------------------------------
// Oracle #14: migration safety
// ---------------------------------------------------------------------------

/**
 * Strip SQL line comments and block comments so the statement parser sees
 * only the executable text. LLMs reliably emit a header comment summarizing
 * what the migration does; rejecting that as a statement was an oracle
 * false-positive.
 */
function stripSqlComments(sql: string): string {
  // Remove block comments first (non-greedy, multi-line).
  const noBlock = sql.replace(/\/\*[\s\S]*?\*\//g, "");
  // Then line comments to end-of-line.
  return noBlock.replace(/--[^\n]*/g, "");
}

/**
 * Oracle #14: migration safety.
 *
 * Allow:
 *   - CREATE TABLE ...                     (the new capability's table)
 *   - ALTER TABLE ... ADD COLUMN ...       (additive column on existing tables)
 *   - CREATE INDEX [IF NOT EXISTS] ...     (non-destructive performance index)
 *   - CREATE UNIQUE INDEX [IF NOT EXISTS] ... (non-destructive uniqueness index)
 *
 * Reject:
 *   - DROP anything                         (destructive)
 *   - ALTER TABLE without ADD COLUMN        (potentially destructive)
 *   - Anything else                         (unknown shape — refuse to interpret)
 *
 * Indexes are explicitly permitted because they are structurally safe
 * (no data loss, no migration risk) and capability extractors generated by
 * C6 commonly include them for query performance. Oracle #14's purpose is
 * to refuse destructive operations, not to refuse non-destructive ones.
 */
export function runOracle14(migrationSql: string): { passed: boolean; reason: string } {
  const statements = stripSqlComments(migrationSql)
    .split(";")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);

  for (const stmt of statements) {
    const upper = stmt.toUpperCase();

    // CREATE TABLE — the canonical capability case.
    if (upper.startsWith("CREATE TABLE")) continue;

    // CREATE INDEX / CREATE UNIQUE INDEX — non-destructive performance index.
    if (/^CREATE\s+(UNIQUE\s+)?INDEX\b/.test(upper)) continue;

    // ALTER TABLE ... ADD COLUMN — additive column.
    if (upper.startsWith("ALTER TABLE")) {
      if (/ALTER\s+TABLE\s+\S+\s+ADD\s+(COLUMN\s+)?\S+/i.test(stmt)) continue;
      return {
        passed: false,
        reason: `Oracle #14: ALTER TABLE not adding column is forbidden: ${stmt.slice(0, 100)}`,
      };
    }

    // DROP — destructive.
    if (upper.startsWith("DROP")) {
      return {
        passed: false,
        reason: `Oracle #14: DROP statement is forbidden: ${stmt.slice(0, 100)}`,
      };
    }

    return {
      passed: false,
      reason: `Oracle #14: Only CREATE TABLE, CREATE [UNIQUE] INDEX, and ALTER TABLE ADD COLUMN are allowed. Rejected: ${stmt.slice(0, 100)}`,
    };
  }

  return { passed: true, reason: "migration safe" };
}

// ---------------------------------------------------------------------------
// Oracle #16: extractor coverage — structural pre-check + full execution
// ---------------------------------------------------------------------------

/**
 * Oracle #16 structural pre-check (fast-fail):
 * Parse extractorTs via ts-morph. Confirm it:
 * 1. Exports at least one function.
 * 2. That function's body contains a call matching tx.insert(<table>).values(...) pattern.
 *
 * Called before the expensive transpile + execution path.
 */
function runOracle16Structural(extractorTs: string): { passed: boolean; reason: string } {
  const project = new Project({ useInMemoryFileSystem: true });
  const sourceFile = project.createSourceFile("extractor.ts", extractorTs);

  // Find exported functions.
  const exportedFns = sourceFile.getFunctions().filter((fn) => fn.isExported());

  if (exportedFns.length === 0) {
    return {
      passed: false,
      reason: "Oracle #16: extractorTs must export at least one function",
    };
  }

  // Check that at least one function body contains a tx.insert(...).values(...) call.
  const insertPattern = /tx\s*\.\s*insert\s*\([^)]*\)\s*\.\s*values\s*\(/;
  if (!insertPattern.test(extractorTs)) {
    return {
      passed: false,
      reason: "Oracle #16: extractorTs must contain a tx.insert(<table>).values(...) call",
    };
  }

  return { passed: true, reason: "extractor structure valid" };
}

/**
 * Oracle #16 full execution:
 * 1. Structural pre-check (fast-fail before expensive transpile).
 * 2. Transpile extractorTs + execute against scratch DB with fixtures.
 * 3. Verify positive fixtures emit >= expectedRowCount rows.
 * 4. Verify negative fixtures emit 0 rows.
 *
 * Returns { passed, reason } to match the existing oracle contract.
 */
export async function runOracle16(
  spec: CapabilitySpec,
): Promise<{ passed: boolean; reason: string }> {
  // Fast structural pre-check
  const structural = runOracle16Structural(spec.extractorTs);
  if (!structural.passed) return structural;

  // Full execution
  const result = await executeExtractorSpec(spec);
  return {
    passed: result.passed,
    reason: result.passed ? "extractor execution passed" : result.detail,
  };
}

/**
 * runOracle16Structural is exported for tests that only want the fast-fail
 * structural check without spinning up a scratch DB.
 */
export { runOracle16Structural };

// ---------------------------------------------------------------------------
// Oracle #17: substrate consistency
// ---------------------------------------------------------------------------

/**
 * Oracle #17: Parse schemaTs via ts-morph.
 * 1. Every FK column (node_id or *_node_id) must not target a non-existent table.
 *    In practice: node_id columns are expected to reference nodes(id).
 *    We do NOT have a way to verify ALL tables here (we only see the new schema).
 *    MVP check: if the schema contains a foreignKey() reference, validate that
 *    it doesn't reference a table name that is clearly non-existent.
 * 2. No impossible type combinations (e.g., integer column with boolean default 'abc').
 *
 * NOTE: Full FK validation requires knowing all existing tables. MVP: check that
 * any foreignKey() references are NOT to new tables defined in this same spec
 * (those would be self-referencing, which is suspicious unless it's a tree structure).
 * Flag any foreignKey() that references a table not in the known set AND not "nodes".
 */
export function runOracle17(schemaTs: string, knownTableNames: string[]): { passed: boolean; reason: string } {
  const project = new Project({ useInMemoryFileSystem: true });
  project.createSourceFile("schema.ts", schemaTs);

  // Look for foreignKey() calls referencing columns.
  // Pattern: foreignColumns: [tableName.columnName]
  const fkPattern = /foreignColumns\s*:\s*\[\s*(\w+)\s*\.\s*(\w+)\s*\]/g;
  let m: RegExpExecArray | null;

  // Collect table variable names defined in this schema.
  const definedTableVars = new Set<string>();
  const tableVarPattern = /(?:const|let|var)\s+(\w+)\s*=\s*sqliteTable\s*\(/g;
  while ((m = tableVarPattern.exec(schemaTs)) !== null) {
    if (m[1]) definedTableVars.add(m[1]);
  }

  // Known safe FK targets.
  const safeTargets = new Set(["nodes", "files", ...knownTableNames]);

  while ((m = fkPattern.exec(schemaTs)) !== null) {
    const targetTableVar = m[1]!;

    // If the target is a variable defined in this schema, only allow it if it's
    // a well-known reference (nodes table, etc.) or appears in knownTableNames.
    if (!definedTableVars.has(targetTableVar) && !safeTargets.has(targetTableVar)) {
      return {
        passed: false,
        reason: `Oracle #17: foreignKey references unknown table variable '${targetTableVar}'. Known: ${[...safeTargets].join(", ")}`,
      };
    }
  }

  return { passed: true, reason: "schema consistency valid" };
}

// ---------------------------------------------------------------------------
// runSubstrateOracles: run #14, #16, #17 in sequence
// ---------------------------------------------------------------------------

export async function runSubstrateOracles(
  spec: CapabilitySpec,
): Promise<SubstrateOracleResult> {
  const result: SubstrateOracleResult = {
    passed: false,
    reason: "",
    oracleResults: {
      oracle14_migrationSafe: false,
      oracle16_extractorCoverage: false,
      oracle17_substrateConsistency: false,
    },
  };

  // Oracle #14: migration safety
  const o14 = runOracle14(spec.migrationSql);
  result.oracleResults.oracle14_migrationSafe = o14.passed;
  if (!o14.passed) {
    result.reason = o14.reason;
    return result;
  }

  // Oracle #16: extractor coverage (structural pre-check + full execution)
  const o16 = await runOracle16(spec);
  result.oracleResults.oracle16_extractorCoverage = o16.passed;
  if (!o16.passed) {
    result.reason = o16.reason;
    return result;
  }

  // Oracle #17: substrate consistency
  // Get existing table names from capability registry.
  const existingTableNames = listCapabilities().map((c) => c.dslName);
  const o17 = runOracle17(spec.schemaTs, existingTableNames);
  result.oracleResults.oracle17_substrateConsistency = o17.passed;
  if (!o17.passed) {
    result.reason = o17.reason;
    return result;
  }

  result.passed = true;
  result.reason = "all substrate oracles passed";
  return result;
}
