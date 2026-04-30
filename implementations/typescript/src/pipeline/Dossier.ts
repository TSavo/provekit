import { execFileSync } from "child_process";
import { readFileSync, existsSync } from "fs";
import { join, relative } from "path";
import { CallSiteContext } from "./ContextPhase";
import { Contract, ContractStore } from "../contracts";

export interface FunctionDossier {
  priorContract: string;
  diff: string;
  callerContracts: string;
  blame: string;
  runtimeWitnesses: string;
  todos: string;
  siblingFunctions: string;
  returnTypeAnalysis: string;
  testCoverage: string;
  tsconfigContext: string;
  errorHandling: string;
}

export function assembleDossier(
  signals: CallSiteContext[],
  filePath: string,
  projectRoot: string,
  store: ContractStore
): FunctionDossier {
  const representative = signals[0];
  if (!representative) return { priorContract: "(no signals)", diff: "", callerContracts: "", blame: "", runtimeWitnesses: "", todos: "", siblingFunctions: "", returnTypeAnalysis: "", testCoverage: "", tsconfigContext: "", errorHandling: "" };
  const relPath = relative(projectRoot, filePath);
  const fnName = representative.functionName;

  return {
    priorContract: buildPriorContract(signals, relPath, store),
    diff: buildDiff(filePath, fnName, projectRoot),
    callerContracts: buildCallerContracts(signals, store),
    blame: buildBlame(filePath, representative, projectRoot),
    runtimeWitnesses: buildRuntimeWitnesses(signals, relPath, store),
    todos: buildTodos(representative),
    siblingFunctions: buildSiblingFunctions(representative),
    returnTypeAnalysis: buildReturnTypeAnalysis(representative),
    testCoverage: buildTestCoverage(filePath, fnName, projectRoot),
    tsconfigContext: buildTsconfigContext(projectRoot),
    errorHandling: buildErrorHandling(representative),
  };
}

export function formatDossier(dossier: FunctionDossier): string {
  const sections: string[] = [];

  if (dossier.priorContract !== "(none)") {
    sections.push(`#### Prior verification (stale — code has changed since these were derived)\n${dossier.priorContract}`);
  }

  if (dossier.diff !== "(no diff)") {
    sections.push(`#### What changed (git diff)\n\`\`\`diff\n${dossier.diff}\n\`\`\``);
  }

  if (dossier.callerContracts !== "(none)") {
    sections.push(`#### Caller contracts (functions that call this one)\n${dossier.callerContracts}`);
  }

  if (dossier.blame !== "(unavailable)") {
    sections.push(`#### Git blame (who wrote this, when, why)\n${dossier.blame}`);
  }

  if (dossier.runtimeWitnesses !== "(none)") {
    sections.push(`#### Runtime evidence\n${dossier.runtimeWitnesses}`);
  }

  if (dossier.todos !== "(none)") {
    sections.push(`#### Known issues (TODOs/FIXMEs in this function)\n${dossier.todos}`);
  }

  if (dossier.siblingFunctions !== "(none)") {
    sections.push(`#### Sibling functions (same file, may share state)\n${dossier.siblingFunctions}`);
  }

  if (dossier.returnTypeAnalysis !== "(none)") {
    sections.push(`#### Return type analysis\n${dossier.returnTypeAnalysis}`);
  }

  if (dossier.testCoverage !== "(none)") {
    sections.push(`#### Test coverage\n${dossier.testCoverage}`);
  }

  if (dossier.tsconfigContext !== "(none)") {
    sections.push(`#### TypeScript strictness\n${dossier.tsconfigContext}`);
  }

  if (dossier.errorHandling !== "(none)") {
    sections.push(`#### Error handling patterns\n${dossier.errorHandling}`);
  }

  return sections.length > 0
    ? "### Function Dossier\n\n" + sections.join("\n\n")
    : "";
}

function buildPriorContract(signals: CallSiteContext[], relPath: string, store: ContractStore): string {
  const priors: string[] = [];
  for (const s of signals) {
    const key = `${relPath}/${s.functionName}[${s.line}]`;
    const contract = store.get(key);
    if (!contract) continue;

    const lines: string[] = [`Signal ${s.functionName}:${s.line}:`];
    for (const p of contract.proven) {
      lines.push(`  PROVEN [${p.principle || "?"}]: ${p.claim}`);
    }
    for (const v of contract.violations) {
      lines.push(`  VIOLATION [${v.principle || "?"}]: ${v.claim}`);
    }
    if (contract.proven.length === 0 && contract.violations.length === 0) {
      lines.push("  (no proofs derived — this was an unverified signal)");
    }
    priors.push(lines.join("\n"));
  }
  return priors.length > 0 ? priors.join("\n\n") : "(none)";
}

function buildDiff(filePath: string, fnName: string, projectRoot: string): string {
  try {
    const diff = execFileSync("git", ["diff", "HEAD", "--", filePath], {
      cwd: projectRoot,
      encoding: "utf-8",
      timeout: 5000,
      stdio: ["pipe", "pipe", "pipe"],
    }).trim();

    if (!diff) return "(no diff)";

    const lines = diff.split("\n");
    const relevant = lines.filter((l) =>
      l.startsWith("+") || l.startsWith("-") || l.startsWith("@@")
    );

    return relevant.length > 0 ? relevant.slice(0, 50).join("\n") : "(no diff)";
  } catch {
    return "(no diff)";
  }
}

function buildCallerContracts(signals: CallSiteContext[], store: ContractStore): string {
  const callerNames = new Set<string>();
  for (const s of signals) {
    for (const caller of s.calledBy) {
      callerNames.add(caller);
    }
  }

  if (callerNames.size === 0) return "(none)";

  const sections: string[] = [];
  const allContracts = store.getAll();

  for (const callerName of callerNames) {
    const callerContracts = allContracts.filter((c) => c.function === callerName);
    for (const c of callerContracts) {
      const lines: string[] = [`${c.key}:`];
      for (const p of c.proven) {
        lines.push(`  PROVEN [${p.principle || "?"}]: ${p.claim}`);
      }
      for (const v of c.violations) {
        lines.push(`  VIOLATION [${v.principle || "?"}]: ${v.claim}`);
      }
      if (lines.length > 1) sections.push(lines.join("\n"));
    }
  }

  return sections.length > 0 ? sections.join("\n\n") : "(none)";
}

function buildBlame(filePath: string, representative: CallSiteContext, projectRoot: string): string {
  try {
    const fnLines = representative.functionSource.split("\n").length;
    const startLine = representative.line;
    const endLine = representative.line + fnLines;

    const rawBlame = execFileSync("git", [
      "blame", "-L", `${startLine},${endLine}`, "--porcelain", filePath,
    ], { cwd: projectRoot, encoding: "utf-8", timeout: 5000, stdio: ["pipe", "pipe", "pipe"] });
    const blame = rawBlame.split("\n")
      .filter((l) => l.startsWith("author ") || l.startsWith("author-time ") || l.startsWith("summary "))
      .slice(0, 9)
      .join("\n")
      .trim();

    if (!blame) return "(unavailable)";

    const authors = new Set<string>();
    const summaries = new Set<string>();
    for (const line of blame.split("\n")) {
      if (line.startsWith("author ")) authors.add(line.slice(7));
      if (line.startsWith("summary ")) summaries.add(line.slice(8));
    }

    const parts: string[] = [];
    if (authors.size > 0) parts.push(`Authors: ${[...authors].join(", ")}`);
    if (summaries.size > 0) parts.push(`Commits: ${[...summaries].join("; ")}`);
    return parts.join("\n") || "(unavailable)";
  } catch {
    return "(unavailable)";
  }
}

function buildRuntimeWitnesses(signals: CallSiteContext[], relPath: string, store: ContractStore): string {
  const witnesses: string[] = [];
  for (const s of signals) {
    const key = `${relPath}/${s.functionName}[${s.line}]`;
    const contract = store.get(key);
    if (!contract) continue;

    for (const h of contract.clause_history) {
      if (h.current_witness_count > 0) {
        witnesses.push(`${s.functionName}:${s.line} — ${h.current_witness_count} runtime witnesses (${h.status})`);
      }
    }
  }
  return witnesses.length > 0 ? witnesses.join("\n") : "(none)";
}

function buildTodos(representative: CallSiteContext): string {
  const fnSource = representative.functionSource;
  const todoRegex = /\/\/\s*(TODO|FIXME|HACK|XXX|BUG|WARNING)\b[:\s]*(.*)/gi;
  const todos: string[] = [];
  let match;
  while ((match = todoRegex.exec(fnSource)) !== null) {
    todos.push(`${match[1]}: ${match[2].trim()}`);
  }
  return todos.length > 0 ? todos.join("\n") : "(none)";
}

function buildSiblingFunctions(representative: CallSiteContext): string {
  const fileSource = representative.fileSource;
  const fnRegex = /(?:export\s+)?(?:async\s+)?function\s+(\w+)/g;
  const siblings: string[] = [];
  let match;
  while ((match = fnRegex.exec(fileSource)) !== null) {
    if (match[1] !== representative.functionName) {
      siblings.push(match[1]!);
    }
  }
  return siblings.length > 0 ? `Functions in same file: ${siblings.join(", ")}` : "(none)";
}

function buildReturnTypeAnalysis(representative: CallSiteContext): string {
  const parts: string[] = [];

  const typeCtx = representative.typeContext || "";
  const returnMatch = typeCtx.match(/Return type:\s*(.+)/);
  if (returnMatch && returnMatch[1] !== "unknown") {
    parts.push(`Declared return type: ${returnMatch[1]}`);
  }

  const fnSource = representative.functionSource;
  const returnStatements = fnSource.match(/return\s+[^;]+/g) || [];
  if (returnStatements.length > 0) {
    parts.push(`Return statements: ${returnStatements.length}`);
    for (const ret of returnStatements.slice(0, 5)) {
      parts.push(`  ${ret.trim().slice(0, 80)}`);
    }
  }

  const hasImplicitReturn = !fnSource.includes("return") && !returnMatch;
  if (hasImplicitReturn) {
    parts.push("No explicit return — implicitly returns undefined");
  }

  return parts.length > 0 ? parts.join("\n") : "(none)";
}

function buildTestCoverage(filePath: string, fnName: string, projectRoot: string): string {
  const testPatterns = [
    filePath.replace(/\.ts$/, ".test.ts"),
    filePath.replace(/\.ts$/, ".spec.ts"),
    filePath.replace(/src\//, "test/").replace(/\.ts$/, ".test.ts"),
    filePath.replace(/src\//, "__tests__/").replace(/\.ts$/, ".test.ts"),
  ];

  const found: string[] = [];
  for (const pattern of testPatterns) {
    const full = join(projectRoot, pattern);
    if (existsSync(full)) {
      const testSource = readFileSync(full, "utf-8");
      const mentions = (testSource.match(new RegExp(`\\b${fnName}\\b`, "g")) || []).length;
      if (mentions > 0) {
        found.push(`${relative(projectRoot, full)}: ${mentions} references to ${fnName}`);
      } else {
        found.push(`${relative(projectRoot, full)}: exists but does not reference ${fnName}`);
      }
    }
  }

  if (found.length === 0) {
    return `No test file found. ${fnName} has no test coverage.`;
  }
  return found.join("\n");
}

function buildTsconfigContext(projectRoot: string): string {
  try {
    const tsconfig = JSON.parse(readFileSync(join(projectRoot, "tsconfig.json"), "utf-8"));
    const opts = tsconfig.compilerOptions || {};
    const parts: string[] = [];
    if (opts.strict) parts.push("strict: true (all strict checks enabled)");
    if (opts.strictNullChecks) parts.push("strictNullChecks: true");
    if (opts.noImplicitAny) parts.push("noImplicitAny: true");
    if (opts.noImplicitReturns) parts.push("noImplicitReturns: true");
    if (opts.noUncheckedIndexedAccess) parts.push("noUncheckedIndexedAccess: true");
    if (opts.target) parts.push(`target: ${opts.target}`);
    return parts.length > 0 ? parts.join("\n") : "(none)";
  } catch {
    return "(none)";
  }
}

function buildErrorHandling(representative: CallSiteContext): string {
  const fnSource = representative.functionSource;
  const parts: string[] = [];

  const tryCatches = (fnSource.match(/try\s*\{/g) || []).length;
  if (tryCatches > 0) parts.push(`${tryCatches} try/catch block(s)`);

  const emptyCatches = (fnSource.match(/catch\s*(?:\([^)]*\))?\s*\{\s*\}/g) || []).length;
  if (emptyCatches > 0) parts.push(`${emptyCatches} EMPTY catch block(s) — errors silently swallowed`);

  const catchContinue = (fnSource.match(/catch\s*(?:\([^)]*\))?\s*\{\s*continue/g) || []).length;
  if (catchContinue > 0) parts.push(`${catchContinue} catch-and-continue pattern(s)`);

  const throwStatements = (fnSource.match(/throw\s+/g) || []).length;
  if (throwStatements > 0) parts.push(`${throwStatements} throw statement(s)`);

  const nullChecks = (fnSource.match(/!==?\s*null|!==?\s*undefined|\?\./g) || []).length;
  if (nullChecks > 0) parts.push(`${nullChecks} null/undefined check(s)`);

  return parts.length > 0 ? parts.join("\n") : "(none)";
}
