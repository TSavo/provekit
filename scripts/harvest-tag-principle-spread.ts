#!/usr/bin/env tsx
/**
 * scripts/harvest-tag-principle-spread.ts — for each principle that fires
 * in harvest_expressibility, sample N candidates and print the upstream
 * commit subjects so we can eyeball topic-spread.
 *
 * Per the tightening plan: principles whose hits span wildly different
 * commit topics are over-broad and earn the first tightening pass.
 *
 * Usage:
 *   npx tsx scripts/harvest-tag-principle-spread.ts
 *   npx tsx scripts/harvest-tag-principle-spread.ts --principle falsy-default --n 30
 */

import { existsSync } from "fs";
import { join, dirname } from "path";
import { homedir } from "os";
import { fileURLToPath } from "url";
import { extractBugs } from "../src/fix/harvest/extractBugs.js";
import { openDb } from "../src/db/index.js";

function parseFlag(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return undefined;
  return args[idx + 1];
}

interface ClusterRow {
  principle: string;
  project: string;
  bug_id: string;
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const principleFilter = parseFlag(args, "--principle");
  const nFlag = parseFlag(args, "--n");
  const perPrinciple = nFlag !== undefined ? parseInt(nFlag, 10) : 12;

  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");
  const __filename = fileURLToPath(import.meta.url);
  const __dirname = dirname(__filename);
  const projectRoot = join(__dirname, "..");
  const dbPath = join(projectRoot, ".provekit", "harvest", "harvest.db");
  if (!existsSync(dbPath)) {
    process.stderr.write(`No harvest.db at ${dbPath}.\n`);
    process.exit(1);
  }
  const db = openDb(dbPath);

  const rowsRaw = db.$client
    .prepare(
      `WITH recognized AS (
         SELECT project, bug_id, layer1_matched_principles AS principles
         FROM harvest_expressibility WHERE tag = 'expressible-now-recognized'
       )
       SELECT recognized.project AS project, recognized.bug_id AS bug_id, json_each.value AS principle
       FROM recognized, json_each(recognized.principles)
       ${principleFilter ? "WHERE json_each.value = ?" : ""}`,
    )
    .all(...(principleFilter ? [principleFilter] : [])) as ClusterRow[];

  const byPrinciple = new Map<string, ClusterRow[]>();
  for (const r of rowsRaw) {
    const arr = byPrinciple.get(r.principle) ?? [];
    arr.push(r);
    byPrinciple.set(r.principle, arr);
  }

  // Pre-load extractBugs to look up commit messages.
  const projects = Array.from(new Set(rowsRaw.map((r) => r.project)));
  const subjectByKey = new Map<string, string>();
  const filePathByKey = new Map<string, string>();
  for (const project of projects) {
    const projectPath = join(bugsDir, project);
    if (!existsSync(join(projectPath, ".git"))) continue;
    const ex = extractBugs({ projectPath, project });
    for (const c of ex.candidates) {
      const key = `${project}-${c.source.bugId}`;
      subjectByKey.set(key, c.upstreamFixMessage.split("\n")[0] ?? "");
      const firstFile = (() => {
        const m = /^diff --git a\/([^\s]+)/m.exec(c.diff);
        return m ? m[1] : "";
      })();
      filePathByKey.set(key, firstFile);
    }
  }

  const principles = Array.from(byPrinciple.keys()).sort(
    (a, b) => (byPrinciple.get(b)?.length ?? 0) - (byPrinciple.get(a)?.length ?? 0),
  );

  for (const principle of principles) {
    const hits = byPrinciple.get(principle) ?? [];
    console.log(`## ${principle} (n=${hits.length})`);
    // Stable order: stratify by project so the sample isn't all-eslint.
    const byProj = new Map<string, ClusterRow[]>();
    for (const h of hits) {
      const arr = byProj.get(h.project) ?? [];
      arr.push(h);
      byProj.set(h.project, arr);
    }
    const sample: ClusterRow[] = [];
    const projOrder = Array.from(byProj.keys()).sort();
    let i = 0;
    while (sample.length < perPrinciple) {
      let added = false;
      for (const proj of projOrder) {
        const arr = byProj.get(proj) ?? [];
        if (i < arr.length) {
          sample.push(arr[i]!);
          added = true;
          if (sample.length >= perPrinciple) break;
        }
      }
      if (!added) break;
      i++;
    }
    for (const h of sample) {
      const key = `${h.project}-${h.bug_id}`;
      const subject = subjectByKey.get(key) ?? "<missing>";
      const file = filePathByKey.get(key) ?? "<no file>";
      console.log(`  [${h.project}-${h.bug_id}] ${file}`);
      console.log(`    ${truncate(subject, 110)}`);
    }
    console.log();
  }

  db.$client.close();
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + "...";
}

main().catch((err) => {
  process.stderr.write(`fatal: ${err instanceof Error ? err.stack ?? err.message : String(err)}\n`);
  process.exit(1);
});
