/**
 * SemGrep community-rules → CorpusScenario importer.
 *
 * Converts a local clone of github.com/semgrep/semgrep-rules into a set of
 * CorpusScenario files under src/fix/corpus/scenarios/imported/ so the corpus
 * sweep can run them with no manual authoring.
 *
 * Scope (intentionally narrow):
 *   - Languages: typescript, javascript (sibling .ts or .js test fixture).
 *   - Each rule must have at least one `// ruleid: <id>` annotation in its
 *     test file (positive example present).
 *   - Bug-class mapping:
 *       * "division-by-zero" if rule id mentions divide / division
 *       * "null-assertion"   if rule id mentions null / undefined / non-null
 *       * "ternary-branch-collapse" if rule id mentions ternary / branch
 *       * "novel" otherwise → expected.outcome = "out_of_scope"
 *
 * Networking is intentionally out of scope: the user clones semgrep-rules
 * locally and passes its path. If absent, the CLI prints instructions:
 *   git clone --depth 1 https://github.com/semgrep/semgrep-rules ~/semgrep-rules
 *
 * Once a clone exists at ~/semgrep-rules (or a sibling), run:
 *   npx tsx scripts/import-semgrep.ts ~/semgrep-rules
 */

import { readdirSync, readFileSync, statSync, existsSync, mkdirSync, writeFileSync } from "fs";
import { join, basename, extname, dirname } from "path";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export interface ImportOptions {
  /** Path to a local clone of github.com/semgrep/semgrep-rules. */
  ruleDir: string;
  /** Where generated scenario .ts files should be written. */
  outDir: string;
  /** Optional cap so dev runs do not produce thousands of scenarios. */
  maxScenarios?: number;
}

export interface ImportSummary {
  written: number;
  skipped: number;
  reasons: Record<string, number>;
}

/**
 * Walk `ruleDir`, locate all SemGrep YAML rules with TS/JS test fixtures, and
 * write a scenario file per rule into `outDir`.
 */
export function importSemgrepCorpus(opts: ImportOptions): ImportSummary {
  const reasons: Record<string, number> = {};
  let written = 0;
  let skipped = 0;
  const max = opts.maxScenarios ?? Infinity;

  if (!existsSync(opts.ruleDir)) {
    return {
      written: 0,
      skipped: 0,
      reasons: { "ruleDir not found": 1 },
    };
  }

  mkdirSync(opts.outDir, { recursive: true });

  const yamls = collectYamlFiles(opts.ruleDir);
  for (const yamlPath of yamls) {
    if (written >= max) break;
    try {
      const result = processYamlRule(yamlPath, opts.outDir);
      if (result.kind === "wrote") {
        written += 1;
      } else {
        skipped += 1;
        reasons[result.reason] = (reasons[result.reason] ?? 0) + 1;
      }
    } catch (err) {
      skipped += 1;
      const msg = err instanceof Error ? err.message : String(err);
      reasons[`exception: ${msg.slice(0, 60)}`] = (reasons[`exception: ${msg.slice(0, 60)}`] ?? 0) + 1;
    }
  }

  return { written, skipped, reasons };
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

const TS_JS_TEST_EXTS = [".ts", ".js", ".tsx", ".jsx"];

interface SemgrepRuleMeta {
  id: string;
  message: string;
  severity: string;
  languages: string[];
}

type ProcessResult =
  | { kind: "wrote"; scenarioId: string }
  | { kind: "skipped"; reason: string };

function collectYamlFiles(rootDir: string): string[] {
  const out: string[] = [];
  walk(rootDir, (entry) => {
    if (entry.endsWith(".yaml") || entry.endsWith(".yml")) {
      // Only TS/JS rule subdirectories.
      const lower = entry.toLowerCase();
      if (lower.includes("/typescript/") || lower.includes("/javascript/")) {
        out.push(entry);
      }
    }
  });
  return out;
}

function walk(dir: string, onFile: (path: string) => void): void {
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    if (name.startsWith(".")) continue;
    if (name === "node_modules") continue;
    const full = join(dir, name);
    let st;
    try {
      st = statSync(full);
    } catch {
      continue;
    }
    if (st.isDirectory()) {
      walk(full, onFile);
    } else if (st.isFile()) {
      onFile(full);
    }
  }
}

/**
 * Pull the first rule's id, message, severity, and languages out of a SemGrep
 * YAML file using line-oriented parsing. SemGrep rules have a stable shape:
 *
 *   rules:
 *     - id: my-rule-id
 *       message: |
 *         Something is wrong here.
 *       severity: WARNING
 *       languages: [typescript, javascript]
 *
 * We avoid pulling in a real YAML parser (no js-yaml in deps) and only need
 * the four fields above. If the file shape diverges, return null.
 */
function parseRuleMeta(yamlText: string): SemgrepRuleMeta | null {
  const lines = yamlText.split(/\r?\n/);
  let id: string | null = null;
  let message: string | null = null;
  let severity: string | null = null;
  let languages: string[] | null = null;

  // Walk lines, find first occurrence of each top-level key.
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const trimmed = line.trim();
    if (!id) {
      const m = trimmed.match(/^-?\s*id:\s*['"]?([^'"#\n]+?)['"]?\s*$/);
      if (m && m[1]) id = m[1].trim();
    }
    if (!message) {
      const m = trimmed.match(/^message:\s*(.*)$/);
      if (m) {
        const v = m[1]!.trim();
        if (v === "|" || v === ">" || v === "|-" || v === ">-") {
          // Block-scalar: take subsequent indented lines.
          const collected: string[] = [];
          for (let j = i + 1; j < lines.length; j++) {
            const sub = lines[j]!;
            if (sub.trim() === "") {
              collected.push("");
              continue;
            }
            // Keep going while the line is more indented than `message:`.
            const subIndent = sub.length - sub.trimStart().length;
            if (subIndent === 0) break;
            collected.push(sub.trim());
          }
          message = collected.join(" ").replace(/\s+/g, " ").trim();
        } else {
          message = v.replace(/^['"]/, "").replace(/['"]$/, "");
        }
      }
    }
    if (!severity) {
      const m = trimmed.match(/^severity:\s*([A-Za-z]+)\s*$/);
      if (m && m[1]) severity = m[1];
    }
    if (!languages) {
      const m = trimmed.match(/^languages:\s*\[(.+)\]\s*$/);
      if (m && m[1]) {
        languages = m[1]
          .split(",")
          .map((s) => s.trim().replace(/^['"]/, "").replace(/['"]$/, ""))
          .filter(Boolean);
      } else if (trimmed === "languages:") {
        // Block list form.
        const collected: string[] = [];
        for (let j = i + 1; j < lines.length; j++) {
          const sub = lines[j]!.trim();
          const lm = sub.match(/^-\s*['"]?([^'"#\n]+?)['"]?\s*$/);
          if (!lm) break;
          collected.push(lm[1]!);
        }
        languages = collected;
      }
    }
  }

  if (!id || !message || !languages || languages.length === 0) return null;
  return {
    id,
    message: message.slice(0, 400),
    severity: severity ?? "WARNING",
    languages,
  };
}

function findFixtureFile(yamlPath: string, ruleId: string): string | null {
  const dir = dirname(yamlPath);
  const base = basename(yamlPath, extname(yamlPath));
  // Try sibling files: <base>.ts, <base>.js, <base>.tsx, <base>.jsx, <ruleId>.ts ...
  const candidates: string[] = [];
  for (const ext of TS_JS_TEST_EXTS) {
    candidates.push(join(dir, `${base}${ext}`));
    candidates.push(join(dir, `${ruleId}${ext}`));
  }
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  return null;
}

function inferBugClass(ruleId: string): "division-by-zero" | "null-assertion" | "ternary-branch-collapse" | "novel" {
  const lower = ruleId.toLowerCase();
  if (lower.includes("divid")) return "division-by-zero";
  if (lower.includes("null") || lower.includes("undefined") || lower.includes("non-null")) return "null-assertion";
  if (lower.includes("ternary") || lower.includes("branch")) return "ternary-branch-collapse";
  return "novel";
}

function buildScenarioSource(args: {
  scenarioId: string;
  bugClass: ReturnType<typeof inferBugClass>;
  fileRelPath: string;
  fileContent: string;
  bugReport: string;
}): string {
  const { scenarioId, bugClass, fileRelPath, fileContent, bugReport } = args;
  const isOutOfScope = bugClass === "novel";
  const fileLiteral = JSON.stringify(fileContent);
  const reportLiteral = JSON.stringify(bugReport);
  const idLiteral = JSON.stringify(scenarioId);
  const pathLiteral = JSON.stringify(fileRelPath);

  if (isOutOfScope) {
    // Drive only B1 (intake) + B2 (locate would fail) — D1 will reject naturally.
    // expected.outcome = "out_of_scope" via classify() decision.
    return `/**
 * IMPORTED FROM semgrep-rules.
 * Auto-generated by src/fix/corpus/import-semgrep.ts.
 * Do not edit by hand: re-run the importer.
 */
import type { CorpusScenario } from "../../scenarios.js";

export const scenario: CorpusScenario = {
  id: ${idLiteral},
  bugClass: "novel",
  files: {
    [${pathLiteral}]: ${fileLiteral},
  },
  bugReport: ${reportLiteral},
  expected: {
    completes: ["intake"],
    outcome: "out_of_scope",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: ${JSON.stringify(bugReport.slice(0, 120))},
        failureDescription: ${JSON.stringify(bugReport.slice(0, 200))},
        fixHint: "imported semgrep rule — out of scope for current principle library",
        codeReferences: [{ file: ${pathLiteral}, line: 1 }],
        bugClassHint: "novel",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "out_of_scope",
        secondaryLayers: [],
        artifacts: [],
        rationale: "Imported semgrep rule with no matching principle.",
      }),
    },
  ],
};
`;
  }

  // Principled bug — generate scenario with fixture stub matching the bug class.
  // The patch is intentionally a no-op so the scenario verifies that the
  // pipeline reaches at least C1 + C2; downstream C3 oracle #2 may fail without
  // a real fix, but that is acceptable for an imported placeholder.
  const fixtureImport =
    bugClass === "division-by-zero"
      ? 'import { intZeroFixtureStub } from "../../commonStubs.js";'
      : bugClass === "null-assertion"
      ? 'import { boolFixtureStub } from "../../commonStubs.js";'
      : 'import { intEqualityFixtureStub } from "../../commonStubs.js";';

  const fixtureCall =
    bugClass === "division-by-zero"
      ? 'intZeroFixtureStub("b")'
      : bugClass === "null-assertion"
      ? 'boolFixtureStub("isNull", true)'
      : 'intEqualityFixtureStub("a", "b")';

  const invariantBlock =
    bugClass === "division-by-zero"
      ? `{
        description: "imported: divisor may be zero at the offending site",
        smt_declarations: ["(declare-const b Int)"],
        smt_violation_assertion: "(assert (= b 0))",
        bindings: [{ smt_constant: "b", source_expr: "b", sort: "Int" }],
        citations: [{ smt_clause: "(= b 0)", source_quote: ${JSON.stringify(bugReport.slice(0, 120))} }],
      }`
      : bugClass === "null-assertion"
      ? `{
        description: "imported: value may be null at dereference",
        smt_declarations: ["(declare-const isNull Bool)"],
        smt_violation_assertion: "(assert (= isNull true))",
        bindings: [{ smt_constant: "isNull", source_expr: "x === null", sort: "Bool" }],
        citations: [{ smt_clause: "(= isNull true)", source_quote: ${JSON.stringify(bugReport.slice(0, 120))} }],
      }`
      : `{
        description: "imported: ternary branches collapse to same value",
        smt_declarations: ["(declare-const a Int)", "(declare-const b Int)"],
        smt_violation_assertion: "(assert (= a b))",
        bindings: [
          { smt_constant: "a", source_expr: "? branch", sort: "Int" },
          { smt_constant: "b", source_expr: ": branch", sort: "Int" },
        ],
        citations: [{ smt_clause: "(= a b)", source_quote: ${JSON.stringify(bugReport.slice(0, 120))} }],
      }`;

  return `/**
 * IMPORTED FROM semgrep-rules.
 * Auto-generated by src/fix/corpus/import-semgrep.ts.
 * Do not edit by hand: re-run the importer.
 */
import type { CorpusScenario } from "../../scenarios.js";
${fixtureImport}

export const scenario: CorpusScenario = {
  id: ${idLiteral},
  bugClass: ${JSON.stringify(bugClass)},
  files: {
    [${pathLiteral}]: ${fileLiteral},
  },
  bugReport: ${reportLiteral},
  expected: {
    // Imported rules don't ship a known-good patch, so we expect the pipeline
    // to advance through invariant formulation but reject at C3 (no satisfying
    // fix). This is recorded as expected_failure, not integration_gap.
    completes: ["intake", "locate", "classify", "C1", "C2"],
    fails: { stage: "C3", reason: "imported rule: no canned fix candidate" },
    outcome: "rejected",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: ${JSON.stringify(bugReport.slice(0, 120))},
        failureDescription: ${JSON.stringify(bugReport.slice(0, 200))},
        fixHint: "see semgrep rule for guidance",
        codeReferences: [{ file: ${pathLiteral}, line: 1 }],
        bugClassHint: ${JSON.stringify(bugClass)},
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [{ kind: "code_patch", rationale: "imported semgrep rule" }],
        rationale: "Imported semgrep rule mapped to known bug class.",
      }),
    },
    {
      matchPrompt: "formal verification expert",
      response: JSON.stringify(${invariantBlock}),
    },
    ${fixtureCall},
    {
      matchPrompt: "propose up to",
      response: JSON.stringify({ candidates: [] }),
    },
  ],
};
`;
}

/**
 * Sanitize the SemGrep rule id into a valid TS file basename and CorpusScenario id.
 */
function sanitizeId(ruleId: string): string {
  return ruleId.replace(/[^a-zA-Z0-9._-]+/g, "-").slice(0, 80);
}

function processYamlRule(yamlPath: string, outDir: string): ProcessResult {
  const text = readFileSync(yamlPath, "utf-8");
  const meta = parseRuleMeta(text);
  if (!meta) return { kind: "skipped", reason: "could not parse rule metadata" };

  const langOk = meta.languages.some((l) => /^(typescript|javascript|ts|js|tsx|jsx)$/i.test(l));
  if (!langOk) return { kind: "skipped", reason: "language not TS/JS" };

  const fixturePath = findFixtureFile(yamlPath, meta.id);
  if (!fixturePath) return { kind: "skipped", reason: "no sibling test fixture" };

  const fixtureContent = readFileSync(fixturePath, "utf-8");
  if (!/\bruleid\s*:\s*\S/.test(fixtureContent)) {
    return { kind: "skipped", reason: "no ruleid annotation in fixture" };
  }

  const bugClass = inferBugClass(meta.id);
  const scenarioId = `imported-${sanitizeId(meta.id)}`;
  const fileRelPath = `src/${basename(fixturePath)}`;

  const source = buildScenarioSource({
    scenarioId,
    bugClass,
    fileRelPath,
    fileContent: fixtureContent,
    bugReport:
      `Imported from semgrep rule '${meta.id}': ${meta.message} ` +
      `(${meta.severity}). Found at ${fileRelPath} line 1.`,
  });

  const outPath = join(outDir, `${scenarioId}.ts`);
  writeFileSync(outPath, source, "utf-8");
  return { kind: "wrote", scenarioId };
}
