#!/usr/bin/env node

import { statSync, readFileSync, existsSync } from "fs";
import { resolve, dirname, relative } from "path";
import { Pipeline } from "./pipeline";
import { SignalRegistry, LLMSignalGenerator } from "./signals";
import { DiffAnalyzer, HookInstaller, ProofDiff } from "./git";
import { ContractStore } from "./contracts";

const VERSION = "0.3.0";

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.length === 0 || args.includes("--help") || args.includes("-h")) {
    printHelp();
    process.exit(0);
  }

  if (args.includes("--version")) {
    console.log(`neurallog v${VERSION}`);
    process.exit(0);
  }

  const command = args[0]!;
  const rest = args.slice(1);

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
    default:
      console.error(`Unknown command: ${command}`);
      printHelp();
      process.exit(1);
  }
}

// ---------------------------------------------------------------------------
// init — scan, optionally analyze, install hook
// ---------------------------------------------------------------------------

async function runInit(args: string[]): Promise<void> {
  const projectRoot = resolveProjectRoot(args);
  const signalRegistry = SignalRegistry.createDefault();

  console.log(`neurallog v${VERSION}`);
  console.log(`Initializing in ${projectRoot}`);
  console.log();

  const diff = new DiffAnalyzer(projectRoot);
  if (!diff.isGitRepo()) {
    console.error("Not a git repository. neurallog requires git.");
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

  // Install hook
  const hookInstaller = new HookInstaller(projectRoot);
  if (!args.includes("--no-hook")) {
    const result = hookInstaller.install();
    console.log(`Git hook: ${result.message}`);
    if (result.path) console.log(`  ${result.path}`);
  }

  console.log();
  console.log("Next steps:");
  console.log("  neurallog analyze <file.ts>   Derive proofs for a file");
  console.log("  neurallog derive              Derive proofs for changed files");
  console.log("  neurallog verify              Run Z3 against cached proofs");
  console.log("  neurallog report              Show coverage summary");
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

  const signalRegistry = buildSignalRegistry(args, model);

  console.log(`neurallog v${VERSION}`);
  console.log(`File:    ${filePath}`);
  console.log(`Model:   ${model}`);
  console.log(`Signals: ${signalRegistry.getGeneratorNames().join(", ")}`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  const concurrency = parseInt(getFlag(args, "--concurrency") || "5", 10);

  const pipeline = new Pipeline();
  const result = await pipeline.runFull({
    entryFilePath: filePath,
    projectRoot,
    model,
    verbose,
    signalRegistry,
    maxConcurrency: concurrency,
  });

  printSummary(result);

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

  console.log(`neurallog v${VERSION} — derive (diff-powered)`);
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

    if (result.report.violations > 0) {
      process.exit(1);
    }
    process.exit(0);
  }

  console.log(`neurallog v${VERSION} — verify (Phase 5 only, no LLM, pure Z3)`);
  console.log(`Project: ${projectRoot}`);
  console.log();

  const pipeline = new Pipeline();
  const report = pipeline.runVerifyOnly(projectRoot, verbose);

  if (ci) {
    if (report.violations > 0) {
      console.log();
      console.log(`${report.violations} violation${report.violations === 1 ? "" : "s"} found.`);
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
  const ref = args.find((a) => !a.startsWith("-")) || "HEAD~1";

  console.log(`neurallog v${VERSION} — proof diff against ${ref}`);
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
    console.error("Usage: neurallog explain <file>:<line>");
    process.exit(1);
  }

  const [filePart, linePart] = target.split(":");
  if (!filePart || !linePart) {
    console.error("Usage: neurallog explain <file>:<line>");
    process.exit(1);
  }

  const filePath = resolve(filePart);
  const line = parseInt(linePart, 10);
  const projectRoot = findProjectRoot(dirname(filePath));

  const store = new ContractStore(projectRoot);
  const contracts = store.getAll();

  const relPath = relative(projectRoot, filePath);
  const contract = contracts.find(
    (c) => (c.file === filePath || c.file === relPath) && c.line === line
  ) || contracts.find(
    (c) => (c.file === filePath || c.file === relPath) && Math.abs(c.line - line) <= 2
  );

  if (!contract) {
    console.error(`No contract found for ${relPath}:${line}`);
    console.error("Run 'neurallog analyze' first.");
    process.exit(1);
  }

  console.log();
  console.log("┌─────────────────────────────────────────────────────┐");
  console.log(`│  ${contract.function}:${contract.line}`);
  console.log(`│  File: ${contract.file}`);
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
    console.log("No contracts found. Run 'neurallog analyze' first.");
    process.exit(0);
  }

  let totalProven = 0;
  let totalViolations = 0;

  const byFile = new Map<string, { proven: number; violations: number }>();

  for (const c of contracts) {
    totalProven += c.proven.length;
    totalViolations += c.violations.length;

    const key = c.file;
    const entry = byFile.get(key) || { proven: 0, violations: 0 };
    entry.proven += c.proven.length;
    entry.violations += c.violations.length;
    byFile.set(key, entry);
  }

  console.log(`neurallog v${VERSION} — coverage report`);
  console.log("──────────────────────────────────────────");
  console.log(`Contracts:     ${contracts.length}`);
  console.log(`  Proven:      ${totalProven}`);
  console.log(`  Violations:  ${totalViolations}`);
  console.log();

  if (byFile.size > 0) {
    console.log("By file:");
    const sorted = [...byFile.entries()].sort((a, b) => b[1].violations - a[1].violations);
    for (const [file, counts] of sorted) {
      const relPath = file.length > 60 ? "..." + file.slice(-57) : file;
      console.log(`  ${relPath.padEnd(60)} ${counts.proven} proven, ${counts.violations} violations`);
    }
  }
}

// ---------------------------------------------------------------------------
// hook — install/uninstall git hook
// ---------------------------------------------------------------------------

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
// override — allow committing despite violations
// ---------------------------------------------------------------------------

function runOverride(args: string[]): void {
  const reason = getFlag(args, "--reason");
  if (!reason) {
    console.error("Usage: neurallog override --reason \"why this is intentional\"");
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
  console.log(`  Phase 5: ${result.report.proven} proven | ${result.report.violations} violations | ${result.report.consistency} consistency`);
  console.log("═══════════════════════════════════════════════════════════");
}

async function fileIssues(result: { derivation: any }, dryRun: boolean): Promise<void> {
  const { collectViolationIssues, fileViolationIssues } = require("./issues");

  const fakeResults = result.derivation.contracts.map((c: any) => ({
    derivation: {
      callSite: { line: c.line, column: 0, logText: "", functionName: c.function, functionSource: "", functionStartLine: c.line, functionEndLine: c.line },
      filePath: c.file,
      rawResponse: "",
    },
    verifications: [
      ...c.proven.map((p: any) => ({ smt2: p.smt2, z3Result: "unsat" as const, principle: p.principle, error: undefined })),
      ...c.violations.map((v: any) => ({ smt2: v.smt2, z3Result: "sat" as const, principle: v.principle, error: undefined })),
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
  const registry = SignalRegistry.createDefault();

  if (args.includes("--llm-signals")) {
    const llmModel = getFlag(args, "--signal-model") || model;
    console.log(`LLM signal generator enabled (model: ${llmModel})`);
    registry.register(new LLMSignalGenerator({ model: llmModel }));
  }

  return registry;
}

function printHelp(): void {
  console.log(`neurallog v${VERSION} — a logger that fixes your code`);
  console.log();
  console.log("Commands:");
  console.log("  init [project]              Scan codebase, install git hook");
  console.log("  analyze <file.ts>           Full pipeline (phases 1-5)");
  console.log("  derive                      Analyze changed files only (diff-powered)");
  console.log("  verify [project]            Phase 5 only (no LLM, just Z3)");
  console.log("  diff [ref]                  Show proof changes since ref (default: HEAD~1)");
  console.log("  explain <file>:<line>        Show details for a finding");
  console.log("  report [project]            Coverage summary");
  console.log("  hook [--uninstall]          Install/remove git hook");
  console.log("  override --reason \"...\"      Record override for --no-verify");
  console.log();
  console.log("Options:");
  console.log("  --model <name>       LLM model (default: sonnet)");
  console.log("  --verbose, -v        Stream LLM reasoning");
  console.log("  --issues             File GitHub issues for violations");
  console.log("  --dry-run            Preview issues without filing");
  console.log("  --ci                 Exit 1 on violations (for CI pipelines)");
  console.log("  --since <ref>        Git ref to diff against (for derive)");
  console.log("  --no-hook            Skip hook installation during init");
  console.log("  --llm-signals        Enable LLM signal generator (finds invariant points beyond logs)");
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
    for (const c of [".neurallog", "package.json", ".git"]) {
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

main().catch((err) => {
  console.error("Fatal:", err.message || err);
  process.exit(1);
});
