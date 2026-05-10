#!/usr/bin/env tsx
/**
 * #115 step 2: manual-30 gate.
 *
 * Pulls 30 stratified-random rows from `harvest_expressibility` (the v1
 * mechanical tagger output, 403 rows total) and dumps them to a markdown
 * file with checkbox cells for the user to label inline. Stratification
 * by tag bucket prevents the dominant class (252 pending-principle rows)
 * from drowning out the minorities.
 *
 * Determinism: row picks are stable across runs (no time-based RNG).
 * Each row is sorted within its stratum by sha256(project ":" bugId) so
 * re-running the script picks the same 30. If we need a different sample
 * later, change SAMPLE_SALT.
 *
 * Stratum sizes (default: total 30):
 *   - expressible-now-pending-principle:  16
 *   - expressible-now-recognized:         11
 *   - needs-new-relation:                  1 (entire population)
 *   - unknown:                             2
 *
 * The labeling step itself is manual: read each candidate's diff in
 * the markdown and tick `[x]` in the right cell. No automated
 * verification: the whole point of the gate is "does mechanical
 * tagger agree with a human reviewer?"
 */
import { createHash } from "crypto";
import { writeFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import { openDb } from "../src/db/index.js";
import { harvestExpressibility } from "../src/db/schema/harvestExpressibility.js";
import { eq } from "drizzle-orm";

const __dirname = dirname(fileURLToPath(import.meta.url));
const HARVEST_DB = join(__dirname, "..", ".provekit", "harvest", "harvest.db");
const OUT_PATH = join(__dirname, "..", ".provekit", "manual-sample-30.md");
const SAMPLE_SALT = "115-step2-v1";

const STRATA: Array<{ tag: string; n: number }> = [
  { tag: "expressible-now-pending-principle", n: 16 },
  { tag: "expressible-now-recognized", n: 11 },
  { tag: "needs-new-relation", n: 1 },
  { tag: "unknown", n: 2 },
];

const db = openDb(HARVEST_DB);

function deterministicKey(project: string, bugId: string): string {
  return createHash("sha256")
    .update(`${SAMPLE_SALT}:${project}:${bugId}`)
    .digest("hex");
}

function sample(rows: any[], n: number): any[] {
  return [...rows]
    .map((r) => ({ row: r, key: deterministicKey(r.project, r.bugId) }))
    .sort((a, b) => a.key.localeCompare(b.key))
    .slice(0, n)
    .map((x) => x.row);
}

const lines: string[] = [];
lines.push(`# #115 step 2: manual-30 gate`);
lines.push(``);
lines.push(`Mechanical-tagger-v1 says these 30 candidates have these tags. For each row,`);
lines.push(`open the candidate's diff (\`cd /Users/tsavo/bugsjs/<project> && git diff Bug-<id>..Bug-<id>-fix\`)`);
lines.push(`and tick exactly ONE of: agree / disagree / unclear.`);
lines.push(``);
lines.push(`**Tag legend**`);
lines.push(`- expressible-now-recognized → an existing principle in our library matches the bug locus`);
lines.push(`- expressible-now-pending-principle → substrate covers signature; no principle yet`);
lines.push(`- needs-new-relation → multi-node relation absent (chain, alias, composition)`);
lines.push(`- unknown → tagger could not classify mechanically`);
lines.push(``);
lines.push(`**Disagreement counts as miss-tag for the precision number.** The 90% gate`);
lines.push(`(27 agree / 30 total = 90%) is necessary to proceed to step 3.`);
lines.push(``);
lines.push(`Sample salt: \`${SAMPLE_SALT}\` (re-run \`scripts/sample-30.ts\` reproduces this exact list).`);
lines.push(``);

let rowNum = 1;
for (const { tag, n } of STRATA) {
  const allRows = db
    .select()
    .from(harvestExpressibility)
    .where(eq(harvestExpressibility.tag, tag))
    .all();
  const picked = sample(allRows, Math.min(n, allRows.length));

  lines.push(`## ${tag} (${picked.length} sampled / ${allRows.length} total)`);
  lines.push(``);

  for (const r of picked) {
    lines.push(`### ${rowNum}. ${r.project}/Bug-${r.bugId}`);
    lines.push(``);
    lines.push(`**Tagger says:** \`${r.tag}\``);
    lines.push(``);
    lines.push(`**Audit line:** ${r.auditLine}`);
    lines.push(``);
    if (r.tag === "expressible-now-recognized") {
      lines.push(`**Matched principles:** ${r.layer1MatchedPrinciples}`);
    }
    if (r.tag === "expressible-now-pending-principle") {
      lines.push(`**Signature columns:** ${r.signatureColumns}`);
      lines.push(`**Signature kinds:** ${r.signatureKinds}`);
      if (r.signatureRelations !== "[]") {
        lines.push(`**Signature relations:** ${r.signatureRelations}`);
      }
    }
    if (r.missingColumns !== "[]" || r.missingRelations !== "[]") {
      lines.push(`**Missing:** cols=${r.missingColumns} relations=${r.missingRelations}`);
    }
    lines.push(``);
    lines.push(`Diff: \`cd /Users/tsavo/bugsjs/${r.project} && git diff Bug-${r.bugId}..Bug-${r.bugId}-fix\``);
    lines.push(``);
    lines.push(`- [ ] agree (tagger correctly classified)`);
    lines.push(`- [ ] disagree (provide correct tag in note)`);
    lines.push(`- [ ] unclear (mark with ?)`);
    lines.push(``);
    lines.push(`**Note:**`);
    lines.push(``);
    lines.push(`---`);
    lines.push(``);
    rowNum++;
  }
}

writeFileSync(OUT_PATH, lines.join("\n"), "utf-8");
console.log(`Wrote ${rowNum - 1} candidates to ${OUT_PATH}`);
console.log(`Stratification:`);
for (const { tag, n } of STRATA) {
  const total = db
    .select()
    .from(harvestExpressibility)
    .where(eq(harvestExpressibility.tag, tag))
    .all().length;
  console.log(`  ${tag.padEnd(40)} sampled=${n} / total=${total}`);
}
