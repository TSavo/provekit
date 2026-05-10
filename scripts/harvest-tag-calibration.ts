#!/usr/bin/env tsx
/**
 * scripts/harvest-tag-calibration.ts: calibrate the v1 mechanical tagger
 * against in-repo hard-bug fixtures with known-correct expressibility tags.
 *
 * Tagger v1 needs a calibration set before BugsJS results are trustworthy.
 * The dogfood fixtures (empty-catch, shell-injection) are ground-truth
 * cases the architecture identified as substrate-extension territory:
 *
 *   - empty-catch     → needs-capability-extension (try/catch with empty
 *                       handler body has no kind in current `throws` /
 *                       `narrows` enums; substrate gap diagnosed in
 *                       docs/plans/2026-04-23-fix-loop/capability-gaps.md)
 *   - shell-injection → needs-new-relation (string_composition + data
 *                       flow reaches; per Leak 2 / pitch-leaks doc)
 *
 * If v1 tags these correctly, the heuristic can see substrate-gap shapes
 * when they exist. If it tags them expressible-now, the heuristic is too
 * loose. If it returns unknown, the heuristic doesn't see the shape.
 *
 * Usage:
 *   npx tsx scripts/harvest-tag-calibration.ts
 */

import { tagExpressibility } from "../src/fix/harvest/expressibility.js";
import type { HarvestCandidate } from "../src/fix/harvest/extractBugs.js";

interface CalibrationCase {
  name: string;
  expected: string;
  candidate: HarvestCandidate;
  rationale: string;
}

function makeCandidate(
  buggyFiles: Record<string, string>,
  diffPath: string,
  hunkStart: number,
  hunkLen: number,
): HarvestCandidate {
  const diff = `diff --git a/${diffPath} b/${diffPath}\n--- a/${diffPath}\n+++ b/${diffPath}\n@@ -${hunkStart},${hunkLen} +${hunkStart},${hunkLen} @@\n`;
  return {
    source: {
      project: "dogfood",
      bugId: "calibration",
      baseSha: "0".repeat(40),
      fixSha: "1".repeat(40),
      testSha: null,
      originalSha: null,
    },
    buggyFiles,
    fixedFiles: {},
    diff,
    upstreamFixMessage: "",
    testFiles: {},
    stats: { filesChanged: 1, insertions: 0, deletions: 0 },
  };
}

const EMPTY_CATCH_SOURCE = `export function loadConfig(path: string): string {
  try {
    return require("fs").readFileSync(path, "utf8");
  } catch (e) {
    // empty catch: silently swallows exception
  }
  return "";
}
`;

const SHELL_INJECTION_SOURCE = `import { execSync } from "child_process";
export function listFiles(input: string): Buffer {
  return execSync(\`ls \${input}\`);
}
`;

const cases: CalibrationCase[] = [
  {
    name: "empty-catch",
    expected: "needs-capability-extension",
    candidate: makeCandidate(
      { "src/demo.ts": EMPTY_CATCH_SOURCE },
      "src/demo.ts",
      2,
      5, // try/catch lines 2-6
    ),
    rationale:
      "the try/catch with empty handler is the bug: substrate has `throws` capability but no `try_catch_handler_empty` kind. Tagger should report missing-column or new-capability if it sees the shape.",
  },
  {
    name: "shell-injection",
    expected: "needs-new-relation",
    candidate: makeCandidate(
      { "src/cmd.ts": SHELL_INJECTION_SOURCE },
      "src/cmd.ts",
      2,
      3, // execSync call lines 2-4
    ),
    rationale:
      "the bug shape requires reasoning over a tainted-string composition chain (parameter → template literal → execSync arg). data_flow_transitive exists but does not carry taint. Real gap is string_composition + data_flow_reaches relation.",
  },
];

function fmt(actual: string, expected: string): string {
  if (actual === expected) return `OK match`;
  // soft pass: any needs-* bucket is in the right family for these calibration cases
  if (actual.startsWith("needs-") && expected.startsWith("needs-")) return `SOFT (family match)`;
  return `MISS`;
}

async function main(): Promise<void> {
  console.log(`# Tagger v1 calibration on in-repo hard-bug fixtures`);
  console.log();
  let okCount = 0;
  let softCount = 0;
  let missCount = 0;

  for (const c of cases) {
    const tag = tagExpressibility({ candidate: c.candidate });
    const verdict = fmt(tag.tag, c.expected);
    if (verdict === "OK match") okCount++;
    else if (verdict.startsWith("SOFT")) softCount++;
    else missCount++;

    console.log(`## ${c.name}`);
    console.log(`expected: ${c.expected}`);
    console.log(`actual:   ${tag.tag}`);
    console.log(`verdict:  ${verdict}`);
    console.log(`rationale: ${c.rationale}`);
    console.log(`audit:    ${tag.auditLine}`);
    console.log();
  }

  console.log(`# Summary`);
  console.log(`OK: ${okCount}/${cases.length}`);
  console.log(`SOFT (family match): ${softCount}/${cases.length}`);
  console.log(`MISS: ${missCount}/${cases.length}`);
  console.log();
  if (missCount > 0) {
    console.log(`Calibration failed. v1 heuristic is not seeing substrate-gap shapes when they exist.`);
    console.log(`Iterate before trusting full-corpus results.`);
    process.exit(1);
  }
}

main().catch((err) => {
  process.stderr.write(`fatal: ${err instanceof Error ? err.stack ?? err.message : String(err)}\n`);
  process.exit(1);
});
