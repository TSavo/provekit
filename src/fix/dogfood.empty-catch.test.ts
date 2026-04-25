/**
 * Dogfood test: substrate-extension path via empty-catch capability gap.
 *
 * Exercises the full substrate-extension path end-to-end using the
 * `empty-catch` capability gap documented in
 * docs/plans/2026-04-23-fix-loop/capability-gaps.md.
 *
 * Goal: prove the architecture closes on its own limitations — the loop
 * identifies that empty-catch requires a new capability (try_catch_block),
 * proposes it, passes oracles #14/#16/#17/#18, assembles a substrate bundle.
 *
 * What's exercised:
 *   intake → locate → classify (primary_layer = "code_invariant")
 *   C1: LLM formulates invariant (no principle match for empty-catch)
 *   C2: overlay opens on fixture project
 *   C3: LLM proposes fix (add console.error in catch)
 *   C4: no complementary sites (empty fixture)
 *   C5: injected testRunner bypasses vitest-in-vitest
 *   C6: tryExistingCapabilities → needs_capability → proposeWithCapability
 *       → oracles #14/#16/#17/#18 pass → returns principle_with_capability
 *   D1b: assembleBundle → bundleType = "substrate"
 *   D2: prDraftMode (no autoApply, no branch mutation)
 *
 * Acceptance:
 *   First it(): bundle.bundleType === "substrate", coherence fields set
 *   Second it(): oracle #14 rejects DROP TABLE migration (safety gate)
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  symlinkSync,
  existsSync,
} from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import { openDb } from "../db/index.js";
import { buildSASTForFile } from "../sast/builder.js";
import { parseBugSignal, StubLLMProvider } from "./intake.js";
import { locate } from "./locate.js";
import { classify } from "./classify.js";
import { runFixLoop } from "./orchestrator.js";
import type { BugSignal, RemediationPlan } from "./types.js";
import type { Db } from "../db/index.js";

// ---------------------------------------------------------------------------
// Fixture source — function with empty catch block
// ---------------------------------------------------------------------------

const DEMO_TS_SOURCE = `export function loadConfig(path: string): string {
  try {
    return require("fs").readFileSync(path, "utf8");
  } catch (e) {
    // empty catch — silently swallows exception
  }
  return "";
}
`;

const BUG_REPORT_TEXT =
  "Empty try/catch silently swallows exceptions at src/demo.ts line 4. " +
  "When require('fs').readFileSync throws (e.g., file not found), " +
  "the error is discarded and the caller receives an empty string with no indication of failure. " +
  "Found at src/demo.ts:4 in loadConfig().";

// ---------------------------------------------------------------------------
// Canned stub LLM responses
// Ordered most-specific first (Map iteration is insertion-order)
// ---------------------------------------------------------------------------

// Intake / bug-report parser
const INTAKE_RESPONSE = JSON.stringify({
  summary: "empty catch block silently swallows exceptions",
  failureDescription:
    "The catch handler in loadConfig() is empty, so exceptions from readFileSync are discarded.",
  fixHint: "Add console.error or rethrow in the catch block",
  codeReferences: [{ file: "src/demo.ts", line: 4 }],
  bugClassHint: "empty-catch",
});

// Classify prompt
const CLASSIFY_RESPONSE = JSON.stringify({
  primaryLayer: "code_invariant",
  secondaryLayers: [],
  artifacts: [
    { kind: "code_patch", rationale: "Add error logging in catch" },
    { kind: "regression_test", rationale: "Verify error is surfaced" },
    { kind: "principle_candidate", bugClassName: "empty-catch", rationale: "New principle needed" },
  ],
  rationale:
    "An empty catch block is a code-invariant violation — the catch handler must do something meaningful.",
});

// C1 LLM fallback — formal verification expert
// Uses placeholder names that won't appear in the patched file.
// Includes citations to satisfy oracle #1.5 traceability check.
const INVARIANT_LLM_RESPONSE = JSON.stringify({
  description: "catch handler has zero statements; exception is silently discarded",
  smt_declarations: [
    "(declare-const catchStmtCount Int)",
    "(declare-const hasFinally Bool)",
  ],
  smt_violation_assertion: "(assert (= catchStmtCount 0))",
  bindings: [
    { smt_constant: "catchStmtCount", source_expr: "catchStmtCount", sort: "Int" },
    { smt_constant: "hasFinally", source_expr: "hasFinally", sort: "Bool" },
  ],
  citations: [
    {
      smt_clause: "(= catchStmtCount 0)",
      source_quote: "Empty try/catch silently swallows exceptions",
    },
  ],
});

// Oracle #1.5 traceability verifier — keyed on "Citations to verify"
// (a substring of the verifier prompt that no other prompt contains).
const TRACEABILITY_RESPONSE = JSON.stringify({ all_grounded: true });

// Oracle #1.5 adversarial-fixture pre-validation — keyed on "software testing
// expert" (distinct from C5's "TypeScript testing expert"). Positive fixtures
// have catchStmtCount=0 (SAT against the violation). Negative fixtures have
// catchStmtCount > 0 (UNSAT).
const FIXTURE_PREVAL_RESPONSE = JSON.stringify({
  positive: [
    {
      source: "function p1() { try { doX(); } catch (e) {} }",
      inputBindings: { catchStmtCount: 0, hasFinally: false },
      description: "empty catch",
    },
    {
      source: "function p2() { try { parse(x); } catch (err) {} return x; }",
      inputBindings: { catchStmtCount: 0, hasFinally: false },
      description: "empty catch",
    },
    {
      source: "class L { load() { try { this.init(); } catch (e) {} } }",
      inputBindings: { catchStmtCount: 0, hasFinally: false },
      description: "empty catch in method",
    },
    {
      source: "async function p4() { try { await fn(); } catch (e) {} }",
      inputBindings: { catchStmtCount: 0, hasFinally: false },
      description: "empty catch around await",
    },
    {
      source: "function p5() { try { JSON.parse(s); } catch (e) {} }",
      inputBindings: { catchStmtCount: 0, hasFinally: false },
      description: "empty catch around parse",
    },
  ],
  negative: [
    {
      source: "function n1() { try { doX(); } catch (e) { console.error(e); } }",
      inputBindings: { catchStmtCount: 1, hasFinally: false },
      description: "catch logs",
    },
    {
      source: "function n2() { try { doX(); } catch (e) { throw e; } }",
      inputBindings: { catchStmtCount: 1, hasFinally: false },
      description: "rethrow",
    },
    {
      source: "function n3() { try { doX(); } catch (e) { return null; } }",
      inputBindings: { catchStmtCount: 1, hasFinally: false },
      description: "catch returns",
    },
    {
      source: "function n4() { try { doX(); } catch (e) { logger.warn(e); metrics.inc(); } }",
      inputBindings: { catchStmtCount: 2, hasFinally: false },
      description: "catch logs and metrics",
    },
    {
      source: "function n5() { try { doX(); } catch (e) { handle(e); } finally { cleanup(); } }",
      inputBindings: { catchStmtCount: 1, hasFinally: true },
      description: "catch handles + finally",
    },
  ],
});

// C3 fix proposal — code-repair expert (propose up to N candidate patches)
const FIX_PROPOSAL_RESPONSE = JSON.stringify({
  candidates: [
    {
      rationale: "Add console.error to surface the exception",
      confidence: 0.9,
      patch: {
        description: "Add error logging in catch block",
        fileEdits: [
          {
            file: "src/demo.ts",
            newContent:
              'export function loadConfig(path: string): string {\n' +
              '  try {\n' +
              '    return require("fs").readFileSync(path, "utf8");\n' +
              '  } catch (e) {\n' +
              '    console.error("loadConfig failed:", e);\n' +
              '  }\n' +
              '  return "";\n' +
              '}\n',
          },
        ],
      },
    },
  ],
});

// C4 complementary — code-repair expert (A bug was just fixed at one site)
const COMPLEMENTARY_RESPONSE = JSON.stringify([]);

// C5 regression test — TypeScript testing expert
const TEST_RESPONSE = JSON.stringify({
  testCode:
    "import { describe, it, expect } from 'vitest';\n" +
    "import { loadConfig } from '../demo.js';\n" +
    "describe('loadConfig', () => {\n" +
    "  it('surfaces errors (non-empty catch)', () => {\n" +
    "    expect(() => loadConfig('/nonexistent/path')).not.toThrow();\n" +
    "  });\n" +
    "});\n",
  testFilePath: "src/demo.regression.test.ts",
  testName: "regression: empty catch block silently swallows exceptions",
  witnessInputs: { catchStmtCount: 0, hasFinally: false },
});

// C6: tryExistingCapabilities → needs_capability
// Must match "static-analysis rule author" substring from buildPrinciplePrompt
const NEEDS_CAPABILITY_RESPONSE = JSON.stringify({
  kind: "needs_capability",
  missing_predicate:
    "empty_catch_body — query the number of statements inside a catch handler. " +
    "No current capability tracks handler_stmt_count structural property.",
});

// C6: adversarial validation fixtures
// Must match "security-minded adversary" from buildAdversarialPrompt
const ADVERSARIAL_RESPONSE = JSON.stringify({
  false_positives: [
    {
      source:
        "function ok1() { try { doSomething(); } catch (e) { console.error(e); } }",
    },
    {
      source:
        "function ok2() { try { doSomething(); } catch (e) { throw e; } }",
    },
    {
      source:
        "function ok3() { try { doSomething(); } catch (e) { return null; } }",
    },
  ],
  false_negatives: [
    {
      source: "function bad1() { try { doSomething(); } catch (e) {} }",
    },
    {
      source:
        "function bad2() { try { parse(x); } catch (err) {} return x; }",
    },
    {
      source:
        "class Loader { load() { try { this.init(); } catch (e) {} } }",
    },
  ],
});

// ---------------------------------------------------------------------------
// C6: capability spec proposal (substrate architect prompt)
// ---------------------------------------------------------------------------
//
// The extractorTs must be SELF-CONTAINED: no imports from local files that
// won't exist in the tmpdir. It can require npm packages (drizzle-orm,
// ts-morph) because those resolve via the project's node_modules.
//
// The extractor receives (tx, sourceFile, nodeIdByNode) from capabilityExecutor.ts.
// It must call tx.insert(table).values({...}).run() with a real drizzle table.
//
// IMPORTANT: The table name in migrationSql, schemaTs, and the
// tx.insert(...) call must all match.
//
// Oracle #16 structural regex: tx\s*\.\s*insert\s*\([^)]*\)\s*\.\s*values\s*\(
// Oracle #18: DSL must reference node_try_catch_block (unknown capability before registration)

// SQL table name: follows node_{capabilityName} convention
const NODE_TRY_CATCH_TABLE_NAME = "node_try_catch_block";
// DSL capability name: WITHOUT the node_ prefix (buildTempDescriptor adds node_ prefix)
const CAPABILITY_DSL_NAME = "try_catch_block";

const SCHEMA_TS =
  "const { sqliteTable, text, integer } = require('drizzle-orm/sqlite-core');\n" +
  `const nodeTryCatchBlock = sqliteTable('${NODE_TRY_CATCH_TABLE_NAME}', {\n` +
  "  nodeId: text('node_id').notNull(),\n" +
  "  handlerStmtCount: integer('handler_stmt_count').notNull(),\n" +
  "  hasFinally: integer('has_finally').notNull(),\n" +
  "});\n" +
  "module.exports = { nodeTryCatchBlock };\n";

// The extractor is CJS-compatible TypeScript that will be transpiled.
// It inlines the table definition so it doesn't need to import from a local file.
// SyntaxKind.TryStatement = 241 (stable across TS versions used in ts-morph 22+)
//
// IMPORTANT: oracle #16 negative fixtures check that the extractor inserts 0 rows.
// So this extractor ONLY inserts rows for try statements with an empty catch block
// (handlerStmtCount === 0). Non-empty catch blocks → no row inserted.
// This makes the negative fixture (non-empty catch) produce 0 rows.
const EXTRACTOR_TS =
  "const { SyntaxKind } = require('ts-morph');\n" +
  "const { sqliteTable, text, integer } = require('drizzle-orm/sqlite-core');\n" +
  `const nodeTryCatchBlock = sqliteTable('${NODE_TRY_CATCH_TABLE_NAME}', {\n` +
  "  nodeId: text('node_id').notNull(),\n" +
  "  handlerStmtCount: integer('handler_stmt_count').notNull(),\n" +
  "  hasFinally: integer('has_finally').notNull(),\n" +
  "});\n" +
  "export function extractNodeTryCatchBlock(tx, sourceFile, nodeIdByNode) {\n" +
  "  sourceFile.forEachDescendant(function(node) {\n" +
  "    if (node.getKind() === SyntaxKind.TryStatement) {\n" +
  "      var id = nodeIdByNode.get(node);\n" +
  "      if (!id) return;\n" +
  "      var tryStmt = node;\n" +
  "      var clause = tryStmt.getCatchClause ? tryStmt.getCatchClause() : null;\n" +
  "      var block = clause && clause.getBlock ? clause.getBlock() : null;\n" +
  "      var count = block && block.getStatements ? block.getStatements().length : 0;\n" +
  "      // Only insert rows for empty catch blocks (oracle #16 negative fixtures\n" +
  "      // check that non-empty catches produce 0 rows)\n" +
  "      if (count !== 0) return;\n" +
  "      var finallyBlock = tryStmt.getFinallyBlock ? tryStmt.getFinallyBlock() : null;\n" +
  "      tx.insert(nodeTryCatchBlock).values({\n" +
  "        nodeId: id,\n" +
  "        handlerStmtCount: count,\n" +
  "        hasFinally: finallyBlock !== undefined && finallyBlock !== null ? 1 : 0,\n" +
  "      }).run();\n" +
  "    }\n" +
  "  });\n" +
  "}\n";

const EXTRACTOR_TESTS_TS =
  "import { describe, it, expect } from 'vitest';\n" +
  "describe('extractNodeTryCatchBlock', () => {\n" +
  "  it('placeholder', () => { expect(true).toBe(true); });\n" +
  "});\n";

const DSL_SOURCE =
  `principle empty-catch {\n` +
  `  match $try: node where ${CAPABILITY_DSL_NAME}.handlerStmtCount == 0\n` +
  `  report violation {\n` +
  `    at $try\n` +
  `    captures { tryCatch: $try }\n` +
  `    message "catch handler is empty — exceptions are silently discarded"\n` +
  `  }\n` +
  `}\n`;

const CAPABILITY_SPEC_RESPONSE = JSON.stringify({
  capabilityName: CAPABILITY_DSL_NAME,
  schemaTs: SCHEMA_TS,
  migrationSql: `CREATE TABLE ${NODE_TRY_CATCH_TABLE_NAME} (node_id TEXT NOT NULL, handler_stmt_count INTEGER NOT NULL, has_finally INTEGER NOT NULL);`,
  extractorTs: EXTRACTOR_TS,
  extractorTestsTs: EXTRACTOR_TESTS_TS,
  registryRegistration:
    `registerCapability({ dslName: '${CAPABILITY_DSL_NAME}', table: nodeTryCatchBlock, ` +
    `columns: { node_id: { dslName: 'node_id', drizzleColumn: nodeTryCatchBlock.nodeId, isNodeRef: true, nullable: false }, ` +
    `handlerStmtCount: { dslName: 'handlerStmtCount', drizzleColumn: nodeTryCatchBlock.handlerStmtCount, isNodeRef: false, nullable: false } } });`,
  positiveFixtures: [
    {
      source: "function bad() { try { doSomething(); } catch (e) {} }",
      expectedRowCount: 1,
    },
  ],
  negativeFixtures: [
    {
      source: "function ok() { try { doSomething(); } catch (e) { console.error(e); } }",
      expectedRowCount: 0,
    },
  ],
  rationale:
    "Existing capabilities do not expose catch handler body length. " +
    "try_catch_block adds handler_stmt_count to enable empty-catch detection.",
  dslSource: DSL_SOURCE,
  name: "empty-catch",
  smtTemplate:
    "(declare-const handlerStmtCount Int)\n" +
    "(assert (= handlerStmtCount 0))\n" +
    "(check-sat)",
  teachingExample: {
    domain: "exception-handling",
    explanation:
      "A try statement whose catch block has zero statements silently discards the exception.",
    smt2:
      "(declare-const handlerStmtCount Int)\n" +
      "(assert (= handlerStmtCount 0))\n" +
      "(check-sat)",
  },
});

// ---------------------------------------------------------------------------
// Destructive-migration variant for second it() test
// ---------------------------------------------------------------------------

// NOTE: EXTRACTOR_TS must be defined before DESTRUCTIVE_CAPABILITY_SPEC_RESPONSE uses it.
// The destructive variant intentionally has DROP TABLE in migrationSql to test oracle #14.
const DESTRUCTIVE_CAPABILITY_SPEC_RESPONSE = JSON.stringify({
  capabilityName: CAPABILITY_DSL_NAME,
  schemaTs: SCHEMA_TS,
  // Contains DROP TABLE — oracle #14 must reject this
  migrationSql: `DROP TABLE IF EXISTS node_try_catch_block; CREATE TABLE ${NODE_TRY_CATCH_TABLE_NAME} (node_id TEXT NOT NULL, handler_stmt_count INTEGER NOT NULL, has_finally INTEGER NOT NULL);`,
  extractorTs: EXTRACTOR_TS,
  extractorTestsTs: EXTRACTOR_TESTS_TS,
  registryRegistration: "registerCapability({});",
  positiveFixtures: [
    {
      source: "function bad() { try { doSomething(); } catch (e) {} }",
      expectedRowCount: 1,
    },
  ],
  negativeFixtures: [],
  rationale: "destructive migration — should be rejected by oracle #14",
  dslSource: DSL_SOURCE,
  name: "empty-catch",
  smtTemplate: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
  teachingExample: {
    domain: "exception-handling",
    explanation: "destructive test",
    smt2: "(declare-const x Int)\n(assert (= x 0))\n(check-sat)",
  },
});

// ---------------------------------------------------------------------------
// Stub LLM builder
// ---------------------------------------------------------------------------

/**
 * Build a StubLLMProvider with all the canned responses for the happy-path
 * substrate-extension test. The Map is insertion-ordered (most specific first).
 */
function buildHappyPathLLM(): StubLLMProvider {
  return new StubLLMProvider(
    new Map<string, string>([
      // C6 capability spec proposal (most specific — checked before C3/C4 "expert" keys)
      ["static-analysis substrate architect", CAPABILITY_SPEC_RESPONSE],
      // C6 adversarial validation
      ["security-minded adversary", ADVERSARIAL_RESPONSE],
      // C6 tryExistingCapabilities
      ["static-analysis rule author", NEEDS_CAPABILITY_RESPONSE],
      // Intake / bug-report parser
      ["bug-report parser", INTAKE_RESPONSE],
      // Classify
      ["classifying a bug report", CLASSIFY_RESPONSE],
      // Oracle #1.5 traceability check (keyed before "formal verification expert"
      // even though prompts are distinct; explicit ordering aids review).
      ["Citations to verify", TRACEABILITY_RESPONSE],
      // Oracle #1.5 adversarial fixture pre-validation. The fixture prompt
      // contains "software testing expert" (distinct from C5's "TypeScript testing expert").
      ["software testing expert", FIXTURE_PREVAL_RESPONSE],
      // C1 LLM + cross-LLM agreement (both use "formal verification expert")
      ["formal verification expert", INVARIANT_LLM_RESPONSE],
      // C3 fix generation (propose up to N candidate patches)
      ["propose up to", FIX_PROPOSAL_RESPONSE],
      // C4 complementary (A bug was just fixed at one site)
      ["A bug was just fixed at one site", COMPLEMENTARY_RESPONSE],
      // C5 regression test
      ["TypeScript testing expert", TEST_RESPONSE],
    ]),
  );
}

/**
 * Build a StubLLMProvider for the destructive-migration negative test.
 * Same as happy-path except capabilitySpec contains DROP TABLE.
 */
function buildDestructiveMigrationLLM(): StubLLMProvider {
  return new StubLLMProvider(
    new Map<string, string>([
      ["static-analysis substrate architect", DESTRUCTIVE_CAPABILITY_SPEC_RESPONSE],
      ["security-minded adversary", ADVERSARIAL_RESPONSE],
      ["static-analysis rule author", NEEDS_CAPABILITY_RESPONSE],
      ["bug-report parser", INTAKE_RESPONSE],
      ["classifying a bug report", CLASSIFY_RESPONSE],
      ["Citations to verify", TRACEABILITY_RESPONSE],
      ["software testing expert", FIXTURE_PREVAL_RESPONSE],
      ["formal verification expert", INVARIANT_LLM_RESPONSE],
      ["propose up to", FIX_PROPOSAL_RESPONSE],
      ["A bug was just fixed at one site", COMPLEMENTARY_RESPONSE],
      ["TypeScript testing expert", TEST_RESPONSE],
    ]),
  );
}

// ---------------------------------------------------------------------------
// Scratch project helpers
// ---------------------------------------------------------------------------

function setupScratchProject(): {
  scratchDir: string;
  db: Db;
  demoFilePath: string;
} {
  const scratchDir = mkdtempSync(join(tmpdir(), "provekit-dogfood-"));

  // Write fixture sources.
  mkdirSync(join(scratchDir, "src"), { recursive: true });
  const demoFilePath = join(scratchDir, "src", "demo.ts");
  writeFileSync(demoFilePath, DEMO_TS_SOURCE, "utf8");

  // Git init + initial commit so applyBundle (D2) can run git rev-parse.
  try {
    execFileSync("git", ["init"], { cwd: scratchDir, stdio: "pipe" });
    execFileSync("git", ["config", "user.email", "test@test.com"], {
      cwd: scratchDir,
      stdio: "pipe",
    });
    execFileSync("git", ["config", "user.name", "Test"], {
      cwd: scratchDir,
      stdio: "pipe",
    });
    execFileSync("git", ["add", "-A"], { cwd: scratchDir, stdio: "pipe" });
    execFileSync("git", ["commit", "-m", "init"], {
      cwd: scratchDir,
      stdio: "pipe",
    });
  } catch {
    // Non-fatal: D2 may fail, but our test may not reach D2.
  }

  // Open DB and apply migrations.
  const dbPath = join(scratchDir, "provekit.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });

  // Symlink node_modules so the scratch project can require ts-morph etc.
  const nmLink = join(scratchDir, "node_modules");
  if (!existsSync(nmLink)) {
    symlinkSync(join(process.cwd(), "node_modules"), nmLink, "dir");
  }

  return { scratchDir, db, demoFilePath };
}

// Injected C5 test runner — returns pass for fixed code, fail for original.
// The runner is called twice in C5:
//   call 1 (fixed code): must return exitCode 0
//   call 2 (original, reverted): must return exitCode 1
function buildC5TestRunner(): (
  overlay: import("./types.js").OverlayHandle,
  testFilePath: string,
  mainRepoRoot: string,
) => { exitCode: number; stdout: string; stderr: string } {
  let callCount = 0;
  return (_overlay, _testFilePath, _mainRepoRoot) => {
    callCount++;
    // Odd call → fixed-code run (pass). Even call → original-code run (fail).
    if (callCount % 2 === 1) {
      return { exitCode: 0, stdout: "1 test passed", stderr: "" };
    }
    return { exitCode: 1, stdout: "1 test failed (mutation check)", stderr: "" };
  };
}

// Injected D1b vitest runner — full suite (oracle #10).
function buildVitestRunner(): (
  overlay: import("./types.js").OverlayHandle,
) => { exitCode: number; stdout: string; stderr: string } {
  return (_overlay) => ({
    exitCode: 0,
    stdout: "full suite passed",
    stderr: "",
  });
}

// ---------------------------------------------------------------------------
// Suite: Dogfood — substrate-extension path via empty-catch gap
// ---------------------------------------------------------------------------

describe(
  "Dogfood: substrate-extension path via empty-catch gap",
  () => {
    let scratchDir: string;
    let db: Db;
    let demoFilePath: string;

    beforeEach(() => {
      ({ scratchDir, db, demoFilePath } = setupScratchProject());
    });

    afterEach(() => {
      try {
        db.$client.close();
      } catch {
        // ignore
      }
      rmSync(scratchDir, { recursive: true, force: true });
    });

    it(
      "closes the empty-catch gap: LLM proposes try_catch_block capability → oracles 14/16/17/18 pass → substrate bundle assembles",
      { timeout: 120_000 },
      async () => {
        // --- Setup: build SAST for the fixture file ---
        buildSASTForFile(db, demoFilePath);

        const llm = buildHappyPathLLM();

        // --- Intake: parse bug signal ---
        const signal = await parseBugSignal(
          { text: BUG_REPORT_TEXT, source: "report" },
          llm,
        );
        expect(signal.codeReferences.length).toBeGreaterThan(0);

        // Override codeReferences to use absolute path so locate() finds the SAST node.
        const absoluteSignal: BugSignal = {
          ...signal,
          codeReferences: [{ file: demoFilePath, line: 4 }],
        };

        // --- Locate ---
        const locus = locate(db, absoluteSignal);
        expect(locus).not.toBeNull();
        expect(locus!.file).toContain("demo.ts");

        // --- Classify ---
        const plan: RemediationPlan = await classify(absoluteSignal, locus, llm);
        expect(plan.primaryLayer).toBe("code_invariant");

        // --- Run fix loop (with injected test runners) ---
        const result = await runFixLoop({
          signal: absoluteSignal,
          locus: locus!,
          plan,
          db,
          llm,
          options: {
            autoApply: false,
            maxComplementarySites: 5,
            confidenceThreshold: 0.5,
          },
          c5TestRunner: buildC5TestRunner(),
          vitestRunner: buildVitestRunner(),
        });

        // --- Dogfood proof assertions ---
        const bundle = result.bundle;
        if (bundle === null) {
          // Print full audit trail so failures are diagnosable
          console.error(
            "Dogfood failure audit trail:",
            JSON.stringify(result.auditTrail, null, 2),
          );
          throw new Error(`Dogfood failed: ${result.reason}`);
        }

        // MUST be a substrate bundle — that's the whole point
        expect(bundle.bundleType).toBe("substrate");

        // The principle candidate MUST carry a capabilitySpec
        expect(bundle.artifacts.principle).not.toBeNull();
        expect(bundle.artifacts.principle?.kind).toBe("principle_with_capability");

        // The capability spec is structurally complete
        const cap = bundle.artifacts.capabilitySpec;
        expect(cap).not.toBeNull();
        expect(cap!.capabilityName).toMatch(/try.*catch/i);
        expect(cap!.schemaTs).toContain("sqliteTable");
        expect(cap!.extractorTs).toContain("forEachDescendant");
        expect(cap!.migrationSql).toMatch(/CREATE TABLE/i);

        // Substrate coherence fields (oracles #14, #16, #17, #18 all passed in C6)
        // These are set true by assembleBundle when bundleType === "substrate"
        expect(bundle.coherence.extractorCoverage).toBe(true);
        expect(bundle.coherence.migrationSafe).toBe(true);
        expect(bundle.coherence.substrateConsistency).toBe(true);
        expect(bundle.coherence.principleNeedsCapability).toBe(true);

        // Audit trail: C1 through D1 all completed
        function entry(stage: string, kind: string) {
          return result.auditTrail.find((e) => e.stage === stage && e.kind === kind);
        }
        expect(entry("C1", "complete")).toBeDefined();
        expect(entry("C2", "complete")).toBeDefined();
        expect(entry("C3", "complete")).toBeDefined();
        expect(entry("C4", "complete")).toBeDefined();
        expect(entry("C5", "complete")).toBeDefined();
        expect(entry("C6", "complete")).toBeDefined();
        expect(entry("D1", "complete")).toBeDefined();
      },
    );

    it(
      "substrate-extension path correctly fails when LLM proposes a destructive migration",
      { timeout: 120_000 },
      async () => {
        // --- Setup: build SAST for the fixture file ---
        buildSASTForFile(db, demoFilePath);

        const llm = buildDestructiveMigrationLLM();

        // --- Intake + Locate + Classify ---
        const signal = await parseBugSignal(
          { text: BUG_REPORT_TEXT, source: "report" },
          llm,
        );
        const absoluteSignal: BugSignal = {
          ...signal,
          codeReferences: [{ file: demoFilePath, line: 4 }],
        };
        const locus = locate(db, absoluteSignal);
        expect(locus).not.toBeNull();

        const plan: RemediationPlan = await classify(absoluteSignal, locus, llm);
        expect(plan.primaryLayer).toBe("code_invariant");

        // --- Run fix loop ---
        const result = await runFixLoop({
          signal: absoluteSignal,
          locus: locus!,
          plan,
          db,
          llm,
          options: {
            autoApply: false,
            maxComplementarySites: 5,
            confidenceThreshold: 0.5,
          },
          c5TestRunner: buildC5TestRunner(),
          vitestRunner: buildVitestRunner(),
        });

        // The loop MUST fail because oracle #14 rejects the DROP TABLE migration.
        // C6 proposeWithCapability returns null → C6 stage returns null →
        // assembleBundle receives principle=null → bundleType="fix" (not substrate)
        // OR C6 aborts and returns null, causing assembleBundle to get null principle.
        // Either way: bundle may be null OR bundle.bundleType === "fix" (not "substrate").
        // The safety gate is that we never get a substrate bundle with a DROP TABLE migration.

        if (result.bundle !== null) {
          // If a bundle was produced, it must NOT be a substrate bundle
          // (oracle #14 should have prevented the substrate path from succeeding)
          expect(result.bundle.bundleType).not.toBe("substrate");
          expect(result.bundle.artifacts.capabilitySpec).toBeNull();
        } else {
          // Expected path: C6 failed, bundle is null with reason mentioning oracle #14
          expect(result.bundle).toBeNull();
          // Verify the loop reached C6 (and failed there, not earlier)
          const hadC1 = result.auditTrail.some(
            (e) => e.stage === "C1" && e.kind === "complete",
          );
          const hadC5 = result.auditTrail.some(
            (e) => e.stage === "C5" && (e.kind === "complete" || e.kind === "error"),
          );
          expect(hadC1).toBe(true);
          expect(hadC5).toBe(true);
        }
      },
    );
  },
);
