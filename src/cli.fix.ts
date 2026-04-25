/**
 * B4: CLI entry point for `provekit fix <ref>`.
 *
 * Resolves a <ref> into raw text + source kind, runs intake → locate →
 * classify, pretty-prints the plan, and prompts for confirmation.
 *
 * The testable core is runFixLoopCli(args). The runFix(argv) wrapper parses
 * argv + resolves dependencies + calls runFixLoopCli + translates exit codes
 * into process.exit(). Tests exercise runFixLoopCli directly with stub deps.
 *
 * Cut list (deferred to later sections):
 *   - gh:<number>: actual GitHub API fetch (v1 treats as plain text)
 *   - http(s)://...: actual URL fetch (v1 treats as plain text)
 *   - orchestrator invocation (B5)
 */

import { existsSync, readFileSync } from "fs";
import { createInterface } from "readline";
import { Readable, Writable } from "stream";
import { eq } from "drizzle-orm";

import { parseBugSignal } from "./fix/intake.js";
import { locate } from "./fix/locate.js";
import { classify, ClassifyError } from "./fix/classify.js";
import { openDb, type Db } from "./db/index.js";
import { gapReports, clauses } from "./db/schema/index.js";
import { nodes, nodeBinding, files as sastFiles } from "./sast/schema/index.js";
import { createProvider } from "./llm/index.js";
import type { LLMProvider as RealLLMProvider } from "./llm/index.js";
import { resolve, join } from "path";
import { writeFileSync } from "fs";
import type { LLMProvider, RemediationPlan, BugLocus, BugSignal } from "./fix/types.js";
import { runFixLoop } from "./fix/orchestrator.js";
import type { RunFixLoopArgs } from "./fix/orchestrator.js";
import {
  createFixLoopLogger,
  createNoopLogger,
  loggedComplete,
  type FixLoopLogger,
} from "./fix/logger.js";

export { ClassifyError };

const VERSION = "0.3.0";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface RunFixArgs {
  ref: string;
  db: Db;
  llm: LLMProvider;
  /** false when --no-confirm */
  confirm: boolean;
  /** true when --dry-run */
  dryRun: boolean;
  /** true when --apply flag is passed; enables autoApply (cherry-pick) mode */
  apply?: boolean;
  /** override maxComplementarySites (default 10) */
  maxSites?: number;
  stdout: NodeJS.WritableStream;
  stderr: NodeJS.WritableStream;
  /** for reading from - or for confirmation prompt */
  stdin: NodeJS.ReadableStream;
  /**
   * Optional override for testing. Defaults to importing runFixLoop from orchestrator.
   * Inject a stub here to avoid running all stages in CLI unit tests.
   */
  runFixLoopFn?: typeof runFixLoop;
  /** Enable verbose logging (full prompts + responses). Default false. */
  verbose?: boolean;
  /** Pre-constructed logger. When omitted, createFixLoopLogger is used (or noop for dryRun). */
  logger?: FixLoopLogger;
  /** Override the log file path. Defaults to .provekit/fix-loop-<timestamp>.log */
  logFilePath?: string;
}

// ---------------------------------------------------------------------------
// Ref resolution
// ---------------------------------------------------------------------------

interface ResolvedRef {
  text: string;
  source: string;
  context?: unknown;
}

async function resolveRef(
  ref: string,
  db: Db,
  stdin: NodeJS.ReadableStream,
): Promise<ResolvedRef> {
  // 1. gap_report:<id>
  if (ref.startsWith("gap_report:")) {
    const idStr = ref.slice("gap_report:".length);
    const id = parseInt(idStr, 10);
    if (isNaN(id)) {
      throw new Error(`gap_report ref has invalid id: '${idStr}'`);
    }

    // Fetch the gap_report row joined with its clause for context
    const row = db
      .select({
        id: gapReports.id,
        kind: gapReports.kind,
        atNodeRef: gapReports.atNodeRef,
        explanation: gapReports.explanation,
        smtConstant: gapReports.smtConstant,
        clauseId: gapReports.clauseId,
        contractKey: clauses.contractKey,
        principleName: clauses.principleName,
      })
      .from(gapReports)
      .innerJoin(clauses, eq(clauses.id, gapReports.clauseId))
      .where(eq(gapReports.id, id))
      .get();

    if (!row) {
      throw new Error(`gap_report id ${id} not found in database`);
    }

    // Parse sourceLine from atNodeRef (format: "file.ts:42" or "file.ts:42:fn")
    const sourceLine = row.atNodeRef ?? undefined;
    const reason = row.explanation ?? row.kind ?? "SAST gap finding";

    return {
      text: reason,
      source: "gap_report",
      context: {
        gapReportId: row.id,
        reason,
        sourceLine,
        principleId: row.principleName ?? undefined,
      },
    };
  }

  // 2. Stdin marker
  if (ref === "-") {
    const chunks: Buffer[] = [];
    await new Promise<void>((resolve, reject) => {
      stdin.on("data", (chunk: Buffer) => chunks.push(chunk));
      stdin.on("end", resolve);
      stdin.on("error", reject);
    });
    const text = Buffer.concat(chunks).toString("utf-8");
    return { text, source: "report" };
  }

  // 3. gh:<number> shorthand — v1: treat as report with placeholder text
  if (/^gh:\d+$/.test(ref)) {
    const num = ref.slice(3);
    // CUT LIST: actual GitHub API fetch deferred. For now, treat as text.
    return {
      text: `GitHub issue #${num} (fetch deferred — see B4 cut list)`,
      source: "report",
    };
  }

  // 4. http(s):// URL — v1: treat as report with URL as text
  if (ref.startsWith("http://") || ref.startsWith("https://")) {
    // CUT LIST: actual URL fetch deferred.
    return { text: ref, source: "report" };
  }

  // 5. File path — check existence to break tie with plain text
  const resolvedPath = resolve(ref);
  if (existsSync(resolvedPath)) {
    const text = readFileSync(resolvedPath, "utf-8");
    return { text, source: "report" };
  }

  // 6. Plain text fallback
  return { text: ref, source: "report" };
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

function nodeKindAndName(
  db: Db,
  nodeId: string,
): { kind: string; name: string | null } {
  const nodeRow = db
    .select({ kind: nodes.kind })
    .from(nodes)
    .where(eq(nodes.id, nodeId))
    .get();

  const bindingRow = db
    .select({ name: nodeBinding.name })
    .from(nodeBinding)
    .where(eq(nodeBinding.nodeId, nodeId))
    .get();

  return {
    kind: nodeRow?.kind ?? "Unknown",
    name: bindingRow?.name ?? null,
  };
}

function formatIntakeSection(signal: BugSignal, out: NodeJS.WritableStream): void {
  const w = (s: string) => out.write(s + "\n");
  w("Intake");
  w(`  Source: ${signal.source} (adapter: ${signal.source})`);
  w(`  Summary: ${signal.summary}`);
  w(`  Failure: ${signal.failureDescription}`);
  if (signal.fixHint) {
    w(`  Fix hint: ${signal.fixHint}`);
  }
  if (signal.codeReferences.length > 0) {
    w("  Code references:");
    for (const ref of signal.codeReferences) {
      const loc = ref.line !== undefined ? `${ref.file}:${ref.line}` : ref.file;
      const fn = ref.function ? ` (function ${ref.function})` : "";
      w(`    - ${loc}${fn}`);
    }
  }
}

function formatLocateSection(
  locus: BugLocus,
  db: Db,
  out: NodeJS.WritableStream,
): void {
  const w = (s: string) => out.write(s + "\n");
  const primaryInfo = nodeKindAndName(db, locus.primaryNode);
  const primaryKind = primaryInfo.kind;
  const primaryName = primaryInfo.name ? ` '${primaryInfo.name}'` : "";
  const primaryLoc = locus.line !== undefined ? `${locus.file}:${locus.line}` : locus.file;

  const containingInfo = nodeKindAndName(db, locus.containingFunction);
  const containingName = containingInfo.name ?? "(anonymous)";
  const nodeIdPrefix = locus.containingFunction.slice(0, 8);

  w("Locate");
  w(`  Primary: ${primaryLoc} (${primaryKind}${primaryName})`);
  w(`  Containing function: ${containingName} (${nodeIdPrefix}...)`);
  w(`  Related: ${locus.relatedFunctions.length} functions (callers/callees)`);
  w(`  Data-flow ancestors: ${locus.dataFlowAncestors.length}`);
  w(`  Data-flow descendants: ${locus.dataFlowDescendants.length}`);
  w(`  Dominance region: ${locus.dominanceRegion.length} nodes`);
  w(`  Post-dominance region: ${locus.postDominanceRegion.length} nodes`);
}

function formatClassifySection(plan: RemediationPlan, out: NodeJS.WritableStream): void {
  const w = (s: string) => out.write(s + "\n");
  w("Classify");
  w(`  Primary layer: ${plan.primaryLayer}`);
  if (plan.secondaryLayers.length > 0) {
    w(`  Secondary layers: ${plan.secondaryLayers.join(", ")}`);
  }
  if (plan.artifacts.length > 0) {
    w("  Proposed artifacts:");
    for (const a of plan.artifacts) {
      const extra = a.envVar ? ` (${a.envVar})` : a.site ? ` (${a.site})` : "";
      w(`    - ${a.kind}${extra}`);
    }
  }
  w("");
  w(`  Rationale: ${plan.rationale}`);
}

// ---------------------------------------------------------------------------
// Core: runFixLoopCli
// ---------------------------------------------------------------------------

/**
 * Programmatic entry point. Tests call this directly with stub deps.
 *
 * Exit codes:
 *   0 — success (plan printed + confirmed or dry-run)
 *   1 — intake failure
 *   2 — locate failure (null)
 *   3 — classify failure
 *   4 — user declined confirmation
 *   5 — orchestrator returned null bundle (fix loop failed)
 */
export async function runFixLoopCli(args: RunFixArgs): Promise<number> {
  const { ref, db, llm, confirm, dryRun, stdout, stderr } = args;
  const autoApply = args.apply ?? false;
  const maxSites = args.maxSites ?? 10;
  const fixLoopFn = args.runFixLoopFn ?? runFixLoop;
  const verbose = args.verbose ?? false;

  const w = (s: string) => stdout.write(s + "\n");
  const e = (s: string) => stderr.write(s + "\n");

  // Build log file path: .provekit/fix-loop-<timestamp>.log
  const logFilePath = args.logFilePath ??
    join(
      process.cwd(),
      ".provekit",
      `fix-loop-${new Date().toISOString().replace(/[:.]/g, "-")}.log`,
    );

  // Logger: use injected logger (tests), or build real one if not dryRun, else noop.
  const logger: FixLoopLogger = args.logger ??
    (dryRun
      ? createNoopLogger()
      : createFixLoopLogger({ stdout, verbose, logFilePath }));

  try {
    if (!dryRun) {
      w(`provekit fix loop · v${VERSION}`);
      w("");
      logger.info(`Log file: ${logFilePath}`);
    }

    // ── Intake ──────────────────────────────────────────────────────────────
    logger.stage("Intake");
    const t0Intake = Date.now();

    // 1. Resolve ref → text + source
    let resolved: ResolvedRef;
    try {
      resolved = await resolveRef(ref, db, args.stdin);
      logger.info(`  ref source: ${resolved.source}  text length: ${resolved.text.length} chars`);
    } catch (err) {
      logger.error(`resolveRef failed`, { ref, message: (err as Error).message });
      e(`Intake error: ${(err as Error).message}`);
      return 1;
    }

    // 2. Parse bug signal via intake adapter
    let signal: BugSignal;
    try {
      // Wrap the LLM call with timing + logging
      const wrappedLlm: LLMProvider = {
        ...llm,
        complete: (params) => loggedComplete(logger, "Intake", llm, params),
      };
      signal = await parseBugSignal(
        { text: resolved.text, source: resolved.source, context: resolved.context },
        wrappedLlm,
      );
      logger.info(`  signal summary: ${signal.summary.slice(0, 120)}`);
      logger.info(`  code references: ${signal.codeReferences.length}`);
      for (const cr of signal.codeReferences) {
        const loc = cr.line !== undefined ? `${cr.file}:${cr.line}` : cr.file;
        const fn = cr.function ? ` (${cr.function})` : "";
        logger.info(`    - ${loc}${fn}`);
      }
      logger.info(`  Intake stage OK in ${Date.now() - t0Intake}ms`);
    } catch (err) {
      logger.error(`parseBugSignal failed`, {
        source: resolved.source,
        textLen: resolved.text.length,
        message: (err as Error).message,
      });
      e(`Intake error: ${(err as Error).message}`);
      // List registered adapters
      const { listIntakeAdapters } = await import("./fix/intake.js");
      const names = listIntakeAdapters()
        .map((a) => a.name)
        .join(", ");
      e(`Registered adapters: ${names}`);
      return 1;
    }

    // ── Locate ───────────────────────────────────────────────────────────────
    logger.stage("Locate");
    const t0Locate = Date.now();
    let locus: BugLocus | null;
    try {
      locus = locate(db, signal);
    } catch (err) {
      logger.error(`locate threw`, { message: (err as Error).message });
      e(`Locate error: ${(err as Error).message}`);
      return 2;
    }

    if (locus === null) {
      // Detailed error: dump codeReferences and DB files that were searched
      const allFilePaths = db
        .select({ path: sastFiles.path })
        .from(sastFiles)
        .all()
        .map((r) => r.path);

      const refs = signal.codeReferences;
      const refFiles = refs.map((r) => r.file);

      // Which refs had any DB file match (exact or suffix)?
      const matchedFiles: Record<string, string[]> = {};
      for (const refFile of refFiles) {
        const suffix = refFile.startsWith("/") ? refFile : `/${refFile}`;
        const matched = allFilePaths.filter(
          (p) => p === refFile || p.endsWith(suffix),
        );
        matchedFiles[refFile] = matched;
      }

      logger.error("locate: no candidates matched — locus is null", {
        codeReferences: refs,
        dbFileCount: allFilePaths.length,
        matchedByRef: matchedFiles,
        searchedBy: "exact match then suffix match then function name binding",
      });

      e("Unable to resolve code references in report. Cannot continue.");
      return 2;
    }

    logger.info(`  locus resolved: ${locus.file}:${locus.line} confidence=${locus.confidence.toFixed(2)}`);
    logger.info(`  primary node: ${locus.primaryNode}`);
    logger.info(`  containing function: ${locus.containingFunction}`);
    logger.info(`  related: ${locus.relatedFunctions.length} df-ancestors: ${locus.dataFlowAncestors.length} df-desc: ${locus.dataFlowDescendants.length}`);
    logger.info(`  Locate stage OK in ${Date.now() - t0Locate}ms`);

    // ── Classify ─────────────────────────────────────────────────────────────
    logger.stage("Classify");
    const t0Classify = Date.now();
    let plan: RemediationPlan;
    try {
      const wrappedLlm: LLMProvider = {
        ...llm,
        complete: (params) => loggedComplete(logger, "Classify", llm, params),
      };
      plan = await classify(signal, locus, wrappedLlm);
      logger.info(`  primary layer: ${plan.primaryLayer}`);
      if (plan.secondaryLayers.length) {
        logger.info(`  secondary: ${plan.secondaryLayers.join(", ")}`);
      }
      logger.info(`  artifacts: ${plan.artifacts.map((a) => a.kind).join(", ") || "(none)"}`);
      logger.info(`  Classify stage OK in ${Date.now() - t0Classify}ms`);
    } catch (err) {
      logger.error(`classify failed`, { message: (err as Error).message });
      e(`Classify error: ${(err as Error).message}`);
      return 3;
    }

    // ── Output ───────────────────────────────────────────────────────────────
    if (dryRun) {
      // JSON output for scripting
      stdout.write(JSON.stringify({ signal, locus, plan }, null, 2) + "\n");
      return 0;
    }

    // Pretty-print the three sections
    formatIntakeSection(signal, stdout);
    w("");
    formatLocateSection(locus, db, stdout);
    w("");
    formatClassifySection(plan, stdout);
    w("");
    w("━━━ Plan ready ━━━");
    w("");

    // 6. Confirmation prompt (or auto-run)
    if (confirm) {
      const answer = await askYN(args.stdin, stdout);
      if (!answer) {
        w("Aborted.");
        return 4;
      }
    }

    // ── Orchestrator ─────────────────────────────────────────────────────────
    logger.stage("Orchestrator");
    w("Running fix loop...");

    const fixLoopArgs: RunFixLoopArgs = {
      signal,
      locus,
      plan,
      db,
      llm,
      options: {
        autoApply,
        maxComplementarySites: maxSites,
        confidenceThreshold: 0.5,
      },
      logger,
    };

    const result = await fixLoopFn(fixLoopArgs);

    if (result.bundle === null) {
      logger.error(`fix loop returned null bundle`, {
        reason: result.reason,
        auditTrail: result.auditTrail,
      });
      e(`Fix loop failed: ${result.reason ?? "unknown reason"}`);
      // One-line audit summary: last error or skipped stage
      const lastFault = [...result.auditTrail]
        .reverse()
        .find((entry) => entry.kind === "error" || entry.kind === "skipped");
      if (lastFault) {
        e(`  ${lastFault.stage}: ${lastFault.detail}`);
      }
      return 5;
    }

    logger.stage("Bundle");

    // 8. Print bundle summary
    const bundle = result.bundle;
    w(`Bundle type: ${bundle.bundleType}`);
    w(`Artifacts:`);
    w(`  primaryFix: ${bundle.artifacts.primaryFix !== null ? "yes" : "none"}`);
    w(`  complementary: ${bundle.artifacts.complementary.length}`);
    w(`  test: ${bundle.artifacts.test !== null ? "yes" : "none"}`);
    w(`  principle: ${bundle.artifacts.principle !== null ? "yes" : "none"}`);
    w(`Confidence: ${bundle.confidence.toFixed(2)}`);
    w(`Coherence:`);
    for (const [key, val] of Object.entries(bundle.coherence)) {
      if (val !== null) {
        w(`  ${key}: ${val}`);
      }
    }

    logger.info(`  bundle type: ${bundle.bundleType} confidence: ${bundle.confidence.toFixed(2)}`);

    if (autoApply && result.applyResult?.commitSha) {
      w(`Committed: ${result.applyResult.commitSha}`);
      logger.info(`  committed: ${result.applyResult.commitSha}`);
    }

    if (!autoApply && result.applyResult?.prDraft) {
      const patchPath = join(process.cwd(), "provekit-fix.patch");
      const mdPath = join(process.cwd(), "provekit-fix.md");
      writeFileSync(patchPath, result.applyResult.prDraft.patch, "utf-8");
      writeFileSync(mdPath, result.applyResult.prDraft.prBody, "utf-8");
      w(`Patch written to: ${patchPath}`);
      w(`PR body written to: ${mdPath}`);
    }

    return 0;
  } finally {
    logger.close();
  }
}

// ---------------------------------------------------------------------------
// Confirmation prompt
// ---------------------------------------------------------------------------

async function askYN(
  stdin: NodeJS.ReadableStream,
  stdout: NodeJS.WritableStream,
): Promise<boolean> {
  stdout.write("Proceed? [y/N] ");

  return new Promise((resolve) => {
    // If stdin is already ended or is a closed stream, treat as "n"
    const rl = createInterface({
      input: stdin as Readable,
      output: new Writable({ write(_chunk, _enc, cb) { cb(); } }),
      terminal: false,
    });

    let answered = false;

    rl.once("line", (line: string) => {
      answered = true;
      rl.close();
      const answer = line.trim().toLowerCase();
      resolve(answer === "y" || answer === "yes");
    });

    rl.once("close", () => {
      if (!answered) {
        resolve(false);
      }
    });
  });
}

// ---------------------------------------------------------------------------
// argv shim: runFix
// ---------------------------------------------------------------------------

/**
 * argv parser + shim. Reads env/process/fs for a real run.
 * Tests use runFixLoopCli directly.
 */
export async function runFix(argv: string[]): Promise<void> {
  const noConfirm = argv.includes("--no-confirm");
  const dryRun = argv.includes("--dry-run");
  const apply = argv.includes("--apply");
  const verbose = argv.includes("--verbose") || argv.includes("-v");

  // --max-sites N
  const maxSitesIdx = argv.indexOf("--max-sites");
  const maxSites = maxSitesIdx !== -1 && maxSitesIdx + 1 < argv.length
    ? parseInt(argv[maxSitesIdx + 1], 10)
    : 10;

  const positionals = argv.filter((a) => !a.startsWith("-"));
  const ref = positionals[0];

  if (!ref) {
    process.stderr.write("Usage: provekit fix <ref> [--no-confirm] [--dry-run] [--apply] [--max-sites N] [--verbose]\n");
    process.exit(1);
  }

  // Resolve project root → DB path
  const { statSync, existsSync: fsExistsSync } = await import("fs");
  const { dirname: pathDirname, resolve: pathResolve } = await import("path");

  function findProjectRoot(startDir: string): string {
    let dir = startDir;
    while (dir !== pathDirname(dir)) {
      for (const c of [".provekit", "package.json", ".git"]) {
        try {
          const p = pathResolve(dir, c);
          const s = statSync(p);
          if (s.isDirectory() || s.isFile()) return dir;
        } catch { continue; }
      }
      dir = pathDirname(dir);
    }
    return startDir;
  }

  const projectRoot = findProjectRoot(process.cwd());
  const dbPath = join(projectRoot, ".provekit", "provekit.db");

  if (!fsExistsSync(dbPath)) {
    process.stderr.write(`No database found at ${dbPath}. Run 'provekit analyze' first.\n`);
    process.exit(1);
  }

  const db = openDb(dbPath);
  const realProvider: RealLLMProvider = createProvider();
  // Bridge: wrap the real LLM provider to match the fix-layer interface.
  // Forward agent() when the underlying provider implements it — this is how
  // the capture-the-change pipeline (C3-C6) gets activated.
  const llm: LLMProvider = {
    complete: async (params) => {
      const resp = await realProvider.complete(params.prompt, {
        model: params.model ?? "opus",
        systemPrompt: "",
      });
      return resp.text;
    },
    ...(realProvider.agent
      ? {
          agent: (prompt, options) => realProvider.agent!(prompt, { ...options, model: options.model ?? "opus" }),
        }
      : {}),
  };

  // Register all intake adapters + remediation layers (side-effects via imports)
  await import("./fix/intake.js");
  await import("./fix/classify.js");

  const exitCode = await runFixLoopCli({
    ref,
    db,
    llm,
    confirm: !noConfirm,
    dryRun,
    apply,
    maxSites,
    verbose,
    stdout: process.stdout,
    stderr: process.stderr,
    stdin: process.stdin,
  });

  db.$client.close();
  process.exit(exitCode);
}
