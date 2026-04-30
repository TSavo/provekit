#!/usr/bin/env node

import { statSync, readFileSync, existsSync } from "fs";
import { resolve, dirname, relative, join } from "path";
import { Pipeline } from "./pipeline";
import { SignalRegistry } from "./signals";
import { DiffAnalyzer, HookInstaller, ProofDiff } from "./git";
import { ContractStore, signalKey } from "./contracts";
import { createProvider } from "./llm";
import { openDb, type Db } from "./db/index.js";
import { gapReports, clauses, runtimeValues } from "./db/schema/index.js";
import { eq } from "drizzle-orm";
import { runFix } from "./cli.fix.js";
import { runMineHistory } from "./cli.mineHistory.js";
import { runMint } from "./cli.mint.js";
import { runDump } from "./cli.dump.js";
import { runAttest } from "./cli.attest.js";
import { buildSASTForFile } from "./sast/builder.js";
import { WorkflowRunner } from "./workflow/runner.js";
import { runManifest } from "./workflow/manifest.js";
import { parseArgv } from "./workflow/producers/parseArgv.js";
import {
  defaultRegistryFactories,
  discoverWorkflows,
  loadDispatchManifest,
  registerDispatchRegistries,
  WORKFLOWS_DIR,
} from "./workflows/_dispatch.js";

const VERSION = "0.4.0";

/**
 * Command-name aliases. The brand is "ProvekIt — Prove It," and English
 * grammar wants different verb forms in different sentences:
 *   provekit must X    /  proveit must X
 *   proveit will  X    →  must
 *   proveit always X   →  must
 *   proveit verify X   /  proveit verifies X
 *   proveit change X   /  proveit changes X
 *   proveit prove  X   /  proveit proves X
 * The expansion is purely lexical: argv[0] gets rewritten to the
 * canonical verb before parseArgv sees it. Pure aliasing — no
 * separate dispatch logic.
 */
const COMMAND_ALIASES: Readonly<Record<string, string>> = Object.freeze({
  // Authoring (kit emits IR for an invariant)
  will: "must",
  always: "must",
  shall: "must",
  // Verification
  verifies: "verify",
  // Code-change workflows
  changes: "change",
  proves: "prove",
});

export function expandCommandAlias(argv: string[]): string[] {
  if (argv.length === 0) return argv;
  const first = argv[0]!;
  const canonical = COMMAND_ALIASES[first];
  if (!canonical) return argv;
  return [canonical, ...argv.slice(1)];
}

async function main(): Promise<void> {
  const args = expandCommandAlias(process.argv.slice(2));

  if (args.includes("--version")) {
    console.log(`provekit v${VERSION}`);
    console.log("The Kit to Prove It's Fixed.");
    process.exit(0);
  }

  // Walk src/workflows/ once at startup. Workflows with a `cli:` block
  // become addressable as `provekit <name>`; the rest are internals.
  const { cliBlocks, manifestPaths } = discoverWorkflows(WORKFLOWS_DIR);
  const factories = defaultRegistryFactories();

  // Pre-parse argv. parse-argv is the meta-dispatcher's first Stage,
  // exported as a pure function so the CLI entry can intercept help
  // and unknown-command paths without invoking the dispatcher manifest
  // (the manifest format has no conditionals; only the happy path runs
  // through it).
  const parsed = parseArgv(args, cliBlocks);

  if (parsed.kind === "help") {
    console.log(parsed.helpText);
    process.exit(0);
  }
  if (parsed.kind === "unknown") {
    // Fall through to the legacy switch for commands the meta-dispatcher
    // doesn't yet route. Migration of legacy commands to YAML happens
    // incrementally; the switch shrinks per commit.
    await runLegacyCommand(parsed.command, args.slice(1));
    return;
  }

  // Dispatchable command. If the workflow has a registered factory,
  // route through the meta-dispatcher manifest. If not, fall through
  // to legacy (the cli: block exists but the workflow's factory hasn't
  // been wired into _dispatch.ts yet).
  if (!factories[parsed.command]) {
    await runLegacyCommand(parsed.command, args.slice(1));
    return;
  }

  const dbPath = resolveDbPath(args);
  // Ensure the DB schema is up-to-date before any dispatched workflow
  // tries to read/write through it. migrate() is idempotent — applying
  // already-applied migrations is a no-op — so paying this cost on
  // every dispatcher invocation is cheap and lets `provekit diff`
  // (or any workflow) work in projects that haven't run `provekit init`
  // explicitly. The init command continues to do extra setup (signal
  // index, sample principles); this just guarantees the DB schema is
  // present so commands don't crash with "no such table".
  {
    const { mkdirSync } = await import("fs");
    const { dirname: pathDirname } = await import("path");
    const { migrate: runMigrate } = await import("drizzle-orm/better-sqlite3/migrator");
    mkdirSync(pathDirname(dbPath), { recursive: true });
    const initDb = openDb(dbPath);
    try {
      runMigrate(initDb, { migrationsFolder: join(__dirname, "..", "drizzle") });
    } finally {
      initDb.$client.close();
    }
  }
  const db = openDb(dbPath);
  try {
    const llm = process.env.PROVEKIT_LLM ? createProvider(process.env.PROVEKIT_LLM) : undefined;
    const projectRoot = resolveProjectRoot(args);
    const dispatcher = loadDispatchManifest();
    const { registry, actionRegistry } = registerDispatchRegistries({ db });
    const runner = new WorkflowRunner(
      db,
      { name: dispatcher.name, cid: dispatcher.cid },
      registry,
    );
    await runManifest(
      runner,
      registry,
      dispatcher,
      {
        argv: args,
        cliBlocks,
        manifestPaths,
        factories,
        deps: { db, llm, projectRoot },
      },
      actionRegistry,
    );
  } finally {
    db.$client.close();
  }
}

/**
 * Legacy fallback. Routes commands not yet migrated to YAML workflows
 * through the prior imperative implementations. Each entry shrinks
 * the switch as the corresponding workflow lands a `cli:` block AND
 * a registered factory in `defaultRegistryFactories()`.
 */
async function runLegacyCommand(command: string, rest: string[]): Promise<void> {
  switch (command) {
    case "init":     await runInit(rest); break;
    case "analyze":  await runAnalyze(rest); break;
    case "verify":   await runVerify(rest); break;
    case "derive":   await runDerive(rest); break;
    case "diff":     runDiff(rest); break;
    case "explain":  runExplain(rest); break;
    case "report":   runReport(rest); break;
    case "hook":     runHook(rest); break;
    case "override": runOverride(rest); break;
    case "fix":     await runFix(rest); break;
    case "prove":   await runFix(rest); break;
    case "change":  await runFix(rest); break;
    case "lint":    await runLint(rest); break;
    case "invariants": await runInvariants(rest); break;
    case "mine-history": await runMineHistory(rest); break;
    case "mint":         await runMint(rest); break;
    case "dump":         await runDump(rest); break;
    case "attest":       await runAttest(rest); break;
    default:
      console.error(`Unknown command: ${command}`);
      console.error("Run `provekit --help` to see available commands.");
      process.exit(1);
  }
}

function resolveDbPath(args: string[]): string {
  const projectRoot = resolveProjectRoot(args);
  return join(projectRoot, ".provekit", "provekit.db");
}

// ---------------------------------------------------------------------------
// init — scan, optionally analyze, install hook
// ---------------------------------------------------------------------------

async function runInit(args: string[]): Promise<void> {
  const projectRoot = resolveProjectRoot(args);
  const signalRegistry = SignalRegistry.createDefault();

  console.log(`provekit v${VERSION}`);
  console.log("The Kit to Prove It's Fixed.");
  console.log(`Initializing in ${projectRoot}`);
  console.log();

  const diff = new DiffAnalyzer(projectRoot);
  if (!diff.isGitRepo()) {
    console.error("Not a git repository. provekit requires git.");
    process.exit(1);
  }

  // Scan for signals
  const { execSync } = require("child_process");
  let tsFiles: string[];
  try {
    const output = execSync("git ls-files '*.ts' '*.tsx'", {
      cwd: projectRoot, encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"],
    }).trim();
    tsFiles = output ? output.split("\n").filter((f: string) => !f.includes("node_modules") && !f.endsWith(".d.ts")).map((f: string) => resolve(projectRoot, f)) : [];
  } catch {
    tsFiles = [];
  }

  const { parseFile } = require("./parser");
  const TypeScript = require("tree-sitter-typescript");
  const Parser = require("tree-sitter");

  let totalSignals = 0;
  let fileCount = 0;

  for (const file of tsFiles) {
    try {
      const source = readFileSync(file, "utf-8");
      const parser = new Parser();
      parser.setLanguage(TypeScript.typescript);
      const tree = parser.parse(source);
      const signals = signalRegistry.findAll(file, source, tree);
      if (signals.length > 0) {
        totalSignals += signals.length;
        fileCount++;
      }
    } catch { /* skip unreadable files */ }
  }

  console.log(`Found ${totalSignals} signals across ${fileCount} files`);
  console.log(`  Signal generators: ${signalRegistry.getGeneratorNames().join(", ")}`);
  console.log();

  // Create the persistent .provekit/provekit.db with migrations applied.
  // Without this, `provekit fix --apply` and other tools that expect the
  // SAST schema fail on a fresh project — the F1 from the P2 dogfood
  // surfaced this in 2026-04-26: init scaffolding only handled the signal
  // index, not the SAST DB. Real-LLM fix runs require the schema to be
  // pre-applied so per-fix builds can populate it incrementally.
  const dbPath = join(projectRoot, ".provekit", "provekit.db");
  const dbExisted = existsSync(dbPath);
  const { migrate } = await import("drizzle-orm/better-sqlite3/migrator");
  const initDb = openDb(dbPath);
  try {
    migrate(initDb, { migrationsFolder: join(__dirname, "..", "drizzle") });
  } finally {
    initDb.$client.close();
  }
  console.log(`Database: ${dbExisted ? "migrations applied" : "created"} at .provekit/provekit.db`);
  console.log();

  // Seed the principle library. The bundled .provekit/principles/ directory
  // ships in the npm package via the "files" array in package.json. Idempotent:
  // if the target already has any files we leave it alone — users edit their
  // local copy and we don't clobber edits on re-init.
  const principlesResult = seedPrinciples(projectRoot);
  console.log(`Principles: ${principlesResult.message}`);
  console.log();

  // Install hook
  const hookInstaller = new HookInstaller(projectRoot);
  if (!args.includes("--no-hook")) {
    const result = hookInstaller.install();
    console.log(`Git hook: ${result.message}`);
    if (result.path) console.log(`  ${result.path}`);
  }

  // Scaffold a GitHub Actions workflow stub by default. Flag to skip:
  // --no-actions-workflow. The workflow runs `npx provekit invariants verify
  // --ci` on push and pull_request — Channel 1 of the distribution surface
  // (every developer adds it to their CI). Idempotent: never overwrites an
  // existing file.
  const skipWorkflow = args.includes("--no-actions-workflow");
  if (!skipWorkflow) {
    const wfResult = scaffoldGitHubWorkflow(projectRoot);
    console.log(`Workflow:   ${wfResult.message}`);
    if (wfResult.path) console.log(`  ${wfResult.path}`);
  }

  console.log();
  console.log("Next steps:");
  console.log("  provekit lint                Run principle library across the codebase");
  console.log("  provekit invariants verify   Run the standing-invariant gate (Z3, no LLM)");
  console.log("  provekit analyze <file.ts>   Derive proofs for a file");
  console.log("  provekit derive              Derive proofs for changed files");
  console.log("  provekit verify              Run Z3 against cached proofs");
  console.log("  provekit report              Show coverage summary");
}

// ---------------------------------------------------------------------------
// init helpers
// ---------------------------------------------------------------------------

function seedPrinciples(projectRoot: string): { copied: number; skipped: boolean; message: string } {
  const { readdirSync, mkdirSync, copyFileSync, statSync } = require("fs");
  const targetDir = join(projectRoot, ".provekit", "principles");

  // The bundled library lives next to the package's drizzle/ directory:
  // `<package_root>/.provekit/principles/`. At runtime, __dirname is dist/
  // (compiled output) so we step up one level. This mirrors the lookup in
  // runLint() above.
  const bundledDir = join(__dirname, "..", ".provekit", "principles");

  if (!existsSync(bundledDir)) {
    return { copied: 0, skipped: true, message: `bundled library not found at ${bundledDir}` };
  }

  // Idempotence: if the target exists and is non-empty (recursive — count
  // partition subdirs too), leave it alone. v2 of the library uses
  // partitioned directories (universal/, typescript/, ...), so a flat
  // file-only check would miss them.
  function hasAnyFiles(dir: string): boolean {
    try {
      for (const name of readdirSync(dir)) {
        const full = join(dir, name);
        try {
          const st = statSync(full);
          if (st.isFile()) return true;
          if (st.isDirectory() && hasAnyFiles(full)) return true;
        } catch { /* skip unreadable */ }
      }
    } catch { /* nothing */ }
    return false;
  }
  if (existsSync(targetDir) && hasAnyFiles(targetDir)) {
    return { copied: 0, skipped: true, message: `existing library at .provekit/principles/ (left alone)` };
  }

  // Recursive copy so partition subdirectories (universal/, typescript/,
  // cpp/, rust/, ...) seed correctly.
  let copied = 0;
  function copyTree(src: string, dst: string): void {
    mkdirSync(dst, { recursive: true });
    let entries: string[];
    try { entries = readdirSync(src); } catch { return; }
    for (const name of entries) {
      const srcPath = join(src, name);
      const dstPath = join(dst, name);
      try {
        const st = statSync(srcPath);
        if (st.isDirectory()) {
          copyTree(srcPath, dstPath);
        } else if (st.isFile()) {
          copyFileSync(srcPath, dstPath);
          copied++;
        }
      } catch { /* skip */ }
    }
  }
  copyTree(bundledDir, targetDir);
  return { copied, skipped: false, message: `seeded ${copied} principle(s) into .provekit/principles/` };
}

function scaffoldGitHubWorkflow(projectRoot: string): { wrote: boolean; path: string; message: string } {
  const { mkdirSync, writeFileSync } = require("fs");
  const wfDir = join(projectRoot, ".github", "workflows");
  const wfPath = join(wfDir, "provekit.yml");

  if (existsSync(wfPath)) {
    return { wrote: false, path: wfPath, message: "workflow already exists (left alone)" };
  }

  const yaml = `name: ProvekIt
on:
  push:
  pull_request:

jobs:
  prove:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - name: Install
        run: npm ci || npm install
      - name: ProvekIt verify
        run: npx provekit invariants verify --ci
`;

  mkdirSync(wfDir, { recursive: true });
  writeFileSync(wfPath, yaml, "utf-8");
  return { wrote: true, path: wfPath, message: "scaffolded .github/workflows/provekit.yml" };
}

// ---------------------------------------------------------------------------
// analyze — full pipeline on a specific file
// ---------------------------------------------------------------------------

async function runAnalyze(args: string[]): Promise<void> {
  const filePath = resolve(args.find((a) => !a.startsWith("-")) || ".");
  const projectRoot = findProjectRoot(dirname(filePath));
  const model = getFlag(args, "--model") || "sonnet";
  const verbose = args.includes("--verbose") || args.includes("-v");
  const dryRun = args.includes("--dry-run");
  const substrateOnly = args.includes("--substrate-only");

  // --substrate-only: skip the LLM-driven phases-1-5 pipeline. Walk every
  // TS/JS file in the project tree and run the SAST indexer against each.
  // No LLM cost. Sufficient for the standing-runtime's Locate stage to
  // resolve invariant callsites — but does NOT produce signals, contracts,
  // or violations (those need the LLM phases).
  //
  // Use case: bootstrap the substrate before `provekit prove` against a
  // prose intent. Without the substrate populated, Locate has no AST nodes
  // to match codeReferences against and the prove run aborts.
  if (substrateOnly) {
    console.log(`provekit v${VERSION} — substrate-only indexer`);
    console.log(`Project: ${projectRoot}`);
    console.log();

    const { execSync: ex } = require("child_process");
    const tsFiles: string[] = (() => {
      try {
        const out = ex("git ls-files '*.ts' '*.tsx' '*.js' '*.jsx'", {
          cwd: projectRoot,
          encoding: "utf-8",
          stdio: ["pipe", "pipe", "pipe"],
        }).trim();
        return out
          ? out
              .split("\n")
              .filter((f: string) => !f.includes("node_modules") && !f.endsWith(".d.ts"))
              .map((f: string) => resolve(projectRoot, f))
          : [];
      } catch {
        return [];
      }
    })();

    console.log(`Files to index: ${tsFiles.length}`);

    const dbPath = join(projectRoot, ".provekit", "provekit.db");
    const db = openDb(dbPath);
    let indexed = 0;
    let skipped = 0;
    try {
      for (const file of tsFiles) {
        try {
          buildSASTForFile(db, file);
          indexed++;
          if (verbose && indexed % 50 === 0) {
            console.log(`  indexed ${indexed}/${tsFiles.length}...`);
          }
        } catch (err: any) {
          skipped++;
          if (verbose) {
            console.warn(`  SKIP ${file}: ${err?.message ?? err}`);
          }
        }
      }
    } finally {
      db.$client.close();
    }

    console.log();
    console.log(`Substrate indexed: ${indexed} files (${skipped} skipped)`);
    console.log(`Database: ${dbPath}`);
    return;
  }

  const signalRegistry = buildSignalRegistry(args, model);

  console.log(`provekit v${VERSION}`);
  console.log(`File:    ${filePath}`);
  console.log(`Model:   ${model}`);
  console.log(`Signals: ${signalRegistry.getGeneratorNames().join(", ")}`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  const concurrency = parseInt(getFlag(args, "--concurrency") || "5", 10);
  const providerName = getFlag(args, "--provider");
  const provider = providerName ? createProvider(providerName) : undefined;

  const pipeline = new Pipeline();
  const result = await pipeline.runFull({
    entryFilePath: filePath,
    projectRoot,
    model,
    verbose,
    signalRegistry,
    maxConcurrency: concurrency,
    provider,
  });

  printSummary(result);

  // Populate SAST substrate tables (files/nodes/capabilities/data_flow/dominance)
  // so the fix loop's Locate step can find code sites.
  const dbPath = join(projectRoot, ".provekit", "provekit.db");
  const db = openDb(dbPath);
  try {
    // ts-morph parses .js and .jsx via its compiler API. Restricting SAST
    // indexing to TypeScript blocked BugsJS harvest from JS-only projects
    // (Express, Mocha, etc.). Accept all four extensions.
    const sastExtensions = new Set([".ts", ".tsx", ".js", ".jsx"]);
    for (const fileNode of result.graph.files) {
      const ext = fileNode.path.slice(fileNode.path.lastIndexOf("."));
      if (!sastExtensions.has(ext)) continue;
      try {
        buildSASTForFile(db, fileNode.path);
      } catch (err: any) {
        console.warn(`SAST index skipped for ${fileNode.path}: ${err?.message ?? err}`);
      }
    }
  } finally {
    db.$client.close();
  }

  if (args.includes("--issues") || dryRun) {
    await fileIssues(result, dryRun);
  }
}

// ---------------------------------------------------------------------------
// derive — analyze only changed files (diff-powered)
// ---------------------------------------------------------------------------

async function runDerive(args: string[]): Promise<void> {
  const projectRoot = resolveProjectRoot(args);
  const model = getFlag(args, "--model") || "sonnet";
  const verbose = args.includes("--verbose") || args.includes("-v");
  const ref = getFlag(args, "--since") || "HEAD";

  console.log(`provekit v${VERSION} — derive (diff-powered)`);
  console.log(`Project: ${projectRoot}`);
  console.log(`Model:   ${model}`);
  console.log();

  const diff = new DiffAnalyzer(projectRoot);
  const changedFiles = ref === "HEAD"
    ? diff.getWorkingTreeChangedFiles()
    : diff.getChangedFilesSince(ref);

  if (changedFiles.length === 0) {
    console.log("No changed TypeScript files found.");
    process.exit(0);
  }

  console.log(`Changed files (${changedFiles.length}):`);
  for (const f of changedFiles) {
    console.log(`  ${relative(projectRoot, f)}`);
  }
  console.log();

  const signalRegistry = buildSignalRegistry(args, model);
  const entryFile = changedFiles[0]!;
  const pipeline = new Pipeline();
  const result = await pipeline.runFull({
    entryFilePath: entryFile,
    projectRoot,
    model,
    verbose,
    changedFiles,
    signalRegistry,
  });

  printSummary(result);
}

// ---------------------------------------------------------------------------
// verify — incremental in hook mode, Phase 5 only otherwise
// ---------------------------------------------------------------------------

async function runVerify(args: string[]): Promise<void> {
  const projectRoot = resolveProjectRoot(args);
  const isHook = args.includes("--hook");
  const ci = args.includes("--ci");
  const verbose = args.includes("--verbose") || args.includes("-v");
  const model = getFlag(args, "--model") || "sonnet";

  if (isHook) {
    const { DiffAnalyzer } = require("./git");
    const diff = new DiffAnalyzer(projectRoot);
    const changedFiles = diff.getChangedTypeScriptFiles();

    if (changedFiles.length === 0) {
      process.exit(0);
    }

    const signalRegistry = buildSignalRegistry(args, model);
    const pipeline = new Pipeline();
    const result = await pipeline.runIncremental({
      entryFilePath: changedFiles[0]!,
      projectRoot,
      model,
      verbose,
      changedFiles,
      signalRegistry,
    });

    const { PrincipleStore: CIPrincipleStore } = require("./principles");
    const ciPs = new CIPrincipleStore(projectRoot);
    const ciCf = new Map<string, string>();
    for (const p of ciPs.getAll()) ciCf.set(p.id, p.confidence || "low");
    let ciHigh = 0;
    for (const c of new ContractStore(projectRoot).getAll()) {
      for (const v of c.violations) {
        const id = (v.principle || "").replace(/[\[\]]/g, "").trim();
        if ((v.confidence || ciCf.get(id) || "low") === "high") ciHigh++;
      }
    }
    if (ciHigh > 0) { process.exit(1); }
    process.exit(0);
  }

  console.log(`provekit v${VERSION} — verify (Phase 5 only, no LLM, pure Z3)`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  const pipeline = new Pipeline();
  const report = await pipeline.runVerifyOnly(projectRoot, verbose);

  // Bridge enforcement: walk .proof files, discharge per-callsite IR
  // obligations against bridge target preconditions. Composes with the
  // legacy invariants pass; both contribute to the unified verdict.
  const { runBridgeEnforcement, formatBridgeEnforcementReport } = await import(
    "./verifier/bridgeEnforcement.js"
  );
  const bridgeReport = await runBridgeEnforcement(projectRoot);
  if (bridgeReport.totalCallsites > 0 || bridgeReport.loadErrors.length > 0) {
    console.log("Bridge enforcement:");
    process.stdout.write(formatBridgeEnforcementReport(bridgeReport));
  }

  if (ci) {
    const { PrincipleStore } = require("./principles");
    const ciPrinciples = new PrincipleStore(projectRoot);
    const ciConf = new Map<string, string>();
    for (const p of ciPrinciples.getAll()) ciConf.set(p.id, p.confidence || "low");
    const ciStore = new ContractStore(projectRoot);
    let highCount = 0;
    for (const c of ciStore.getAll()) {
      for (const v of c.violations) {
        const id = (v.principle || "").replace(/[\[\]]/g, "").trim();
        const conf = v.confidence || ciConf.get(id) || "low";
        if (conf === "high") highCount++;
      }
    }
    // Bridge violations are exit-1 in CI mode (any unsatisfied / unresolved
    // / lift-error / undecidable / disagreement counts as a high-confidence
    // violation; a discharged callsite is the only "passing" status).
    if (highCount > 0 || bridgeReport.violations > 0) {
      console.log();
      const total = highCount + bridgeReport.violations;
      console.log(
        `${total} high-confidence violation${total === 1 ? "" : "s"} found ` +
          `(${highCount} contract, ${bridgeReport.violations} bridge).`,
      );
      process.exit(1);
    }
    process.exit(0);
  }
}

// ---------------------------------------------------------------------------
// diff — show proof changes between refs
// ---------------------------------------------------------------------------

function runDiff(args: string[]): void {
  const projectRoot = resolveProjectRoot(args);
  const ref = args.find((a) => !a.startsWith("-")) ?? "HEAD~1";

  console.log(`provekit v${VERSION} — proof diff against ${ref}`);
  console.log();

  const proofDiff = new ProofDiff(projectRoot);
  const changes = proofDiff.diffAgainst(ref);

  if (changes.length === 0) {
    console.log("No proof changes.");
    return;
  }

  const symbols: Record<string, string> = {
    added: "+",
    removed: "-",
    regressed: "!",
    fixed: "~",
    unchanged: " ",
  };

  const labels: Record<string, string> = {
    added: "NEW",
    removed: "REMOVED",
    regressed: "REGRESSION",
    fixed: "FIXED",
    unchanged: "",
  };

  for (const change of changes) {
    const sym = symbols[change.type] || "?";
    const label = labels[change.type] || change.type;
    const principle = change.principle ? `[${change.principle}]` : "";
    console.log(`  ${sym} ${change.file}:${change.line}  ${label}: ${change.claim.slice(0, 70)} ${principle}`);
  }

  console.log();
  const regressions = changes.filter((c) => c.type === "regressed").length;
  const added = changes.filter((c) => c.type === "added").length;
  const fixed = changes.filter((c) => c.type === "fixed").length;
  const removed = changes.filter((c) => c.type === "removed").length;
  console.log(`  ${added} new | ${fixed} fixed | ${regressions} regressions | ${removed} removed`);

  if (regressions > 0) {
    console.log();
    console.log(`  ${regressions} proof regression${regressions === 1 ? "" : "s"} detected.`);
  }
}

// ---------------------------------------------------------------------------
// explain — show details for a specific finding
// ---------------------------------------------------------------------------

function runExplain(args: string[]): void {
  const target = args.find((a) => !a.startsWith("-"));
  if (!target) {
    console.error("Usage: provekit explain <file>:<line> [--gaps]");
    process.exit(1);
  }

  const [filePart, linePart] = target.split(":");
  if (!filePart || !linePart) {
    console.error("Usage: provekit explain <file>:<line> [--gaps]");
    process.exit(1);
  }

  const filePath = resolve(filePart);
  const line = parseInt(linePart, 10);
  const projectRoot = findProjectRoot(dirname(filePath));

  const store = new ContractStore(projectRoot);
  const contracts = store.getAll();

  const relPath = relative(projectRoot, filePath);
  const contract = contracts.find((c) => c.key.includes(relPath) && c.line === line)
    || contracts.find((c) => c.key.includes(relPath) && Math.abs(c.line - line) <= 2)
    || contracts.find((c) => c.file === filePath && c.line === line);

  if (!contract) {
    console.error(`No contract found for ${relPath}:${line}`);
    console.error("Run 'provekit analyze' first.");
    process.exit(1);
  }

  if (args.includes("--gaps")) {
    // Encoding-gap output is separate from v1 contract explain. It reads the
    // SQLite gap_reports table populated by the Phase D gap detector.
    const db = openDb(join(projectRoot, ".provekit", "provekit.db"));
    process.stdout.write(explainGaps(db, contract.key));
    db.$client.close();
    return;
  }

  console.log();
  console.log("┌─────────────────────────────────────────────────────┐");
  console.log(`│  ${contract.key}`);
  console.log(`│  Signal hash: ${contract.signal_hash}`);
  console.log("└─────────────────────────────────────────────────────┘");
  console.log();

  if (contract.proven.length > 0) {
    console.log("Proven properties (Z3 confirmed unsat):");
    for (const p of contract.proven) {
      const tag = p.principle ? `[${p.principle}]` : "";
      console.log(`  ✓ ${tag} ${p.claim}`);
      console.log();
      console.log("  Proof (Z3):");
      console.log(`  \`\`\`smt2`);
      for (const smt2Line of p.smt2.split("\n")) {
        console.log(`  ${smt2Line}`);
      }
      console.log(`  \`\`\``);
      console.log();
      const escaped = p.smt2.replace(/'/g, "'\\''");
      console.log(`  Verify: echo '${escaped}' | z3 -in`);
      console.log();
    }
  }

  if (contract.violations.length > 0) {
    console.log("Reachable violations (Z3 confirmed sat):");
    for (const v of contract.violations) {
      const tag = v.principle ? `[${v.principle}]` : "";
      console.log(`  ✗ ${tag} ${v.claim}`);
      console.log();
      console.log("  Proof of reachability (Z3):");
      console.log(`  \`\`\`smt2`);
      for (const smt2Line of v.smt2.split("\n")) {
        console.log(`  ${smt2Line}`);
      }
      console.log(`  \`\`\``);
      console.log();
      const escaped = v.smt2.replace(/'/g, "'\\''");
      console.log(`  Verify: echo '${escaped}' | z3 -in`);
      console.log();
    }
  }
}

// ---------------------------------------------------------------------------
// report — coverage summary
// ---------------------------------------------------------------------------

function runReport(args: string[]): void {
  const projectRoot = resolveProjectRoot(args);

  const store = new ContractStore(projectRoot);
  const contracts = store.getAll();

  if (contracts.length === 0) {
    console.log("No contracts found. Run 'provekit analyze' first.");
    process.exit(0);
  }

  const { PrincipleStore } = require("./principles");
  const principleStore = new PrincipleStore(projectRoot);
  const principleConfidence = new Map<string, string>();
  for (const p of principleStore.getAll()) {
    principleConfidence.set(p.id, p.confidence || "low");
  }

  const getConfidence = (v: any): string => {
    if (v.confidence) return v.confidence;
    if (v.principle) {
      const id = v.principle.replace(/[\[\]]/g, "").trim();
      return principleConfidence.get(id) || "low";
    }
    return "low";
  };

  let totalProven = 0;
  let totalViolations = 0;
  let totalUnverified = 0;
  let highConfidence = 0;
  let lowConfidence = 0;

  const byFile = new Map<string, { proven: number; violations: number; unverified: number; signals: number; high: number }>();

  for (const c of contracts) {
    totalProven += c.proven.length;
    totalViolations += c.violations.length;
    const isUnverified = c.proven.length === 0 && c.violations.length === 0;
    if (isUnverified) totalUnverified++;

    for (const v of c.violations) {
      if (getConfidence(v) === "high") highConfidence++;
      else lowConfidence++;
    }

    const key = c.file;
    const entry = byFile.get(key) || { proven: 0, violations: 0, unverified: 0, signals: 0, high: 0 };
    entry.proven += c.proven.length;
    entry.violations += c.violations.length;
    entry.high += c.violations.filter((v) => getConfidence(v) === "high").length;
    if (isUnverified) entry.unverified++;
    entry.signals++;
    byFile.set(key, entry);
  }

  const coveragePct = contracts.length > 0
    ? Math.round(((contracts.length - totalUnverified) / contracts.length) * 100)
    : 0;

  console.log(`provekit v${VERSION} — coverage report`);
  console.log("──────────────────────────────────────────");
  console.log(`Signals:       ${contracts.length}`);
  console.log(`  Proven:      ${totalProven}`);
  console.log(`  Violations:  ${totalViolations} (${highConfidence} high confidence, ${lowConfidence} structural)`);
  console.log(`  Unverified:  ${totalUnverified}`);
  console.log(`  Coverage:    ${coveragePct}% (${contracts.length - totalUnverified}/${contracts.length} signals have proofs)`);
  console.log();

  if (byFile.size > 0) {
    console.log("By file:");
    const sorted = [...byFile.entries()].sort((a, b) => b[1].violations - a[1].violations);
    for (const [file, counts] of sorted) {
      const relPath = file.length > 60 ? "..." + file.slice(-57) : file;
      const fileCoverage = counts.signals > 0 ? Math.round(((counts.signals - counts.unverified) / counts.signals) * 100) : 0;
      console.log(`  ${relPath.padEnd(50)} ${counts.proven} proven  ${counts.violations} violations  ${counts.unverified} unverified  ${fileCoverage}%`);
    }
  }
}

// ---------------------------------------------------------------------------
// hook — install/uninstall git hook
// ---------------------------------------------------------------------------

/**
 * @deprecated Migrated to src/workflows/hook.{ts,workflow.yaml}.
 * The meta-dispatcher will route `provekit hook` through the YAML
 * workflow once it lands; until then this imperative path is kept so
 * existing CLI invocations keep working.
 */
function runHook(args: string[]): void {
  const projectRoot = resolveProjectRoot(args);
  const installer = new HookInstaller(projectRoot);

  if (args.includes("--uninstall") || args.includes("--remove")) {
    const result = installer.uninstall();
    console.log(result.message);
  } else if (args.includes("--status")) {
    console.log(installer.isInstalled() ? "Hook installed" : "Hook not installed");
  } else {
    const result = installer.install();
    console.log(result.message);
    if (result.path) console.log(`  ${result.path}`);
  }
}

// ---------------------------------------------------------------------------
// invariants — list/verify/retire the per-codebase constraint store
// ---------------------------------------------------------------------------

async function runInvariants(args: string[]): Promise<void> {
  const sub = args[0] ?? "list";
  const rest = args.slice(1);
  const projectRoot = resolveProjectRoot(rest);

  // Lazy-load runtime modules so the rest of the CLI doesn't pay the
  // import cost when invariants commands aren't being used.
  const { readInvariants, retireInvariant } = await import("./fix/runtime/invariantStore.js");
  const { formatReport, exitCodeFor } = await import("./fix/runtime/verify.js");
  const { verifyAllCached } = await import("./fix/runtime/verifyCache.js");

  switch (sub) {
    case "list": {
      const invariants = readInvariants(projectRoot, { includeRetired: rest.includes("--all") });
      if (invariants.length === 0) {
        console.log("(no invariants in .provekit/invariants/)");
        return;
      }
      for (const inv of invariants) {
        const status = inv.retired ? "retired" : "active";
        console.log(`${inv.id}  ${status}  ${inv.smt.kind}  ${inv.callsite.filePath}:${inv.callsite.startLine}  ${inv.originatingBug.slice(0, 80)}`);
      }
      return;
    }

    case "paths": {
      // Diagnostic: enumerate dataflow paths to a given callsite or to
      // a stored invariant's callsite. Requires the substrate (.provekit
      // SQLite db) to exist. Useful for debugging the path enumerator
      // before step 4 (Z3 path checker) lands.
      const invariantId = rest[0];
      if (!invariantId) {
        console.error("usage: provekit invariants paths <invariantId> [--max-paths N]");
        process.exit(2);
      }
      const inv = readInvariants(projectRoot, { includeRetired: true })
        .find((i) => i.id === invariantId);
      if (!inv) {
        console.error(`invariant ${invariantId} not found`);
        process.exit(2);
      }
      const maxIdx = rest.indexOf("--max-paths");
      const maxPaths = maxIdx >= 0 && rest[maxIdx + 1] ? parseInt(rest[maxIdx + 1]!, 10) : 50;

      const { openSubstrateDb } = await import("./fix/runtime/substrate.js");
      const { pathsTo } = await import("./fix/runtime/pathEnumerator.js");
      const db = openSubstrateDb(projectRoot);
      if (!db) {
        console.error(".provekit/provekit.db not found — run `provekit analyze` first");
        process.exit(2);
      }

      // The invariant's callsite is recorded by file+line, not by node
      // id — node ids are content-addressable and can drift. For v1
      // diagnostic, we resolve the node id at query time via the
      // substrate's nodes table.
      const { resolveCallsiteNodeId } = await import("./fix/runtime/substrate.js");
      const nodeId = resolveCallsiteNodeId(db, inv.callsite.filePath, inv.callsite.startLine);
      if (!nodeId) {
        console.error(`could not resolve callsite ${inv.callsite.filePath}:${inv.callsite.startLine} in substrate`);
        process.exit(2);
      }

      const paths = pathsTo(db, nodeId, { maxPaths });
      console.log(`paths to ${inv.callsite.filePath}:${inv.callsite.startLine}:`);
      console.log(`  enumerated ${paths.length} path${paths.length === 1 ? "" : "s"}`);
      for (let i = 0; i < paths.length; i++) {
        console.log(`  path ${i + 1}: ${paths[i]!.steps.length} steps`);
        for (const step of paths[i]!.steps) {
          console.log(`    ${step.slot}: ${step.nodeId.slice(0, 16)}`);
        }
      }
      return;
    }

    case "verify": {
      const verbose = rest.includes("--verbose") || rest.includes("-v");
      const json = rest.includes("--json");
      const adversarial = rest.includes("--adversarial");
      const timeoutIdx = rest.indexOf("--timeout");
      const timeoutSeconds =
        timeoutIdx >= 0 && rest[timeoutIdx + 1]
          ? parseInt(rest[timeoutIdx + 1]!, 10)
          : undefined;
      const maxPathsIdx = rest.indexOf("--max-paths");
      const maxPathsArg =
        maxPathsIdx >= 0 && rest[maxPathsIdx + 1]
          ? parseInt(rest[maxPathsIdx + 1]!, 10)
          : undefined;
      // Adversarial default is more generous than the per-callsite
      // default (200 vs 50) per step-7 spec, but still bounded.
      const maxPaths = maxPathsArg ?? (adversarial ? 200 : undefined);

      try {
        // Step 7 cache decision: --adversarial bypasses verifyAllCached
        // and calls verifyAll directly. Reasoning: the cache fingerprint
        // currently folds in resolved binding hashes + substrate
        // identity, but adversarial mode adds a new dimension — the
        // sink's reverse-reachable set. Folding that in correctly means
        // re-resolving every upstream node's content hash on every
        // lookup, which dominates the cost the cache is meant to save.
        // v1 skips caching for adversarial; CI/manual ops aren't
        // pre-commit-budgeted anyway. Future work: a separate
        // adversarial-cache layer keyed on (invariant id, reverse-
        // reachable-set hash, substrate identity).
        let report: Awaited<ReturnType<typeof verifyAllCached>>;
        if (adversarial) {
          const { verifyAll } = await import("./fix/runtime/verify.js");
          const fresh = await verifyAll(projectRoot, {
            timeoutMs: timeoutSeconds !== undefined ? timeoutSeconds * 1000 : undefined,
            maxPaths,
            adversarial: true,
          });
          // Adapt the non-cached VerifyReport into the cached shape so
          // the rest of this branch (JSON output, formatReport, exit
          // code derivation) stays uniform. cacheHits/cacheMisses are
          // synthetic 0s — the adversarial summary line below explains.
          report = {
            verdicts: fresh.verdicts.map((v) => ({ ...v, cacheStatus: "miss" as const })),
            summary: {
              ...fresh.summary,
              cacheHits: 0,
              cacheMisses: fresh.verdicts.length,
            },
          };
        } else {
          report = await verifyAllCached(projectRoot, {
            timeoutMs: timeoutSeconds !== undefined ? timeoutSeconds * 1000 : undefined,
            maxPaths,
          });
        }

        if (json) {
          console.log(JSON.stringify(report, null, 2));
        } else {
          console.log(formatReport(report, { verbose }));
          // Surface the cache line at the bottom (spec section 7) for
          // non-adversarial runs; adversarial prints its own scope line.
          if (report.summary.total > 0 && !adversarial) {
            console.log(
              `cache: ${report.summary.cacheHits}/${report.summary.total} hit, ` +
              `${report.summary.cacheMisses} re-evaluated`,
            );
          } else if (adversarial && report.summary.total > 0) {
            const sinkScoped = report.verdicts.filter(
              (v) => v.invariant.scope === "sink",
            ).length;
            console.log(
              `adversarial scan: ${sinkScoped}/${report.summary.total} invariant${report.summary.total === 1 ? "" : "s"} are scope=sink (callsite-scoped invariants verified normally)`,
            );
          }
        }
        process.exit(exitCodeFor(report));
      } catch (err) {
        // Spec exit code 3: internal error (Z3 crashed, substrate
        // unreadable, etc.). Surface stderr so CI logs catch it.
        const msg = err instanceof Error ? err.message : String(err);
        console.error(`provekit verify: internal error: ${msg}`);
        if (err instanceof Error && err.stack) console.error(err.stack);
        process.exit(3);
      }
      return;
    }

    case "retire": {
      const id = rest[0];
      if (!id) {
        console.error("usage: provekit invariants retire <id> --reason \"<text>\"");
        process.exit(2);
      }
      const reasonIdx = rest.indexOf("--reason");
      const reason = reasonIdx >= 0 && rest[reasonIdx + 1] ? rest[reasonIdx + 1]! : "(no reason given)";
      const result = retireInvariant(projectRoot, id, reason);
      if (!result) {
        console.error(`invariant ${id} not found`);
        process.exit(2);
      }
      console.log(`retired ${id}: ${reason}`);
      return;
    }

    default:
      console.error(`unknown subcommand: ${sub}`);
      console.error("usage: provekit invariants {list|verify|retire <id> --reason \"<text>\"}");
      process.exit(2);
  }
}

// ---------------------------------------------------------------------------
// override — allow committing despite violations
// ---------------------------------------------------------------------------

async function runLint(args: string[]): Promise<void> {
  // Walk all .ts/.tsx files under projectRoot, build SAST, evaluate every
  // .dsl in .provekit/principles/, print principle_matches. The principle
  // library's "static analyzer" surface — what `analyze` doesn't do.
  const { mkdtempSync, mkdirSync, readdirSync, rmSync, writeFileSync } = await import("fs");
  const { tmpdir } = await import("os");
  const { migrate } = await import("drizzle-orm/better-sqlite3/migrator");
  const { evaluatePrinciple } = await import("./dsl/evaluator.js");
  const { buildSASTForFile } = await import("./sast/builder.js");

  const projectRoot = resolve(args.find((a) => !a.startsWith("-")) || ".");
  const localPrinciplesDir = join(projectRoot, ".provekit", "principles");
  // Fall back to the principles bundled with the provekit checkout. `provekit
  // init` doesn't seed principles into the target project (only the SAST
  // scaffolding), so a greenfield user runs lint against the package's
  // library out-of-the-box. They can copy/customize into their project's
  // .provekit/principles/ to override.
  const bundledPrinciplesDir = join(__dirname, "..", ".provekit", "principles");
  const principlesDir = existsSync(localPrinciplesDir) ? localPrinciplesDir : bundledPrinciplesDir;
  if (!existsSync(principlesDir)) {
    process.stderr.write(`No principles found at ${localPrinciplesDir} or ${bundledPrinciplesDir}.\n`);
    process.exit(1);
  }
  const usingBundled = principlesDir === bundledPrinciplesDir;
  const ci = args.includes("--ci");
  const verbose = args.includes("--verbose") || args.includes("-v");

  // Recursively walk the project for .ts/.tsx files. Skip node_modules,
  // dist, .git, .provekit, anything under test/ or __tests__.
  const tsFiles: string[] = [];
  const skip = new Set(["node_modules", "dist", ".git", ".provekit", "test", "tests", "__tests__"]);
  function walk(dir: string): void {
    let entries: string[];
    try { entries = readdirSync(dir, { withFileTypes: true }).map((e) => e.isDirectory() ? `${e.name}/` : e.name); }
    catch { return; }
    for (const name of entries) {
      const isDir = name.endsWith("/");
      const clean = isDir ? name.slice(0, -1) : name;
      if (skip.has(clean)) continue;
      const full = join(dir, clean);
      if (isDir) walk(full);
      else if (/\.(ts|tsx|mts|cts)$/.test(clean) && !/\.(test|spec)\.[^/]+$/.test(clean)) {
        tsFiles.push(full);
      }
    }
  }
  walk(projectRoot);

  if (tsFiles.length === 0) {
    console.log(`No .ts/.tsx files under ${projectRoot}.`);
    return;
  }

  // Open scratch DB, build SAST for every file, run every principle.
  const scratchDir = mkdtempSync(join(tmpdir(), "provekit-lint-"));
  void mkdirSync; // keep tsc happy if future versions need parent mkdir
  void writeFileSync;
  const dbPath = join(scratchDir, "scratch.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: join(__dirname, "..", "drizzle") });

  let parsedFiles = 0;
  let parserFailures = 0;
  for (const f of tsFiles) {
    try { buildSASTForFile(db, f); parsedFiles++; }
    catch (e) {
      parserFailures++;
      if (verbose) process.stderr.write(`parser failed on ${f}: ${e instanceof Error ? e.message : String(e)}\n`);
    }
  }

  // Partition-aware enumeration (task #134): walks universal/ + the
  // language partitions detected for projectRoot. The lint surface
  // wants every applicable principle, NOT just the flat root.
  const { enumeratePrincipleFiles } = await import("./principleEnumeration.js");
  const { dslPaths } = enumeratePrincipleFiles(principlesDir, { projectRoot });
  let principleErrors = 0;
  for (const dslPath of dslPaths) {
    const dslFile = relative(principlesDir, dslPath);
    let dsl: string;
    try { dsl = readFileSync(dslPath, "utf-8"); } catch { principleErrors++; continue; }
    try { evaluatePrinciple(db, dsl); }
    catch (e) {
      principleErrors++;
      if (verbose) process.stderr.write(`principle ${dslFile} failed: ${e instanceof Error ? e.message : String(e)}\n`);
    }
  }

  // Read matches + print them.
  const matches = db.$client
    .prepare(
      `SELECT pm.principle_name, pm.severity, pm.message, n.source_line, f.path
       FROM principle_matches pm
       JOIN nodes n ON n.id = pm.root_match_node_id
       JOIN files f ON f.id = pm.file_id
       ORDER BY f.path, n.source_line`,
    )
    .all() as Array<{ principle_name: string; severity: string; message: string; source_line: number; path: string }>;

  const violations = matches.filter((m) => m.severity === "violation");
  const warnings = matches.filter((m) => m.severity === "warning");
  const infos = matches.filter((m) => m.severity === "info");

  for (const m of matches) {
    const rel = relative(projectRoot, m.path);
    console.log(`${rel}:${m.source_line}  [${m.severity}]  ${m.principle_name}: ${m.message}`);
  }

  console.log();
  if (usingBundled) {
    console.log(`(Using bundled principles from ${principlesDir} — copy to ${localPrinciplesDir} to customize)`);
  }
  console.log(`Files indexed: ${parsedFiles}/${tsFiles.length}${parserFailures ? ` (${parserFailures} parser failures, run with -v to see)` : ""}`);
  console.log(`Principles evaluated: ${dslPaths.length - principleErrors}/${dslPaths.length}${principleErrors ? ` (${principleErrors} errors)` : ""}`);
  console.log(`Matches: ${matches.length} (${violations.length} violations, ${warnings.length} warnings, ${infos.length} info)`);

  db.$client.close();
  try { rmSync(scratchDir, { recursive: true, force: true }); } catch { /* ignore */ }

  // CI mode: exit non-zero if any violations.
  if (ci && violations.length > 0) process.exit(1);
}

/**
 * @deprecated Migrated to src/workflows/override.{ts,workflow.yaml}.
 * The meta-dispatcher will route `provekit override` through the YAML
 * workflow once it lands; until then this imperative path is kept so
 * existing CLI invocations keep working.
 */
function runOverride(args: string[]): void {
  const reason = getFlag(args, "--reason");
  if (!reason) {
    console.error("Usage: provekit override --reason \"why this is intentional\"");
    process.exit(1);
  }

  console.log(`Override recorded: ${reason}`);
  console.log("Run: git commit --no-verify");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function printSummary(result: { graph: any; derivation: any; report: any }): void {
  console.log("═══════════════════════════════════════════════════════════");
  console.log(`  ${result.graph.files.length} files | ${result.derivation.contracts.length} contracts`);
  console.log(`  Phase 3: ${result.derivation.contracts.reduce((n: number, c: any) => n + c.proven.length, 0)} proven | ${result.derivation.contracts.reduce((n: number, c: any) => n + c.violations.length, 0)} violations`);
  const consistencyResult = result.report.checkerResults.find((cr: any) => cr.checker === "consistency");
  const consistencyVerdict = consistencyResult?.results[0]?.verdict ?? "n/a";
  console.log(`  Phase 5: ${result.report.totalProven} proven | ${result.report.totalViolations} violations | ${consistencyVerdict} consistency`);
  console.log("═══════════════════════════════════════════════════════════");
}

async function fileIssues(result: { derivation: any }, dryRun: boolean): Promise<void> {
  const { collectViolationIssues, fileViolationIssues } = require("./issues");

  const fakeResults = result.derivation.contracts.map((c: any) => ({
    derivation: {
      callSite: { line: c.line, column: 0, logText: c.key, functionName: c.function },
      filePath: c.file,
    },
    verifications: [
      ...c.proven.map((p: any) => ({ smt2: p.smt2, z3Result: "unsat" as const, principle: p.principle })),
      ...c.violations.map((v: any) => ({ smt2: v.smt2, z3Result: "sat" as const, principle: v.principle })),
    ],
  }));

  const issues = collectViolationIssues(fakeResults);
  if (issues.length > 0) {
    console.log(`\n${dryRun ? "[DRY RUN] " : ""}Filing ${issues.length} issues...`);
    const filed = fileViolationIssues(issues, dryRun);
    console.log(`Issues: ${filed.filed} ${dryRun ? "previewed" : "filed"}, ${filed.skipped} skipped, ${filed.errors} errors`);
  }
}

function buildSignalRegistry(args: string[], model: string): SignalRegistry {
  if (args.includes("--llm-signals")) {
    const llmModel = getFlag(args, "--signal-model") ?? model;
    console.log(`LLM signal generator (model: ${llmModel})`);
    return SignalRegistry.createLLM({ model: llmModel });
  }

  if (args.includes("--rule-based")) {
    console.log("Rule-based signal generators (log, comment, function-name)");
    return SignalRegistry.createRuleBased();
  }

  console.log("AST signal generator");
  return SignalRegistry.createDefault();
}

function printHelp(): void {
  console.log(`provekit v${VERSION}`);
  console.log("The Kit to Prove It's Fixed.");
  console.log();
  console.log("Commands:");
  console.log("  init [project]              Bootstrap a project: seed principles, install git hook,");
  console.log("                              scaffold .github/workflows/provekit.yml.");
  console.log("                              Flags: --no-hook, --no-actions-workflow.");
  console.log("  invariants <sub>            Constraint store ops: list | verify | retire | paths.");
  console.log("                              `verify --ci` is the standing-invariant gate (Z3, no LLM).");
  console.log("  analyze <file.ts>           Full pipeline (phases 1-5)");
  console.log("  derive                      Analyze changed files only (diff-powered)");
  console.log("  verify [project]            Phase 5 only (no LLM, just Z3)");
  console.log("  diff [ref]                  Show proof changes since ref (default: HEAD~1)");
  console.log("  explain <file>:<line>        Show details for a finding");
  console.log("  report [project]            Coverage summary");
  console.log("  hook [--uninstall]          Install/remove git hook");
  console.log("  override --reason \"...\"      Record override for --no-verify");
  console.log("  lint [project]              Run the principle library across the project (Mode 1, no LLM).");
  console.log("                              Falls back to bundled principles when local .provekit/principles/ is empty.");
  console.log("                              Flags: --ci (exit 1 on violation), -v (verbose error surfacing).");
  console.log("  mine-history [project]      Walk git log; run B0 retrospective intake;");
  console.log("                              persist constraint-shaped intents to");
  console.log("                              .provekit/invariants/. Bootstrap-from-history.");
  console.log("                              Flags: --since <sha-or-date>, --max-commits N,");
  console.log("                              --dry-run (print would-mint counts + cost est).");
  console.log("  prove <intent>          Run the intent loop. Accepts any user text:");
  console.log("                          a bug report, a change request, or a property");
  console.log("                          assertion. The pipeline derives the property the");
  console.log("                          code should satisfy; if the verifier finds a");
  console.log("                          violation, generates a patch + locking test.");
  console.log("                          <intent> can be:");
  console.log("                            gap_report:<id>      — reference a gap_reports row");
  console.log("                            <file-path>          — path to an intent text file");
  console.log("                            gh:<number>          — GitHub issue shorthand");
  console.log("                            http(s)://...        — URL (v1: treated as text)");
  console.log("                            -                    — read from stdin");
  console.log("                            <plain text>         — intent text directly");
  console.log("  fix <ref>               Alias for `prove`. Legacy name.");
  console.log("                          Options:");
  console.log("                            --no-confirm         Skip the \"Proceed?\" prompt");
  console.log("                            --dry-run            Print the plan as JSON and exit");
  console.log("                            --apply              Cherry-pick fix onto target branch (autoApply mode)");
  console.log("                            --max-sites N        Override max complementary sites (default 10)");
  console.log();
  console.log("Options:");
  console.log("  --model <name>       LLM model (default: sonnet)");
  console.log("  --provider <name>    LLM provider (claude-agent, opencode, openai, openrouter, pool)");
  console.log("  --verbose, -v        Stream LLM reasoning");
  console.log("  --issues             File GitHub issues for violations");
  console.log("  --dry-run            Preview issues without filing");
  console.log("  --ci                 Exit 1 on violations (for CI pipelines)");
  console.log("  --since <ref>        Git ref to diff against (for derive)");
  console.log("  --no-hook            Skip hook installation during init");
  console.log("  --llm-signals        Use LLM for signal generation (slower, costs API calls)");
  console.log("  --rule-based         Use rule-based signal generators (log, comment, function-name)");
  console.log("  --signal-model <m>   Model for LLM signal generator (default: same as --model)");
}

function getFlag(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  return idx !== -1 && idx + 1 < args.length ? args[idx + 1] : undefined;
}

function resolveProjectRoot(args: string[]): string {
  const explicit = args.find((a) => !a.startsWith("-"));
  if (explicit) return resolve(explicit);
  return findProjectRoot(process.cwd());
}

function findProjectRoot(startDir: string): string {
  let dir = startDir;
  while (dir !== dirname(dir)) {
    for (const c of [".provekit", "package.json", ".git"]) {
      try {
        if (statSync(resolve(dir, c)).isDirectory() || statSync(resolve(dir, c)).isFile()) {
          return dir;
        }
      } catch { continue; }
    }
    dir = dirname(dir);
  }
  return startDir;
}

// ---------------------------------------------------------------------------
// explainGaps — render encoding-gap rows for a contract key
// ---------------------------------------------------------------------------

// Returns a string (not console.log'd) so the test can assert against it
// without touching stdout. The CLI handler writes it directly to stdout.

export function explainGaps(db: Db, contractKey: string): string {
  const rows = db
    .select({
      kind: gapReports.kind,
      smtConstant: gapReports.smtConstant,
      atNodeRef: gapReports.atNodeRef,
      explanation: gapReports.explanation,
      smtValueId: gapReports.smtValueId,
      runtimeValueId: gapReports.runtimeValueId,
    })
    .from(gapReports)
    .innerJoin(clauses, eq(clauses.id, gapReports.clauseId))
    .where(eq(clauses.contractKey, contractKey))
    .all();

  if (rows.length === 0) {
    return `No encoding gaps reported for ${contractKey}.\n`;
  }

  const lines: string[] = [];
  for (const row of rows) {
    const header = row.atNodeRef ? `encoding-gap at ${row.atNodeRef}` : `encoding-gap`;
    const constName = row.smtConstant ? ` — ${row.smtConstant}` : "";
    lines.push(`${header}${constName}`);

    if (row.smtValueId) {
      const smtVal = db
        .select()
        .from(runtimeValues)
        .where(eq(runtimeValues.id, row.smtValueId))
        .get();
      if (smtVal) lines.push(`  Z3 modeled:        ${formatRuntimeValue(smtVal)}`);
    }
    if (row.runtimeValueId) {
      const rtVal = db
        .select()
        .from(runtimeValues)
        .where(eq(runtimeValues.id, row.runtimeValueId))
        .get();
      if (rtVal) lines.push(`  Runtime returned:  ${formatRuntimeValue(rtVal)}`);
    }
    lines.push(`  Cause:             ${row.explanation ?? "(no explanation)"}`);
    lines.push(`  Kind:              ${row.kind}`);
    lines.push("");
  }

  return lines.join("\n");
}

function formatRuntimeValue(row: {
  kind: string;
  numberValue: number | null;
  stringValue: string | null;
  boolValue: boolean | null;
}): string {
  switch (row.kind) {
    case "number":      return String(row.numberValue);
    case "string":      return JSON.stringify(row.stringValue);
    case "bool":        return String(row.boolValue);
    case "nan":         return "NaN";
    case "infinity":    return "Infinity";
    case "neg_infinity": return "-Infinity";
    case "null":        return "null";
    case "undefined":   return "undefined";
    default:            return `<${row.kind}>`;
  }
}

if (require.main === module) {
  main().catch((err) => {
    console.error("Fatal:", err.message || err);
    process.exit(1);
  });
}
