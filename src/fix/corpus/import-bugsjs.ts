/**
 * BugsJS real-bug → CorpusScenario importer.
 *
 * BugsJS (https://bugsjs.github.io) is a curated dataset of 453 real bugs from
 * 10 mature Node.js projects (Express, Mocha, ESLint, Karma, Bower, Hexo,
 * Hessian.js, Node-redis, Pencilblue, Shields). Each bug ships with a fixed
 * snapshot, a buggy snapshot, the failing test, and metadata.
 *
 * This importer converts a local clone of github.com/BugsJS/bugs-data into a
 * set of CorpusScenario files under src/fix/corpus/scenarios/imported/bugsjs/.
 * Each BugsJS bug becomes one scenario.
 *
 * Networking is intentionally out of scope: the user clones the descriptor
 * repo locally and passes its path. If absent, the CLI prints instructions:
 *   git clone --depth 1 https://github.com/BugsJS/bugs-data ~/bugsjs-data
 *
 * Design notes
 * ============
 * - We do NOT try to git-show file contents from each project repo (that would
 *   require cloning Express, Mocha, ESLint, Karma, Bower, Hexo, Hessian.js,
 *   node-redis, pencilblue, shields — gigabytes total). We use only what is
 *   present in the descriptor JSON. If a descriptor includes inline file
 *   bodies / diffs / patches, we render them. Otherwise we synthesize a
 *   minimal one-file fixture whose content is a comment block reproducing the
 *   bug-report text — for an out-of-scope rejection the file content barely
 *   matters; the runner just needs *something* keyed at a path.
 *
 * - The BugsJS data layout is not uniform across projects. Some have one big
 *   bugs.json; others have per-bug JSON files; field names vary
 *   (revisionId / commitId / sha; failingTests / tests / testFailure;
 *   diff / patch / changedFiles). The parser is intentionally tolerant: it
 *   tries a list of fallback field names and bumps a `reason` counter when no
 *   shape matches, rather than throwing.
 *
 * - Filtering happens BEFORE write: we skip bugs whose fix touches more than
 *   `maxFilesPerBug` files OR any file longer than `maxLocPerFile` lines, so
 *   we get reasonable single/few-file fixtures.
 *
 * - All imported BugsJS scenarios default to `bugClass: "novel"` and
 *   `expected.outcome: "out_of_scope"`. The runner already handles novel
 *   classification cleanly (classify() returns out_of_scope, sweep records
 *   the result without a stage failure). If we later add principles that
 *   match a bug class — e.g., division-by-zero — we can re-classify in a
 *   follow-up. For Leak 4 hardening, "did we reject without crashing" is the
 *   metric we want.
 *
 * - Scenario IDs use the `bugsjs-` prefix (not `imported-`) so they land in a
 *   dedicated namespace and never collide with the SemGrep importer's output.
 */

import {
  readdirSync,
  readFileSync,
  statSync,
  existsSync,
  mkdirSync,
  writeFileSync,
} from "fs";
import { join, basename, extname, dirname } from "path";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export interface ImportBugsJsOptions {
  /** Path to a local clone of github.com/BugsJS/bugs-data. */
  dataDir: string;
  /** Where generated scenario .ts files should be written. */
  outDir: string;
  /** Optional cap so dev runs do not produce hundreds of scenarios. */
  maxBugs?: number;
  /** Skip bugs whose fix touches more than this many files. Default: 2. */
  maxFilesPerBug?: number;
  /** Skip bugs whose any patched file exceeds this many source lines. Default: 50. */
  maxLocPerFile?: number;
}

export interface ImportBugsJsSummary {
  written: number;
  skipped: number;
  reasons: Record<string, number>;
}

export function importBugsJsCorpus(opts: ImportBugsJsOptions): ImportBugsJsSummary {
  const reasons: Record<string, number> = {};
  let written = 0;
  let skipped = 0;
  const max = opts.maxBugs ?? Infinity;
  const maxFiles = opts.maxFilesPerBug ?? 2;
  const maxLoc = opts.maxLocPerFile ?? 50;

  if (!existsSync(opts.dataDir)) {
    return {
      written: 0,
      skipped: 0,
      reasons: { "dataDir not found": 1 },
    };
  }

  mkdirSync(opts.outDir, { recursive: true });

  const descriptors = collectBugDescriptors(opts.dataDir);
  if (descriptors.length === 0) {
    return {
      written: 0,
      skipped: 0,
      reasons: { "no bug descriptors found in dataDir": 1 },
    };
  }

  for (const desc of descriptors) {
    if (written >= max) break;
    try {
      const result = processBugDescriptor(desc, { outDir: opts.outDir, maxFiles, maxLoc });
      if (result.kind === "wrote") {
        written += 1;
      } else {
        skipped += 1;
        reasons[result.reason] = (reasons[result.reason] ?? 0) + 1;
      }
    } catch (err) {
      skipped += 1;
      const msg = err instanceof Error ? err.message : String(err);
      const key = `exception: ${msg.slice(0, 60)}`;
      reasons[key] = (reasons[key] ?? 0) + 1;
    }
  }

  return { written, skipped, reasons };
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/**
 * A "raw" bug descriptor. We don't know the upstream JSON shape exactly, so
 * we keep the parsed JSON as `unknown` and pull fields out defensively.
 */
interface BugDescriptor {
  /** Filesystem path the JSON came from (used for project name inference). */
  sourcePath: string;
  /** Inferred project name (e.g., "express", "mocha"). */
  project: string;
  /** Parsed JSON payload. */
  payload: unknown;
}

type ProcessResult =
  | { kind: "wrote"; scenarioId: string }
  | { kind: "skipped"; reason: string };

function collectBugDescriptors(dataDir: string): BugDescriptor[] {
  const out: BugDescriptor[] = [];
  walk(dataDir, (entry) => {
    if (!entry.endsWith(".json")) return;
    // Skip obvious non-bug artifacts.
    const lower = entry.toLowerCase();
    if (lower.endsWith("/package.json") || lower.endsWith("/package-lock.json")) return;
    if (lower.endsWith("/tsconfig.json")) return;
    if (lower.endsWith("/.eslintrc.json")) return;

    let text: string;
    try {
      text = readFileSync(entry, "utf8");
    } catch {
      return;
    }
    let payload: unknown;
    try {
      payload = JSON.parse(text);
    } catch {
      return;
    }

    const project = inferProject(entry, dataDir);

    if (Array.isArray(payload)) {
      // bugs.json that is a JSON array of bug objects.
      for (const item of payload) {
        if (item && typeof item === "object") {
          out.push({ sourcePath: entry, project, payload: item });
        }
      }
    } else if (
      payload &&
      typeof payload === "object" &&
      Array.isArray((payload as { bugs?: unknown }).bugs)
    ) {
      for (const item of (payload as { bugs: unknown[] }).bugs) {
        if (item && typeof item === "object") {
          out.push({ sourcePath: entry, project, payload: item });
        }
      }
    } else if (payload && typeof payload === "object") {
      // Per-bug JSON file.
      out.push({ sourcePath: entry, project, payload });
    }
  });
  return out;
}

function walk(dir: string, onFile: (path: string) => void): void {
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    if (name.startsWith(".")) continue;
    if (name === "node_modules") continue;
    const full = join(dir, name);
    let st;
    try {
      st = statSync(full);
    } catch {
      continue;
    }
    if (st.isDirectory()) {
      walk(full, onFile);
    } else if (st.isFile()) {
      onFile(full);
    }
  }
}

function inferProject(filePath: string, dataDir: string): string {
  // Derive project name from the path relative to dataDir.
  // Common BugsJS layouts:
  //   <dataDir>/<project>/bugs.json
  //   <dataDir>/<project>/<bugId>.json
  //   <dataDir>/data/<project>/...
  const rel = filePath.startsWith(dataDir) ? filePath.slice(dataDir.length) : filePath;
  const parts = rel.split(/[\\/]/).filter(Boolean);
  // Skip a leading "data" or "bugs" segment.
  const skip = new Set(["data", "bugs", "src"]);
  for (const part of parts) {
    if (skip.has(part)) continue;
    if (part.endsWith(".json")) break;
    return part;
  }
  return "unknown";
}

/** Pull a string field from an object, trying multiple field names. */
function pickString(obj: unknown, ...keys: string[]): string | null {
  if (!obj || typeof obj !== "object") return null;
  const rec = obj as Record<string, unknown>;
  for (const key of keys) {
    const v = rec[key];
    if (typeof v === "string" && v.trim() !== "") return v;
    if (typeof v === "number") return String(v);
  }
  return null;
}

/**
 * Pull an array of "changed file" entries from the descriptor. Returned shape:
 *   { path: string; lines?: number; content?: string; }[]
 *
 * BugsJS shapes we accommodate:
 *   - { changedFiles: ["a/b.js", "c/d.js"] }
 *   - { files: [{ path, content }] }
 *   - { patch: "diff --git ..." }   (we count lines from the diff)
 *   - { diff: "--- a/foo.js\n+++ b/foo.js\n..." }
 */
function extractFileList(payload: unknown): { path: string; lines?: number; content?: string }[] {
  if (!payload || typeof payload !== "object") return [];
  const rec = payload as Record<string, unknown>;

  // 1) Explicit changedFiles: string[] | { path }[]
  for (const key of ["changedFiles", "filesChanged", "modifiedFiles", "patchedFiles"]) {
    const v = rec[key];
    if (Array.isArray(v)) {
      const out: { path: string; lines?: number; content?: string }[] = [];
      for (const item of v) {
        if (typeof item === "string") {
          out.push({ path: item });
        } else if (item && typeof item === "object") {
          const irec = item as Record<string, unknown>;
          const p = pickString(irec, "path", "filename", "file", "name");
          if (p) {
            const linesV = irec.lines;
            const lines = typeof linesV === "number" ? linesV : undefined;
            const content = pickString(irec, "content", "source", "body") ?? undefined;
            out.push({ path: p, lines, content });
          }
        }
      }
      if (out.length > 0) return out;
    }
  }

  // 2) Generic `files`: object | array
  const files = rec.files;
  if (Array.isArray(files)) {
    const out: { path: string; lines?: number; content?: string }[] = [];
    for (const item of files) {
      if (typeof item === "string") {
        out.push({ path: item });
      } else if (item && typeof item === "object") {
        const irec = item as Record<string, unknown>;
        const p = pickString(irec, "path", "filename", "file", "name");
        if (p) {
          const content = pickString(irec, "content", "source", "body") ?? undefined;
          const lines = typeof irec.lines === "number" ? (irec.lines as number) : undefined;
          out.push({ path: p, lines, content });
        }
      }
    }
    if (out.length > 0) return out;
  } else if (files && typeof files === "object") {
    // Map of { "path/to/file.js": "...content..." }
    const out: { path: string; content: string }[] = [];
    for (const [p, v] of Object.entries(files as Record<string, unknown>)) {
      if (typeof v === "string") out.push({ path: p, content: v });
    }
    if (out.length > 0) return out;
  }

  // 3) Fall back to parsing a diff/patch.
  const patch = pickString(rec, "patch", "diff");
  if (patch) {
    return parseFilesFromPatch(patch);
  }

  return [];
}

/**
 * Parse a unified-diff "patch" string into a list of changed files with line
 * counts. Used as a fallback when the descriptor has no structured file list.
 */
function parseFilesFromPatch(patch: string): { path: string; lines: number }[] {
  const out: { path: string; lines: number }[] = [];
  const lines = patch.split(/\r?\n/);
  let currentPath: string | null = null;
  let currentLines = 0;
  for (const line of lines) {
    const m = line.match(/^diff --git a\/(\S+) b\/\S+/);
    if (m && m[1]) {
      if (currentPath) out.push({ path: currentPath, lines: currentLines });
      currentPath = m[1];
      currentLines = 0;
      continue;
    }
    const altMatch = !m && line.match(/^\+\+\+ b\/(\S+)/);
    if (altMatch && altMatch[1] && !currentPath) {
      currentPath = altMatch[1];
      currentLines = 0;
      continue;
    }
    if (currentPath && (line.startsWith("+") || line.startsWith("-")) && !line.startsWith("+++") && !line.startsWith("---")) {
      currentLines += 1;
    }
  }
  if (currentPath) out.push({ path: currentPath, lines: currentLines });
  return out;
}

/** Pull the failing-test description, if present. */
function extractFailingTests(payload: unknown): string {
  if (!payload || typeof payload !== "object") return "";
  const rec = payload as Record<string, unknown>;
  for (const key of ["failingTests", "tests", "testFailure", "failingTest", "testCase"]) {
    const v = rec[key];
    if (typeof v === "string" && v.trim() !== "") return v.slice(0, 400);
    if (Array.isArray(v)) {
      const strs = v
        .map((x) => (typeof x === "string" ? x : pickString(x, "name", "fullName", "title")))
        .filter(Boolean) as string[];
      if (strs.length > 0) return strs.join("; ").slice(0, 400);
    }
  }
  return "";
}

/** Sanitize an arbitrary string into a valid TS file basename / scenario id. */
function sanitizeId(raw: string): string {
  return raw.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 80);
}

/** Best-effort extraction of a stable bug ID from the descriptor. */
function extractBugId(payload: unknown, sourcePath: string): string {
  const rec = (payload && typeof payload === "object" ? payload : {}) as Record<string, unknown>;
  const candidates = [
    pickString(rec, "bugId", "id", "issueId", "number", "issue"),
    pickString(rec, "revisionId", "commitId", "sha", "commit"),
  ].filter((s): s is string => typeof s === "string");
  if (candidates.length > 0) return candidates[0]!.slice(0, 40);
  // Fall back to the file basename stripped of extension.
  return basename(sourcePath, extname(sourcePath));
}

function processBugDescriptor(
  desc: BugDescriptor,
  cfg: { outDir: string; maxFiles: number; maxLoc: number },
): ProcessResult {
  const { project, payload, sourcePath } = desc;

  // ── Filter on file count + line count ──────────────────────────────────────
  const files = extractFileList(payload);
  if (files.length === 0) {
    return { kind: "skipped", reason: "no changed-file metadata found" };
  }
  if (files.length > cfg.maxFiles) {
    return { kind: "skipped", reason: `fix touches > ${cfg.maxFiles} files` };
  }
  for (const f of files) {
    const len = typeof f.lines === "number" ? f.lines : (f.content ? f.content.split(/\r?\n/).length : 0);
    if (len > cfg.maxLoc) {
      return { kind: "skipped", reason: `file > ${cfg.maxLoc} lines` };
    }
  }

  const bugId = extractBugId(payload, sourcePath);
  const scenarioId = `bugsjs-${sanitizeId(project)}-${sanitizeId(bugId)}`;
  if (scenarioId.length < 10) {
    return { kind: "skipped", reason: "could not synthesize stable scenario id" };
  }

  // ── Build the bug-report text ──────────────────────────────────────────────
  const commitMsg =
    pickString(payload, "commitMessage", "message", "title", "summary") ??
    "(no commit message in descriptor)";
  const failingTests = extractFailingTests(payload);
  const classification = pickString(payload, "classification", "category", "type") ?? "";

  const bugReport =
    `Imported from BugsJS '${project}/${bugId}': ${commitMsg.slice(0, 240)}` +
    (classification ? ` [classification: ${classification}]` : "") +
    (failingTests ? ` Failing tests: ${failingTests}` : "") +
    ` (touches ${files.length} file${files.length === 1 ? "" : "s"})`;

  // ── Build scenario.files: prefer inline content; otherwise synthesize. ─────
  const scenarioFiles: Record<string, string> = {};
  // We always key the first listed path so the bug-report parser's
  // codeReferences resolves against it.
  const primaryPath = files[0]!.path;
  const primaryRel = normalizeRelPath(primaryPath);
  const primaryContent = files[0]!.content ?? synthesizeFixture(project, bugId, commitMsg, primaryRel);
  scenarioFiles[primaryRel] = primaryContent;
  // If a second file is present and inline, include it; otherwise omit.
  if (files.length === 2) {
    const f = files[1]!;
    if (f.content) {
      scenarioFiles[normalizeRelPath(f.path)] = f.content;
    }
  }

  const source = buildScenarioSource({
    scenarioId,
    project,
    bugId,
    primaryRel,
    files: scenarioFiles,
    bugReport,
  });

  const outPath = join(cfg.outDir, `${scenarioId}.ts`);
  writeFileSync(outPath, source, "utf8");
  return { kind: "wrote", scenarioId };
}

/**
 * Normalize a project-relative path into the corpus convention (always rooted
 * under `src/` so the SAST extractor walks a consistent shape, and the file
 * has a JS/TS extension that buildSASTForFile recognises).
 */
function normalizeRelPath(raw: string): string {
  let p = raw.replace(/^[\\/]+/, "");
  // Strip leading "a/" or "b/" from diff paths.
  if (p.startsWith("a/") || p.startsWith("b/")) p = p.slice(2);
  // Force a JS/TS extension so the SAST builder picks it up.
  const ext = extname(p).toLowerCase();
  const jsLike = [".js", ".mjs", ".cjs", ".ts", ".tsx", ".jsx"];
  if (!jsLike.includes(ext)) {
    p = `${p}.js`;
  }
  // Always reroot under src/.
  if (!p.startsWith("src/")) {
    p = `src/${basename(p)}`;
  }
  return p;
}

function synthesizeFixture(project: string, bugId: string, commitMsg: string, relPath: string): string {
  // Minimal one-file fixture. Content is irrelevant for an out-of-scope
  // rejection but must parse as JS/TS so the SAST builder doesn't blow up.
  const safeMsg = commitMsg.replace(/\*\//g, "* /").slice(0, 240);
  return (
    `/**\n` +
    ` * BugsJS fixture: ${project}/${bugId}\n` +
    ` * ${safeMsg}\n` +
    ` *\n` +
    ` * Synthesized stub. Real source unavailable in descriptor; this file\n` +
    ` * exists so the corpus runner can mount a project at ${relPath} and\n` +
    ` * exercise the intake/locate/classify path. Out-of-scope rejection is\n` +
    ` * the expected outcome.\n` +
    ` */\n` +
    `module.exports = { __bugsjs: ${JSON.stringify(`${project}/${bugId}`)} };\n`
  );
}

interface BuildArgs {
  scenarioId: string;
  project: string;
  bugId: string;
  primaryRel: string;
  files: Record<string, string>;
  bugReport: string;
}

function buildScenarioSource(args: BuildArgs): string {
  const { scenarioId, files, bugReport, primaryRel } = args;
  const idLiteral = JSON.stringify(scenarioId);
  const reportLiteral = JSON.stringify(bugReport);
  const primaryRelLiteral = JSON.stringify(primaryRel);

  const filesEntries = Object.entries(files)
    .map(([p, c]) => `    [${JSON.stringify(p)}]: ${JSON.stringify(c)},`)
    .join("\n");

  return `/**
 * IMPORTED FROM BugsJS (https://bugsjs.github.io).
 * Auto-generated by src/fix/corpus/import-bugsjs.ts.
 * Do not edit by hand: re-run the importer.
 */
import type { CorpusScenario } from "../../../scenarios.js";

export const scenario: CorpusScenario = {
  id: ${idLiteral},
  bugClass: "novel",
  files: {
${filesEntries}
  },
  bugReport: ${reportLiteral},
  expected: {
    completes: ["intake"],
    outcome: "out_of_scope",
  },
  llmResponses: [
    {
      matchPrompt: "bug-report parser",
      response: JSON.stringify({
        summary: ${JSON.stringify(bugReport.slice(0, 120))},
        failureDescription: ${JSON.stringify(bugReport.slice(0, 200))},
        fixHint: "imported BugsJS real bug, out of scope for current principle library",
        codeReferences: [{ file: ${primaryRelLiteral}, line: 1 }],
        bugClassHint: "novel",
      }),
    },
    {
      matchPrompt: "classifying a bug report",
      response: JSON.stringify({
        primaryLayer: "out_of_scope",
        secondaryLayers: [],
        artifacts: [],
        rationale: "Imported BugsJS real bug; no matching principle in current library.",
      }),
    },
  ],
};
`;
}
