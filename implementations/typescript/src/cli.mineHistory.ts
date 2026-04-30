/**
 * `provekit mine-history` — step 11 of the standing-invariant-runtime spec.
 *
 * Walks the existing git log of a project and runs B0 retrospective intake
 * (extractIntent) against each commit. Translates each constraint-shaped
 * intent into the InvariantClaim+BugLocus+BugSignal shape that
 * buildStoredInvariant expects, and persists the resulting StoredInvariant
 * to .provekit/invariants/<id>.json.
 *
 * Bootstrap-from-history product surface: a five-year-old codebase ends up
 * with a populated constraint corpus on day one without anyone having to
 * file a single problem statement. The retrospective direction reuses every
 * downstream gate the prospective direction uses, so the corpus that lands
 * on disk has been through the same correctness story (just deferred until
 * `provekit verify` re-resolves bindings and re-checks paths).
 *
 * Reference: protocol/specs/2026-04-27-standing-invariant-runtime.md, §
 * "Implementation order" item 11.
 *
 * v1 scope:
 *   - oldest-to-newest walk by default (most additive corpus; same
 *     constraint twice collapses via content-addressable hashing).
 *   - --since accepts both commit-sha (uses `<sha>..HEAD`) and date string
 *     (uses `--since=<date>`); detected via shape.
 *   - --max-commits caps the walk (default 100) so a green-field user
 *     can't accidentally burn $50 on a 5-year repo's first run.
 *   - --dry-run walks + extracts but does not persist; prints would-mint
 *     counts so the user can size up the cost before committing.
 *   - per-commit error tolerance: a single bad commit (git-show failure,
 *     LLM schema malformation, etc.) logs and continues; the walk only
 *     dies on unrecoverable setup errors.
 */

import { execFileSync } from "child_process";
import { existsSync } from "fs";
import { join, dirname, resolve } from "path";
import { statSync } from "fs";
import type { LLMProvider as RealLLMProvider } from "./llm/index.js";
import { createProvider } from "./llm/index.js";
import { extractIntent } from "./fix/intake/retrospective.js";
import { generateMissingTestsForReport } from "./fix/intake/missingTestForRetrospective.js";
import type {
  IntentReport,
  IntentReportIntent,
  IntentReportConstraintCandidate,
} from "./fix/intake/retrospective.js";
import {
  buildStoredInvariant,
  writeInvariant,
} from "./fix/runtime/invariantStore.js";
import type {
  BugLocus,
  BugSignal,
  InvariantClaim,
  LLMProvider,
} from "./fix/types.js";

interface MineHistoryArgs {
  projectRoot: string;
  since?: string;
  maxCommits: number;
  dryRun: boolean;
  /**
   * Generate missing regression tests as part of the IntentReport's
   * outputBundle.addedTests. Default true. --no-tests disables (e.g. for
   * users on tight LLM budgets, or for fast triage walks). --dry-run also
   * implicitly skips test generation regardless of this flag.
   */
  generateTests: boolean;
  providerName?: string;
}

export async function runMineHistory(rawArgs: string[]): Promise<void> {
  if (rawArgs.includes("--help") || rawArgs.includes("-h")) {
    printMineHistoryHelp();
    return;
  }

  const args = parseArgs(rawArgs);

  if (!isGitRepo(args.projectRoot)) {
    process.stderr.write(`Not a git repository: ${args.projectRoot}\n`);
    process.exit(1);
  }

  const realProvider: RealLLMProvider = createProvider();
  // Bridge the real LLM provider into the fix-layer interface shape that
  // extractIntent expects. Same pattern as cli.fix.ts:711-728.
  const llm: LLMProvider = {
    complete: async (params) => {
      const resp = await realProvider.complete(params.prompt, {
        model: params.model ?? "sonnet",
        systemPrompt: "",
      });
      return resp.text;
    },
    ...(realProvider.agent
      ? {
          agent: (prompt, options) =>
            realProvider.agent!(prompt, {
              ...options,
              model: options.model ?? "sonnet",
            }),
        }
      : {}),
  };

  const shas = listCommits(args.projectRoot, args.since, args.maxCommits);

  console.log(`provekit mine-history`);
  console.log(`  project:     ${args.projectRoot}`);
  console.log(`  since:       ${args.since ?? "<entire history>"}`);
  console.log(`  max-commits: ${args.maxCommits}`);
  console.log(`  dry-run:     ${args.dryRun ? "yes" : "no"}`);
  console.log(`  gen-tests:   ${args.generateTests && !args.dryRun ? "yes" : "no"}`);
  console.log(`  walk order:  oldest → newest (most additive corpus; ` +
    `content-addressable IDs collapse duplicates)`);
  console.log();
  console.log(`Found ${shas.length} commit${shas.length === 1 ? "" : "s"} to walk.`);
  console.log();

  if (shas.length === 0) {
    console.log("Nothing to mine.");
    return;
  }

  let walked = 0;
  let minted = 0;
  let skippedNoCandidate = 0;
  let skippedFileMissing = 0;
  let commitErrors = 0;

  for (const sha of shas) {
    walked++;
    const shortSha = sha.slice(0, 12);

    let report: IntentReport;
    try {
      report = await extractIntent(
        { commitSha: sha },
        llm,
        args.projectRoot,
      );
    } catch (err) {
      commitErrors++;
      const msg = err instanceof Error ? err.message : String(err);
      console.log(`  ${shortSha}  ERROR  ${truncate(msg, 100)}`);
      continue;
    }

    // Step 10: opportunistic missing-test generation. Drive C5's agent over
    // every intent that ships without a test and carries a constraint
    // candidate, then persist. Skip in --dry-run (no LLM-budget commitment
    // for a cost-estimation walk) and in --no-tests (explicit user opt-out).
    // Per-intent failures inside generateMissingTestsForReport are caught
    // there; this top-level try is the same belt-and-suspenders pattern as
    // extractIntent above.
    if (!args.dryRun && args.generateTests) {
      try {
        report = await generateMissingTestsForReport({
          report,
          llm,
          projectRoot: args.projectRoot,
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        console.log(`  ${shortSha}  TEST-GEN-ERROR  ${truncate(msg, 100)}`);
        // Do not bump commitErrors or continue — the report is still
        // good for invariant persistence even if test gen failed wholesale.
      }
    }

    let perCommitMinted = 0;
    let perCommitSkippedNoCandidate = 0;
    let perCommitSkippedFileMissing = 0;

    for (const intent of report.intents) {
      if (!intent.constraintCandidate) {
        perCommitSkippedNoCandidate++;
        skippedNoCandidate++;
        continue;
      }

      // File-existence check against HEAD (the user's working tree). The
      // intent's filePath was relative to the commit's post-tree, but a
      // standing invariant lives against the current code; if the file no
      // longer exists, there's nothing to bind against. The standing
      // runtime would surface this as a decay anyway, so we skip the
      // write entirely rather than mint stale entries.
      const absFilePath = join(args.projectRoot, intent.filePath);
      if (!existsSync(absFilePath)) {
        perCommitSkippedFileMissing++;
        skippedFileMissing++;
        continue;
      }

      if (args.dryRun) {
        perCommitMinted++;
        minted++;
        continue;
      }

      try {
        persistIntent({
          projectRoot: args.projectRoot,
          sha,
          intent,
          candidate: intent.constraintCandidate,
          commitMessage: report.trigger.commitMessage ?? "",
        });
        perCommitMinted++;
        minted++;
      } catch (err) {
        commitErrors++;
        const msg = err instanceof Error ? err.message : String(err);
        console.log(`  ${shortSha}  WRITE-ERROR for intent ${intent.filePath}:` +
          `${intent.lineRange[0]}: ${truncate(msg, 80)}`);
      }
    }

    const summary = [
      `intents=${report.intents.length}`,
      `minted=${perCommitMinted}`,
      perCommitSkippedNoCandidate
        ? `no-candidate=${perCommitSkippedNoCandidate}`
        : null,
      perCommitSkippedFileMissing
        ? `file-missing=${perCommitSkippedFileMissing}`
        : null,
    ]
      .filter(Boolean)
      .join(" ");

    console.log(`  ${shortSha}  ${summary}`);
  }

  console.log();
  console.log(`mine-history summary:`);
  console.log(`  commits walked:          ${walked}`);
  console.log(`  invariants ${args.dryRun ? "would-mint" : "minted"}:        ${minted}`);
  console.log(`  skipped (no candidate):  ${skippedNoCandidate}`);
  console.log(`  skipped (file missing):  ${skippedFileMissing}`);
  console.log(`  per-commit errors:       ${commitErrors}`);

  if (args.dryRun && walked > 0) {
    // Rough cost estimate: B0 retrospective uses sonnet (per modelTiers.ts).
    // Per-commit token mix: ~3-8KB diff in, ~1-2KB JSON out → ~$0.03-0.05.
    const lo = (walked * 0.03).toFixed(2);
    const hi = (walked * 0.05).toFixed(2);
    console.log();
    console.log(`  estimated LLM cost:      ~$${lo}-$${hi} ` +
      `(rough; sonnet @ ~3-8KB in / 1-2KB out per commit)`);
  }

  if (commitErrors > 0) {
    console.log();
    console.log(`note: ${commitErrors} per-commit error${commitErrors === 1 ? "" : "s"} were logged but did not abort the run.`);
  }
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

function parseArgs(rawArgs: string[]): MineHistoryArgs {
  const projectRoot = resolveProjectRoot(rawArgs);
  const since = getFlag(rawArgs, "--since");
  const maxRaw = getFlag(rawArgs, "--max-commits");
  const maxCommits = maxRaw ? parseInt(maxRaw, 10) : 100;
  if (Number.isNaN(maxCommits) || maxCommits <= 0) {
    process.stderr.write(`--max-commits must be a positive integer, got: ${maxRaw}\n`);
    process.exit(2);
  }
  const dryRun = rawArgs.includes("--dry-run");
  const generateTests = !rawArgs.includes("--no-tests");
  const providerName = getFlag(rawArgs, "--provider");

  const out: MineHistoryArgs = {
    projectRoot,
    maxCommits,
    dryRun,
    generateTests,
  };
  if (since !== undefined) out.since = since;
  if (providerName !== undefined) out.providerName = providerName;
  return out;
}

function getFlag(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  return idx !== -1 && idx + 1 < args.length ? args[idx + 1] : undefined;
}

function resolveProjectRoot(args: string[]): string {
  // Skip flag values: any positional after a recognized flag with a value
  // is the flag's value, not a project root. We just take the first
  // positional that isn't preceded by a value-taking flag.
  const valueTakingFlags = new Set(["--since", "--max-commits", "--provider"]);
  for (let i = 0; i < args.length; i++) {
    const a = args[i]!;
    if (a.startsWith("-")) continue;
    const prev = i > 0 ? args[i - 1]! : "";
    if (valueTakingFlags.has(prev)) continue;
    return resolve(a);
  }
  return findProjectRoot(process.cwd());
}

function findProjectRoot(startDir: string): string {
  let dir = startDir;
  while (dir !== dirname(dir)) {
    for (const c of [".provekit", "package.json", ".git"]) {
      try {
        const s = statSync(resolve(dir, c));
        if (s.isDirectory() || s.isFile()) return dir;
      } catch {
        continue;
      }
    }
    dir = dirname(dir);
  }
  return startDir;
}

// ---------------------------------------------------------------------------
// Git plumbing
// ---------------------------------------------------------------------------

function isGitRepo(projectRoot: string): boolean {
  try {
    execFileSync("git", ["rev-parse", "--is-inside-work-tree"], {
      cwd: projectRoot,
      stdio: "pipe",
      encoding: "utf-8",
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * List commit SHAs to walk, oldest-first, capped at maxCommits.
 *
 * --since handling: git's --since flag is approxidate-only ("2 weeks ago",
 * "2024-01-01"). Passing a sha through --since produces silent
 * mismatches (git interprets it as a date string and matches nothing or
 * everything). We sniff the shape: a hex string of length 7-40 is treated
 * as a sha, dispatched via `<sha>..HEAD` range. Anything else goes through
 * --since as a date.
 */
function listCommits(
  projectRoot: string,
  since: string | undefined,
  maxCommits: number,
): string[] {
  const args = ["log", "--format=%H", "--reverse"];

  if (since) {
    if (/^[0-9a-fA-F]{7,40}$/.test(since)) {
      // sha range: include commits after `since` up to HEAD (exclusive of
      // `since` itself). For mining the very first commit, the user can
      // pass a date instead.
      args.push(`${since}..HEAD`);
    } else {
      args.push(`--since=${since}`);
    }
  }

  let raw: string;
  try {
    raw = execFileSync("git", args, {
      cwd: projectRoot,
      encoding: "utf-8",
      maxBuffer: 64 * 1024 * 1024,
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    process.stderr.write(`git log failed: ${msg}\n`);
    process.exit(1);
  }

  const all = raw.split("\n").filter((l) => l.length > 0);
  return all.slice(0, maxCommits);
}

// ---------------------------------------------------------------------------
// IntentReport → StoredInvariant translation
// ---------------------------------------------------------------------------

/**
 * Translate one intent + its constraint candidate into the exact shape
 * buildStoredInvariant expects, and persist it.
 *
 * Spec gap filled here: B0 retrospective's IntentReport contains a
 * constraintCandidate with `smtSketch` + `kind` + `validationStatus`, but
 * buildStoredInvariant expects an InvariantClaim with formalExpression +
 * bindings + complexity + witness + citations. The two shapes are sibling
 * representations of the same property; this function does the field
 * mapping rather than re-running an LLM through C1.
 *
 * Defaults applied per spec gap notes in the task:
 *   - InvariantClaim.complexity = 0   (no proof complexity computed at mine time)
 *   - InvariantClaim.witness = null   (no Z3 witness; verify-time discovery)
 *   - InvariantClaim.citations = []   (intent.citations are similar but not identical
 *                                      shape: smtClause/sourceQuote vs smt_clause/
 *                                      source_quote — translate field names too)
 *   - InvariantClaim.bindings = []    (no SMT-constant → AST-node binding resolved
 *                                      yet; the standing runtime's binding resolver
 *                                      handles this at verify time)
 *   - BugLocus SAST fields = "" / [] (mine-history doesn't index per-commit SAST;
 *                                     these are populated by B2/Investigate in the
 *                                     prospective path. The standing runtime's path
 *                                     enumerator re-resolves callsite at verify time
 *                                     against the current substrate.)
 *   - BugSignal.source = "mine-history" (intake source string; not a closed enum
 *                                        per types.ts)
 */
function persistIntent(args: {
  projectRoot: string;
  sha: string;
  intent: IntentReportIntent;
  candidate: IntentReportConstraintCandidate;
  commitMessage: string;
}): void {
  const { projectRoot, sha, intent, candidate, commitMessage } = args;

  const absFilePath = join(projectRoot, intent.filePath);

  const claim: InvariantClaim = {
    principleId: null,
    description: intent.intent,
    formalExpression: candidate.smtSketch,
    bindings: [],
    complexity: 0,
    witness: null,
    citations: (intent.citations ?? []).map((c) => ({
      smt_clause: c.smtClause,
      source_quote: c.sourceQuote,
    })),
    llmKind: candidate.kind,
  };

  const locus: BugLocus = {
    file: absFilePath,
    line: intent.lineRange[0],
    confidence: 0.5,
    // SAST-structural fields stubbed: mine-history doesn't run an
    // Investigate/B2 pass per commit. Empty values are coherent because
    // the standing runtime's path enumerator re-resolves at verify time
    // against the current substrate, not against fields stored here.
    primaryNode: "",
    containingFunction: "",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
  };

  const summary = intent.intent.length > 200
    ? intent.intent.slice(0, 197) + "..."
    : intent.intent;

  const signal: BugSignal = {
    source: "mine-history",
    rawText: `commit ${sha}\n${commitMessage}`.trim(),
    summary,
    failureDescription:
      `Mined retrospectively from commit ${sha.slice(0, 12)}: ${intent.intent}`,
    codeReferences: [
      {
        file: intent.filePath,
        line: intent.lineRange[0],
      },
    ],
  };

  const stored = buildStoredInvariant({
    claim,
    signal,
    locus,
    test: null,
    patchSha: sha,
    // v1: no node hashes resolved at mine time. The orchestrator path
    // (orchestrator.ts:248) does the same; the standing runtime detects
    // decay by re-resolving at verify time.
    bindingNodeHashes: new Map(),
  });

  writeInvariant(projectRoot, stored);
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 3) + "...";
}

function printMineHistoryHelp(): void {
  console.log(`provekit mine-history — bootstrap invariants from existing git history.`);
  console.log();
  console.log(`Walks the project's git log oldest-to-newest, runs B0 retrospective`);
  console.log(`intent extraction against each commit, and persists every`);
  console.log(`constraint-shaped intent to .provekit/invariants/<id>.json.`);
  console.log();
  console.log(`Usage:`);
  console.log(`  provekit mine-history [project] [options]`);
  console.log();
  console.log(`Options:`);
  console.log(`  --since <commit-or-date>  Starting point. Hex sha (7-40 chars) → range`);
  console.log(`                            <sha>..HEAD; anything else → git --since=<date>`);
  console.log(`                            (e.g. "2 weeks ago", "2024-01-01"). When omitted,`);
  console.log(`                            walks the entire history from the first commit.`);
  console.log(`  --max-commits N           Cap the walk (default 100). Useful for cost-bounding.`);
  console.log(`  --dry-run                 Run extractIntent but do not write invariants;`);
  console.log(`                            print would-mint counts + a rough cost estimate.`);
  console.log(`                            Implies --no-tests (no overlay or C5 agent runs).`);
  console.log(`  --no-tests                Skip missing-test generation. By default, when an`);
  console.log(`                            intent ships without a regression test, the run`);
  console.log(`                            drives C5 to synthesize one and appends it to the`);
  console.log(`                            report's outputBundle.addedTests. Disable for`);
  console.log(`                            tight LLM budgets or for fast triage walks.`);
  console.log(`  --provider <name>         LLM provider (claude-agent, opencode, openai,`);
  console.log(`                            openrouter, pool). Defaults to claude-agent.`);
  console.log();
  console.log(`Walk order: oldest → newest. Same constraint discovered at multiple commits`);
  console.log(`collapses to the same .provekit/invariants/<id>.json file (content-addressable).`);
}
