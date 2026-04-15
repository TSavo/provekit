import { execSync } from "child_process";
import { VerificationResult } from "./verifier";
import { AnalysisResult } from "./types";

export interface IssueData {
  title: string;
  body: string;
}

/**
 * Extract the natural-language claim from SMT-LIB comment lines.
 * Skips PRINCIPLE: tags and trivially short lines.
 */
function extractClaim(smt2: string): string {
  const commentLines = smt2
    .split("\n")
    .filter((l) => l.trim().startsWith(";"))
    .map((l) => l.trim().replace(/^;\s*/, ""));

  return (
    commentLines.find(
      (l) => !l.startsWith("PRINCIPLE:") && l.length > 10
    ) || "(no claim extracted)"
  );
}

/**
 * Build an IssueData from a single sat verification result in context.
 */
function buildIssue(
  v: VerificationResult,
  filePath: string,
  functionName: string,
  line: number,
  logText: string
): IssueData {
  const principle = v.principle || "UNKNOWN";
  const claim = extractClaim(v.smt2);

  const title = `[neurallog] ${principle}: ${claim.slice(0, 80)} — ${filePath}:${line}`;

  const smt2Escaped = v.smt2.replace(/'/g, "'\\''");

  const body = `## Formal Verification Violation

**File:** \`${filePath}\`
**Line:** ${line}
**Function:** \`${functionName}\`
**Log statement:** \`${logText}\`
**Principle:** ${principle}

### Claim
${claim}

### Z3 Proof of Reachability
The following SMT-LIB formula was verified by Z3 to be **satisfiable**, meaning this violation is mathematically reachable:

\`\`\`smt2
${v.smt2}
\`\`\`

### How to verify independently
\`\`\`bash
echo '${smt2Escaped}' | z3 -in
# Expected output: sat
\`\`\`

---
*Filed by [neurallog](https://neurallog.app) — a logger that fixes your code.*`;

  return { title, body };
}

/**
 * Collect all sat violations from analysis results into IssueData[].
 */
export function collectViolationIssues(results: AnalysisResult[]): IssueData[] {
  const issues: IssueData[] = [];

  for (const { derivation, verifications } of results) {
    const satResults = verifications.filter((v) => v.z3Result === "sat");
    for (const v of satResults) {
      issues.push(
        buildIssue(
          v,
          derivation.filePath,
          derivation.callSite.functionName,
          derivation.callSite.line,
          derivation.callSite.logText
        )
      );
    }
  }

  return issues;
}

/**
 * Check if an issue with the given title already exists in the current repo.
 */
function issueExists(title: string): boolean {
  try {
    const output = execSync(
      `gh issue list --state all --search ${JSON.stringify(title)} --json title --limit 50`,
      { encoding: "utf-8", timeout: 15000 }
    );
    const existing: { title: string }[] = JSON.parse(output);
    return existing.some((issue) => issue.title === title);
  } catch {
    return false;
  }
}

/**
 * File a single GitHub issue via `gh` CLI. Returns the issue URL.
 */
function fileIssue(issue: IssueData): string {
  const result = execSync(
    `gh issue create --title ${JSON.stringify(issue.title)} --body ${JSON.stringify(issue.body)} --label neurallog`,
    { encoding: "utf-8", timeout: 30000 }
  ).trim();
  return result;
}

/**
 * File GitHub issues for all sat violations. Deduplicates by title.
 * In dry-run mode, prints issues without filing them.
 */
export function fileViolationIssues(
  issues: IssueData[],
  dryRun: boolean
): { filed: number; skipped: number; errors: number } {
  let filed = 0;
  let skipped = 0;
  let errors = 0;

  const seen = new Set<string>();

  for (const issue of issues) {
    if (seen.has(issue.title)) {
      skipped++;
      continue;
    }
    seen.add(issue.title);

    if (dryRun) {
      console.log();
      console.log("────────────────────────────────────────────");
      console.log(`[DRY RUN] ${issue.title}`);
      console.log("────────────────────────────────────────────");
      console.log(issue.body);
      console.log();
      filed++;
      continue;
    }

    if (issueExists(issue.title)) {
      console.log(`  SKIP (duplicate): ${issue.title.slice(0, 80)}...`);
      skipped++;
      continue;
    }

    try {
      const url = fileIssue(issue);
      console.log(`  FILED: ${url}`);
      filed++;
    } catch (err: any) {
      console.error(`  ERROR filing: ${issue.title.slice(0, 60)}: ${err.message || err}`);
      errors++;
    }
  }

  return { filed, skipped, errors };
}
