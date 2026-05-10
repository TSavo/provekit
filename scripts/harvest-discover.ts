#!/usr/bin/env tsx
/**
 * scripts/harvest-discover.ts: calibration runner for Phase 2-B (discovery).
 *
 * For each candidate that recognition mode does NOT cover, ask the LLM to
 * distill a principle from the diff + commit message. Writes results to
 * .provekit/harvest/staging/ for manual inspection. The full corpus has 165
 * unrecognized candidates after Phase 2-A; this script defaults to --max 1
 * for calibration runs.
 *
 * Usage:
 *   npx tsx scripts/harvest-discover.ts --project express --bug 22
 *   npx tsx scripts/harvest-discover.ts --project express --max 3
 *   npx tsx scripts/harvest-discover.ts --max 5  # first 5 unrecognized across all
 */

import { existsSync, mkdirSync, writeFileSync } from "fs";
import { join } from "path";
import { homedir } from "os";
import { fileURLToPath } from "url";
import { dirname } from "path";
import { extractBugs, type HarvestCandidate } from "../src/fix/harvest/extractBugs.js";
import { recognizeCandidate } from "../src/fix/harvest/recognize.js";
import { discoverPrinciple, type DiscoveryResult } from "../src/fix/harvest/discover.js";
import { appendHarvestProvenance, type HarvestProvenanceEntry } from "../src/fix/harvest/provenance.js";
import { promoteAllStaged } from "../src/fix/harvest/promote.js";
import { createProvider } from "../src/llm/index.js";
import type { LLMProvider } from "../src/fix/types.js";

function parseFlag(args: string[], name: string): string | undefined {
  const idx = args.indexOf(name);
  if (idx === -1) return undefined;
  return args[idx + 1];
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const bugsDir = process.env["BUGSJS_DIR"] ?? join(homedir(), "bugsjs");
  if (!existsSync(bugsDir)) {
    process.stderr.write(`No BugsJS directory at ${bugsDir}.\n`);
    process.exit(1);
  }

  const projectFlag = parseFlag(args, "--project");
  const bugFlag = parseFlag(args, "--bug");
  const maxFlag = parseFlag(args, "--max");
  const max = maxFlag !== undefined ? parseInt(maxFlag, 10) : 1;
  // --persist-provenance: append harvest provenance to recognized candidates'
  // principle JSONs at end of run. Off by default (calibration runs shouldn't
  // mutate the source-controlled library).
  const persistProvenance = args.includes("--persist-provenance");
  // --promote: after discovery, run promotion pass over staging. Off by
  // default for the same reason.
  const promote = args.includes("--promote");

  // Default staging dir lives in the project root.
  const __filename = fileURLToPath(import.meta.url);
  const __dirname = dirname(__filename);
  const projectRoot = join(__dirname, "..");
  const stagingDir = join(projectRoot, ".provekit", "harvest", "staging");
  const principlesDir = join(projectRoot, ".provekit", "principles");
  mkdirSync(stagingDir, { recursive: true });

  // Build provider + bridge to the fix-layer LLMProvider shape (see
  // cli.fix.ts:632 for the original: same pattern).
  const realProvider = createProvider();
  const llm: LLMProvider = {
    complete: async (params: { prompt: string; model?: "haiku" | "sonnet" | "opus" }) => {
      const resp = await realProvider.complete(params.prompt, {
        model: params.model ?? "opus",
        systemPrompt: "",
      });
      return resp.text;
    },
    ...(realProvider.agent
      ? { agent: (prompt: string, options) => realProvider.agent!(prompt, { ...options, model: options.model ?? "opus" }) }
      : {}),
  };

  // Collect candidates per project.
  let toExamine: { candidate: HarvestCandidate; project: string }[] = [];
  if (projectFlag && bugFlag) {
    const projectPath = join(bugsDir, projectFlag);
    const ex = extractBugs({ projectPath, project: projectFlag, onlyBugIds: [bugFlag] });
    if (ex.candidates.length === 0) {
      process.stderr.write(`No candidate for ${projectFlag} bug ${bugFlag}.\n`);
      process.exit(1);
    }
    toExamine = ex.candidates.map((c) => ({ candidate: c, project: projectFlag }));
  } else {
    const projectsRoot = projectFlag ? [projectFlag] : ["express", "eslint", "karma", "hexo", "hessian.js", "pencilblue", "shields", "bower"]
      .filter((p) => existsSync(join(bugsDir, p, ".git")));
    for (const project of projectsRoot) {
      const projectPath = join(bugsDir, project);
      const ex = extractBugs({ projectPath, project });
      for (const candidate of ex.candidates) {
        toExamine.push({ candidate, project });
      }
    }
  }

  console.log(`# Harvest discovery calibration`);
  console.log(`stagingDir=${stagingDir}`);
  console.log(`max=${max} (number of unrecognized candidates to discover on)`);
  console.log();

  let processed = 0;
  let skippedRecognized = 0;
  // All candidates we examined, indexed by "<project>-<bugId>": used by the
  // promotion pass at end-of-run as the negative cohort source.
  const candidatesById = new Map<string, HarvestCandidate>();
  // Recognized provenance entries to write at end (if --persist-provenance).
  const recognizedProvenance: HarvestProvenanceEntry[] = [];

  for (const { candidate, project } of toExamine) {
    candidatesById.set(`${project}-${candidate.source.bugId}`, candidate);
    if (processed >= max) break;

    // Recognition gate: skip candidates already covered by the library.
    const rec = recognizeCandidate(candidate, { principlesDir });
    if (rec.recognized) {
      skippedRecognized++;
      // Track provenance: each principle that fired earns one entry. Dedup
      // by principle name within a single candidate (the recognizer may
      // report the same principle on multiple lines).
      const seen = new Set<string>();
      for (const m of rec.matches) {
        if (seen.has(m.principleName)) continue;
        seen.add(m.principleName);
        recognizedProvenance.push({
          principleId: m.principleName,
          projectId: project,
          bugId: candidate.source.bugId,
        });
      }
      continue;
    }

    const label = `${project}-bug-${candidate.source.bugId}`;
    console.log(`## Discovering: ${label}`);
    console.log(`  upstream message: ${truncate(candidate.upstreamFixMessage.split("\n")[0] ?? "", 100)}`);
    console.log(`  diff stats: ${candidate.stats.filesChanged} files, +${candidate.stats.insertions}/-${candidate.stats.deletions}`);

    const t0 = Date.now();
    let result: DiscoveryResult;
    try {
      result = await discoverPrinciple({ candidate, llm });
    } catch (err) {
      console.log(`  ERROR: ${err instanceof Error ? err.message : String(err)}`);
      processed++;
      continue;
    }
    const elapsedMs = Date.now() - t0;

    console.log(`  outcome: ${result.outcome.kind} (${(elapsedMs / 1000).toFixed(1)}s)`);
    if (result.outcome.kind === "ok") {
      console.log(`  produced ${result.outcome.principleCount} principle(s):`);
      for (const p of result.principles) {
        console.log(`    - ${p.name} (bugClassId=${p.bugClassId})`);
      }
    } else if ("reason" in result.outcome) {
      console.log(`  reason: ${truncate(result.outcome.reason, 200)}`);
    } else if ("rejectedShapes" in result.outcome) {
      console.log(`  rejected: ${result.outcome.rejectedShapes.map((s) => s.name).join(", ")}`);
    } else if ("gap" in result.outcome) {
      console.log(`  gap: ${truncate(result.outcome.gap, 200)}`);
    }

    // Persist the full result for inspection.
    const stagedPath = join(stagingDir, `${label}.json`);
    const stagedRecord = {
      candidate: {
        source: candidate.source,
        upstreamFixMessage: candidate.upstreamFixMessage,
        stats: candidate.stats,
        diff: candidate.diff,
      },
      synthesizedInvariant: result.synthesizedInvariant,
      invariantRaw: result.invariantRaw,
      outcome: result.outcome,
      principles: result.principles.map((p) => ({
        kind: p.kind,
        name: p.name,
        bugClassId: p.bugClassId,
        dslSource: "dslSource" in p ? p.dslSource : undefined,
      })),
      elapsedMs,
    };
    writeFileSync(stagedPath, JSON.stringify(stagedRecord, null, 2), "utf-8");
    console.log(`  staged: ${stagedPath}`);
    console.log();

    processed++;
  }

  console.log(`# Summary`);
  console.log(`processed: ${processed}`);
  console.log(`skipped (already recognized): ${skippedRecognized}`);
  console.log(`staging dir: ${stagingDir}`);

  // Optional provenance writeback for recognized candidates.
  if (persistProvenance && recognizedProvenance.length > 0) {
    const r = appendHarvestProvenance(recognizedProvenance, principlesDir);
    console.log();
    console.log(`# Provenance writeback`);
    console.log(`appended: ${r.appended}`);
    console.log(`duplicates (idempotent): ${r.duplicates}`);
    if (r.missingPrinciples.length > 0) {
      console.log(`missing principle ids (skipped): ${r.missingPrinciples.length}`);
    }
  }

  // Optional promotion pass over staging.
  if (promote) {
    console.log();
    console.log(`# Promotion pass`);
    const r = promoteAllStaged({ stagingDir, candidatesById, principlesDir });
    console.log(`records examined: ${r.totalRecords}`);
    console.log(`promoted: ${r.totalPromoted}`);
    console.log(`quarantined: ${r.totalQuarantined}`);
  }
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + "...";
}

main().catch((err) => {
  process.stderr.write(`fatal: ${err instanceof Error ? err.stack ?? err.message : String(err)}\n`);
  process.exit(1);
});
