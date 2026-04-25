/**
 * Dogfood test: substrate-extension path via shell-injection capability gap.
 *
 * Mirrors dogfood.empty-catch.test.ts. Exercises the full substrate-extension
 * path end-to-end using the shell-injection bug class — the canonical "hard"
 * case from docs/plans/2026-04-25-pitch-leaks.md Leak 2.
 *
 * Goal: prove the architecture closes a non-trivial bug class. The loop:
 *   - identifies that shell-injection requires a new capability
 *     (string_composition: tracks template/concat/literal expressions and
 *     whether they have interpolation),
 *   - proposes the capability + a DSL principle that uses it,
 *   - passes oracles #14/#16/#17/#18,
 *   - assembles a substrate bundle,
 *   - the principle library now catches shell-injection.
 *
 * What's exercised:
 *   intake -> locate -> classify (primary_layer = "code_invariant")
 *   C1: invariant "tainted user input must not flow into execSync's argument"
 *   C2: overlay opens on fixture project
 *   C3: patch switches to execFile (or argv form) which avoids the shell
 *   C4: no complementary sites (single-file fixture)
 *   C5: regression test
 *   C6: tryExistingCapabilities -> needs_capability -> proposeWithCapability
 *       -> oracles #14/#16/#17/#18 pass -> returns principle_with_capability
 *   D1b: assembleBundle -> bundleType = "substrate"
 *   D2: prDraftMode (no autoApply, mirroring empty-catch)
 *
 * Acceptance:
 *   - bundle.bundleType === "substrate"
 *   - bundle.artifacts.principle.kind === "principle_with_capability"
 *   - capabilitySpec.capabilityName matches /string.*composition/i
 *   - All four substrate-coherence flags true
 *   - Audit trail has C1..D1 complete entries
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
// Fixture source: function with shell-injection vulnerability.
// listFiles(input) interpolates user input into a backtick template that is
// passed to execSync. A malicious value like '; rm -rf /' would execute.
// ---------------------------------------------------------------------------

const DEMO_TS_SOURCE = `import { execSync } from "child_process";
export function listFiles(input: string): Buffer {
  return execSync(\`ls \${input}\`);
}
`;

const BUG_REPORT_TEXT =
  "Shell injection in cmd.ts: listFiles(input) interpolates input into a " +
  "backtick template passed to execSync. A malicious value like '; rm -rf /' " +
  "executes arbitrary commands. Found at src/cmd.ts:3 in listFiles().";

// ---------------------------------------------------------------------------
// Canned stub LLM responses.
// Map iteration is insertion-order; most-specific keys must come first.
// ---------------------------------------------------------------------------

const INTAKE_RESPONSE = JSON.stringify({
  summary: "shell injection via interpolated template into execSync",
  failureDescription:
    "listFiles(input) interpolates a function parameter into a backtick template " +
    "passed to execSync. The shell parses the resulting string, so input values " +
    "containing metacharacters (semicolons, backticks, &&, etc.) execute as " +
    "additional commands.",
  fixHint:
    "Switch to execFile, which takes the args array and never invokes a shell. " +
    "Alternatively, validate or escape the input.",
  codeReferences: [{ file: "src/cmd.ts", line: 3 }],
  bugClassHint: "shell-injection",
});

const CLASSIFY_RESPONSE = JSON.stringify({
  primaryLayer: "code_invariant",
  secondaryLayers: [],
  artifacts: [
    { kind: "code_patch", rationale: "Switch execSync to execFile" },
    { kind: "regression_test", rationale: "Verify metacharacters do not execute" },
    {
      kind: "principle_candidate",
      bugClassName: "shell-injection",
      rationale:
        "New principle needed. Existing capabilities cannot express " +
        "interpolation-into-shell-call without a string_composition capability.",
    },
  ],
  rationale:
    "An execSync argument carrying interpolated parameter data is a code-invariant " +
    "violation. The argument must be a constant or a sanitized value, never a " +
    "raw template containing user input.",
});

const INVARIANT_LLM_RESPONSE = JSON.stringify({
  description:
    "tainted user input must not flow into the argument of execSync, exec, or spawn",
  smt_declarations: [
    "(declare-const argHasInterpolation Bool)",
    "(declare-const interpSourceIsParam Bool)",
    "(declare-const calleeIsShellExec Bool)",
  ],
  smt_violation_assertion:
    "(assert (and calleeIsShellExec argHasInterpolation interpSourceIsParam))",
  bindings: [
    { smt_constant: "argHasInterpolation", source_expr: "argHasInterpolation", sort: "Bool" },
    { smt_constant: "interpSourceIsParam", source_expr: "interpSourceIsParam", sort: "Bool" },
    { smt_constant: "calleeIsShellExec", source_expr: "calleeIsShellExec", sort: "Bool" },
  ],
  // Oracle #1.5 traceability: each clause is grounded in a verbatim phrase
  // from BUG_REPORT_TEXT.
  citations: [
    {
      smt_clause: "(= calleeIsShellExec true)",
      source_quote: "passed to execSync",
    },
    {
      smt_clause: "(= argHasInterpolation true)",
      source_quote: "interpolates input into a backtick template",
    },
    {
      smt_clause: "(= interpSourceIsParam true)",
      source_quote: "interpolates input into a backtick template",
    },
  ],
});

// Oracle #1.5 traceability verifier — the prompt contains "Citations to verify".
// Use a discriminating substring that no other prompt contains.
const TRACEABILITY_RESPONSE = JSON.stringify({ all_grounded: true });

// Oracle #1.5 adversarial-fixture pre-validation. The prompt is built by
// adversarialFixturePreValidation() and contains "software testing expert".
// classifyFixture substitutes inputBindings into the violation SMT and runs
// Z3: positive expects SAT (all three flags true), negative expects UNSAT
// (at least one flag false makes the AND false).
const FIXTURE_PREVAL_RESPONSE = JSON.stringify({
  positive: [
    {
      source: "function p1(input: string){ return require('child_process').execSync(`ls ${input}`); }",
      inputBindings: { argHasInterpolation: true, interpSourceIsParam: true, calleeIsShellExec: true },
      description: "interpolated template reaches execSync",
    },
    {
      source: "function p2(p: string){ return require('child_process').execSync(`echo ${p}`); }",
      inputBindings: { argHasInterpolation: true, interpSourceIsParam: true, calleeIsShellExec: true },
      description: "interpolated template reaches execSync",
    },
    {
      source: "function p3(file: string){ return require('child_process').exec(`cat ${file}`); }",
      inputBindings: { argHasInterpolation: true, interpSourceIsParam: true, calleeIsShellExec: true },
      description: "interpolated template reaches exec",
    },
    {
      source: "function p4(name: string){ return require('child_process').execSync(`echo ${name} > log`); }",
      inputBindings: { argHasInterpolation: true, interpSourceIsParam: true, calleeIsShellExec: true },
      description: "interpolated template reaches execSync",
    },
    {
      source: "function p5(arg: string){ return require('child_process').spawn(`bash -c '${arg}'`); }",
      inputBindings: { argHasInterpolation: true, interpSourceIsParam: true, calleeIsShellExec: true },
      description: "interpolated template reaches spawn",
    },
  ],
  negative: [
    {
      source: "function n1(){ return require('child_process').execSync('ls /tmp'); }",
      inputBindings: { argHasInterpolation: false, interpSourceIsParam: false, calleeIsShellExec: true },
      description: "no interpolation",
    },
    {
      source: "function n2(input: string){ return require('child_process').execFileSync('ls', [input]); }",
      inputBindings: { argHasInterpolation: false, interpSourceIsParam: true, calleeIsShellExec: false },
      description: "execFile bypasses shell",
    },
    {
      source: "function n3(){ const cmd = 'ls'; return require('child_process').execSync(cmd); }",
      inputBindings: { argHasInterpolation: false, interpSourceIsParam: false, calleeIsShellExec: true },
      description: "literal command, no interpolation",
    },
    {
      source: "function n4(input: string){ const safe = require('shell-quote').quote([input]); return require('child_process').execSync(`ls ${safe}`); }",
      inputBindings: { argHasInterpolation: true, interpSourceIsParam: false, calleeIsShellExec: true },
      description: "interpolated value is sanitized literal, not raw param",
    },
    {
      source: "function n5(input: string){ console.log(`hello ${input}`); }",
      inputBindings: { argHasInterpolation: true, interpSourceIsParam: true, calleeIsShellExec: false },
      description: "no shell call",
    },
  ],
});

// C3 fix: switch to execFile which takes argv and never invokes a shell.
// This is the cleanest of the three options (escape vs argv vs shell-quote).
const FIX_PROPOSAL_RESPONSE = JSON.stringify({
  candidates: [
    {
      rationale:
        "Switch to execFileSync with the args array. execFile never spawns a " +
        "shell, so interpolation cannot trigger metacharacter expansion.",
      confidence: 0.95,
      patch: {
        description: "Replace execSync with execFileSync; pass input as argv element",
        fileEdits: [
          {
            file: "src/cmd.ts",
            newContent:
              'import { execFileSync } from "child_process";\n' +
              "export function listFiles(input: string): Buffer {\n" +
              '  return execFileSync("ls", [input]);\n' +
              "}\n",
          },
        ],
      },
    },
  ],
});

const COMPLEMENTARY_RESPONSE = JSON.stringify([]);

const TEST_RESPONSE = JSON.stringify({
  testCode:
    "import { describe, it, expect } from 'vitest';\n" +
    "import { listFiles } from '../cmd.js';\n" +
    "describe('listFiles', () => {\n" +
    "  it('does not execute shell metacharacters', () => {\n" +
    "    expect(() => listFiles(\"; echo HACKED\")).toThrow();\n" +
    "  });\n" +
    "});\n",
  testFilePath: "src/cmd.regression.test.ts",
  testName: "regression: shell-injection via execSync interpolation",
  witnessInputs: {
    argHasInterpolation: true,
    interpSourceIsParam: true,
    calleeIsShellExec: true,
  },
});

// C6: tryExistingCapabilities returns needs_capability.
// Must match the "static-analysis rule author" substring used by buildPrinciplePrompt.
const NEEDS_CAPABILITY_RESPONSE = JSON.stringify({
  kind: "needs_capability",
  missing_predicate:
    "string_composition.has_interpolation: a column that records whether a " +
    "string-producing expression (template literal, string concatenation, or " +
    "string literal) carries any runtime-evaluated subexpressions. Required to " +
    "express 'execSync received an interpolated string', which is the prerequisite " +
    "for shell-injection detection.",
});

// C6 adversarial validation fixtures.
// Must match "security-minded adversary" from buildAdversarialPrompt.
// Note: false_positives must NOT match the principle (no interpolated string
// reaching execSync). false_negatives MUST match.
const ADVERSARIAL_RESPONSE = JSON.stringify({
  false_positives: [
    {
      source:
        'import { execFileSync } from "child_process";\n' +
        "function ok1(input: string) { return execFileSync('ls', [input]); }",
    },
    {
      source:
        'import { execSync } from "child_process";\n' +
        "function ok2() { return execSync('ls /tmp'); }",
    },
    {
      source:
        'import { execSync } from "child_process";\n' +
        "function ok3() { const cmd = 'ls'; return execSync(cmd); }",
    },
  ],
  false_negatives: [
    {
      source:
        'import { execSync } from "child_process";\n' +
        "function bad1(input: string) { return execSync(`ls ${input}`); }",
    },
    {
      source:
        'import { execSync } from "child_process";\n' +
        "function bad2(p: string) { return execSync(`echo ${p}`); }",
    },
    {
      source:
        'import { execSync } from "child_process";\n' +
        "function bad3(file: string) { return execSync(`cat ${file} | head`); }",
    },
  ],
});

// ---------------------------------------------------------------------------
// C6 capability spec proposal — string_composition.
//
// Per the plan and per the empty-catch template, the table name follows the
// node_<capabilityName> convention. The DSL name (without node_ prefix) is
// what the principle references.
//
// Extractor selectivity: we ONLY emit rows for TemplateExpression nodes whose
// has_interpolation is TRUE. This matches what the principle catches and keeps
// negative fixtures (no interpolation, or no template at all) emitting 0 rows.
// Tracking 'concat' and 'literal' kinds would be a substrate broaden-out; this
// dogfood test stays narrow on the bug class.
// ---------------------------------------------------------------------------

const NODE_STRING_COMPOSITION_TABLE_NAME = "node_string_composition";
const CAPABILITY_DSL_NAME = "string_composition";

const SCHEMA_TS =
  "const { sqliteTable, text, integer } = require('drizzle-orm/sqlite-core');\n" +
  `const nodeStringComposition = sqliteTable('${NODE_STRING_COMPOSITION_TABLE_NAME}', {\n` +
  "  nodeId: text('node_id').notNull(),\n" +
  "  kind: text('kind').notNull(),\n" +
  "  hasInterpolation: integer('has_interpolation').notNull(),\n" +
  "});\n" +
  "module.exports = { nodeStringComposition };\n";

// Extractor: ts-morph CJS-compatible. SyntaxKind.TemplateExpression is stable
// across ts-morph versions used here. We emit ONE row per TemplateExpression
// that has at least one template span (interpolation). Plain string literals
// and tagless string-concat are intentionally NOT emitted — keeping the
// negative-fixture row count at 0.
const EXTRACTOR_TS =
  "const { SyntaxKind } = require('ts-morph');\n" +
  "const { sqliteTable, text, integer } = require('drizzle-orm/sqlite-core');\n" +
  `const nodeStringComposition = sqliteTable('${NODE_STRING_COMPOSITION_TABLE_NAME}', {\n` +
  "  nodeId: text('node_id').notNull(),\n" +
  "  kind: text('kind').notNull(),\n" +
  "  hasInterpolation: integer('has_interpolation').notNull(),\n" +
  "});\n" +
  "export function extractNodeStringComposition(tx, sourceFile, nodeIdByNode) {\n" +
  "  sourceFile.forEachDescendant(function(node) {\n" +
  "    if (node.getKind() !== SyntaxKind.TemplateExpression) return;\n" +
  "    var id = nodeIdByNode.get(node);\n" +
  "    if (!id) return;\n" +
  "    var spans = node.getTemplateSpans ? node.getTemplateSpans() : [];\n" +
  "    var hasInterp = spans && spans.length > 0;\n" +
  "    // Only emit rows when there is at least one interpolation span.\n" +
  "    // This keeps negative fixtures (literals, no-interp templates) at 0 rows.\n" +
  "    if (!hasInterp) return;\n" +
  "    tx.insert(nodeStringComposition).values({\n" +
  "      nodeId: id,\n" +
  "      kind: 'template',\n" +
  "      hasInterpolation: 1,\n" +
  "    }).run();\n" +
  "  });\n" +
  "}\n";

const EXTRACTOR_TESTS_TS =
  "import { describe, it, expect } from 'vitest';\n" +
  "describe('extractNodeStringComposition', () => {\n" +
  "  it('placeholder', () => { expect(true).toBe(true); });\n" +
  "});\n";

// DSL: principle that requires the new capability.
//
// Two match clauses:
//   $tpl: a string_composition row with has_interpolation == true (the new cap)
//   $call: a calls row with callee_name == "execSync"
//
// And a same-node constraint: the call's first argument is the interpolated
// template. We approximate "is the argument" by saying the template node IS
// the call's callee_node OR appears in the call's arg position. The simplest
// expressible constraint we have today is to require both matches and let
// adversarial validation pick this up: a file that calls execSync('ls /tmp')
// (no template) yields 0 string_composition rows -> 0 matches. A file that
// has a template but no execSync yields 0 calls rows for that callee -> 0
// matches. Only files with BOTH interpolated template AND execSync match.
//
// This is conservative (it would also flag a file with two unrelated
// statements: an execSync and a separate template) but it's strong enough
// for the adversarial fixtures the test uses, where each fixture is a
// single function with the two together or neither.
//
// data_flow_reaches gives stronger guarantees but the DSL grammar requires
// require-clause whole-node args, and structuring that here is more code
// than it's worth for a stub test. The real-LLM run is where we expect the
// data_flow_reaches story to land.
// Per the parser (src/dsl/parser.ts): only the FIRST match clause uses the
// 'match' keyword; subsequent clauses start with $var directly.
const DSL_SOURCE =
  `principle ShellInjection {\n` +
  `  match $tpl: node where ${CAPABILITY_DSL_NAME}.hasInterpolation == true\n` +
  `  $call: node where calls.callee_name == "execSync"\n` +
  `  report violation {\n` +
  `    at $call\n` +
  `    captures { template: $tpl, call: $call }\n` +
  `    message "execSync invoked with an interpolated string template; user input may reach the shell"\n` +
  `  }\n` +
  `}\n`;

const CAPABILITY_SPEC_RESPONSE = JSON.stringify({
  capabilityName: CAPABILITY_DSL_NAME,
  schemaTs: SCHEMA_TS,
  migrationSql:
    `CREATE TABLE ${NODE_STRING_COMPOSITION_TABLE_NAME} ` +
    `(node_id TEXT NOT NULL, kind TEXT NOT NULL, has_interpolation INTEGER NOT NULL);`,
  extractorTs: EXTRACTOR_TS,
  extractorTestsTs: EXTRACTOR_TESTS_TS,
  registryRegistration:
    `registerCapability({ dslName: '${CAPABILITY_DSL_NAME}', table: nodeStringComposition, ` +
    `columns: { node_id: { dslName: 'node_id', drizzleColumn: nodeStringComposition.nodeId, isNodeRef: true, nullable: false }, ` +
    `kind: { dslName: 'kind', drizzleColumn: nodeStringComposition.kind, isNodeRef: false, nullable: false }, ` +
    `has_interpolation: { dslName: 'has_interpolation', drizzleColumn: nodeStringComposition.hasInterpolation, isNodeRef: false, nullable: false } } });`,
  positiveFixtures: [
    {
      source:
        'import { execSync } from "child_process";\n' +
        "function bad(p: string) { return execSync(`ls ${p}`); }",
      expectedRowCount: 1,
    },
  ],
  negativeFixtures: [
    {
      source:
        'import { execSync } from "child_process";\n' +
        "function ok() { return execSync('ls /tmp'); }",
      expectedRowCount: 0,
    },
  ],
  rationale:
    "Existing capabilities (calls, binding, narrows, ...) cannot express " +
    "interpolation-into-shell-call. string_composition tracks whether a " +
    "string-producing expression carries runtime-evaluated subexpressions. " +
    "Combined with the calls capability, this enables a DSL principle that " +
    "flags execSync(`...${x}...`) patterns.",
  dslSource: DSL_SOURCE,
  name: "shell-injection",
  smtTemplate:
    "(declare-const argHasInterpolation Bool)\n" +
    "(declare-const calleeIsShellExec Bool)\n" +
    "(assert (and calleeIsShellExec argHasInterpolation))\n" +
    "(check-sat)",
  teachingExample: {
    domain: "shell-execution",
    explanation:
      "When a string template carrying user input is passed to execSync, " +
      "the shell parses metacharacters in the input and may execute arbitrary commands.",
    smt2:
      "(declare-const argHasInterpolation Bool)\n" +
      "(declare-const calleeIsShellExec Bool)\n" +
      "(assert (and calleeIsShellExec argHasInterpolation))\n" +
      "(check-sat)",
  },
});

// ---------------------------------------------------------------------------
// Stub LLM builder
// ---------------------------------------------------------------------------

/**
 * Build a StubLLMProvider with all canned responses for the happy-path
 * substrate-extension test. Map insertion order matters (most specific first).
 */
function buildHappyPathLLM(): StubLLMProvider {
  return new StubLLMProvider(
    new Map<string, string>([
      // C6 capability spec (most specific — checked before C3/C4 expert keys)
      ["static-analysis substrate architect", CAPABILITY_SPEC_RESPONSE],
      // C6 adversarial validation
      ["security-minded adversary", ADVERSARIAL_RESPONSE],
      // C6 tryExistingCapabilities
      ["static-analysis rule author", NEEDS_CAPABILITY_RESPONSE],
      // Intake
      ["bug-report parser", INTAKE_RESPONSE],
      // Classify
      ["classifying a bug report", CLASSIFY_RESPONSE],
      // Oracle #1.5 traceability check (must come BEFORE "formal verification
      // expert" and BEFORE "TypeScript testing expert" — its prompt does not
      // contain those, but we keep ordering explicit).
      ["Citations to verify", TRACEABILITY_RESPONSE],
      // Oracle #1.5 adversarial fixture pre-validation. The fixture prompt
      // says "You are a software testing expert" — distinct from C5's
      // "TypeScript testing expert".
      ["software testing expert", FIXTURE_PREVAL_RESPONSE],
      // C1 + cross-LLM agreement (both use "formal verification expert")
      ["formal verification expert", INVARIANT_LLM_RESPONSE],
      // C3
      ["propose up to", FIX_PROPOSAL_RESPONSE],
      // C4
      ["A bug was just fixed at one site", COMPLEMENTARY_RESPONSE],
      // C5
      ["TypeScript testing expert", TEST_RESPONSE],
    ]),
  );
}

// ---------------------------------------------------------------------------
// Scratch project helpers (mirror dogfood.empty-catch.test.ts)
// ---------------------------------------------------------------------------

function setupScratchProject(): {
  scratchDir: string;
  db: Db;
  demoFilePath: string;
} {
  const scratchDir = mkdtempSync(join(tmpdir(), "provekit-shell-injection-"));

  mkdirSync(join(scratchDir, "src"), { recursive: true });
  const demoFilePath = join(scratchDir, "src", "cmd.ts");
  writeFileSync(demoFilePath, DEMO_TS_SOURCE, "utf8");

  // Git init so applyBundle (D2) can run git rev-parse.
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
    // Non-fatal.
  }

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

// Injected C5 test runner — odd call passes (fixed code), even call fails
// (mutation check on original code). Same as empty-catch.
function buildC5TestRunner(): (
  overlay: import("./types.js").OverlayHandle,
  testFilePath: string,
  mainRepoRoot: string,
) => { exitCode: number; stdout: string; stderr: string } {
  let callCount = 0;
  return (_overlay, _testFilePath, _mainRepoRoot) => {
    callCount++;
    if (callCount % 2 === 1) {
      return { exitCode: 0, stdout: "1 test passed", stderr: "" };
    }
    return { exitCode: 1, stdout: "1 test failed (mutation check)", stderr: "" };
  };
}

// Injected D1b vitest runner.
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
// Suite: Dogfood — substrate-extension path via shell-injection gap
// ---------------------------------------------------------------------------

describe(
  "Dogfood: substrate-extension path via shell-injection gap",
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
      "closes the shell-injection gap: LLM proposes string_composition capability -> oracles 14/16/17/18 pass -> substrate bundle assembles",
      { timeout: 120_000 },
      async () => {
        // Build SAST for the fixture file.
        buildSASTForFile(db, demoFilePath);

        const llm = buildHappyPathLLM();

        const signal = await parseBugSignal(
          { text: BUG_REPORT_TEXT, source: "report" },
          llm,
        );
        expect(signal.codeReferences.length).toBeGreaterThan(0);

        // Override codeReferences to use absolute path (locate needs the
        // resolved file path to find the SAST node).
        const absoluteSignal: BugSignal = {
          ...signal,
          codeReferences: [{ file: demoFilePath, line: 3 }],
        };

        const locus = locate(db, absoluteSignal);
        expect(locus).not.toBeNull();
        expect(locus!.file).toContain("cmd.ts");

        const plan: RemediationPlan = await classify(absoluteSignal, locus, llm);
        expect(plan.primaryLayer).toBe("code_invariant");

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

        const bundle = result.bundle;
        if (bundle === null) {
          console.error(
            "Dogfood failure audit trail:",
            JSON.stringify(result.auditTrail, null, 2),
          );
          throw new Error(`Dogfood failed: ${result.reason}`);
        }

        // MUST be a substrate bundle.
        expect(bundle.bundleType).toBe("substrate");

        // The principle candidate carries a capabilitySpec.
        expect(bundle.artifacts.principle).not.toBeNull();
        expect(bundle.artifacts.principle?.kind).toBe("principle_with_capability");

        // Capability spec is structurally complete and refers to the right thing.
        const cap = bundle.artifacts.capabilitySpec;
        expect(cap).not.toBeNull();
        expect(cap!.capabilityName).toMatch(/string.*composition/i);
        expect(cap!.schemaTs).toContain("sqliteTable");
        expect(cap!.extractorTs).toContain("forEachDescendant");
        expect(cap!.migrationSql).toMatch(/CREATE TABLE/i);
        expect(cap!.migrationSql).not.toMatch(/DROP/i);

        // Substrate coherence flags (oracles #14, #16, #17, #18 all passed in C6).
        expect(bundle.coherence.extractorCoverage).toBe(true);
        expect(bundle.coherence.migrationSafe).toBe(true);
        expect(bundle.coherence.substrateConsistency).toBe(true);
        expect(bundle.coherence.principleNeedsCapability).toBe(true);

        // Audit trail: C1 through D1 all completed.
        function entry(stage: string, kind: string) {
          return result.auditTrail.find(
            (e) => e.stage === stage && e.kind === kind,
          );
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
  },
);
