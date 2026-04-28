/**
 * Contract + reference-implementation tests for the channel-2 interfaces.
 *
 * Each test exercises ONE reference class against the shape defined in
 * src/integration/interfaces.ts. The tests double as the contract suite an
 * alternative implementation must pass: if a future implementation of the
 * same interface lands, point the same tests at it and they should pass.
 *
 * Tests avoid filesystem + network dependencies wherever possible:
 *   - Parser: in-memory ts-morph, no DB
 *   - Substrate: in-memory better-sqlite3 with a pre-migrated schema
 *   - Sandbox: real git worktree against a tempdir-backed repo
 *   - Scanner: pure-string regex
 *   - Applicator: writes to a sandbox tree
 *
 * BuildTestRunner is verified via shape only — running vitest inside vitest
 * is the same anti-pattern that makes the existing testRunners tests use
 * an injected runner. The shape verification confirms the wrapper resolves
 * a TestRunnerDescriptor and passes the result through.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import { execFileSync } from "child_process";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import {
  ReferenceTsMorphParser,
  ReferenceSqliteSubstrate,
  ReferenceGitWorktreeSandbox,
  ReferenceVitestPnpmRunner,
  ReferenceTypeScriptPatternScanner,
  ReferenceGitApplyApplicator,
  createReferenceImplementations,
} from "./index.js";
import { openDb } from "../../db/index.js";
import { fileURLToPath } from "url";
import { dirname } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

// ---------------------------------------------------------------------------
// Parser contract
// ---------------------------------------------------------------------------

describe("ReferenceTsMorphParser (Parser contract)", () => {
  const parser = new ReferenceTsMorphParser();

  it("exposes a name", () => {
    expect(parser.name).toBe("ts-morph");
  });

  it("parses a trivial source file", () => {
    const graph = parser.parse("export const x = 1;\n", "/virtual/x.ts");
    expect(graph.filePath).toBe("/virtual/x.ts");
    expect(graph.nodes.length).toBeGreaterThan(0);
    expect(graph.rootId).toBe(graph.nodes[0].id);
  });

  it("emits parent-child edges that reference real node ids", () => {
    const graph = parser.parse("function f() { return 42; }\n", "/virtual/f.ts");
    const ids = new Set(graph.nodes.map((n) => n.id));
    for (const edge of graph.edges) {
      expect(ids.has(edge.parentId)).toBe(true);
      expect(ids.has(edge.childId)).toBe(true);
    }
  });

  it("attaches line + column positions", () => {
    const src = "const x = 1;\nconst y = 2;\n";
    const graph = parser.parse(src, "/virtual/y.ts");
    for (const node of graph.nodes) {
      expect(node.line).toBeGreaterThanOrEqual(1);
      expect(node.column).toBeGreaterThanOrEqual(0);
      expect(node.end).toBeGreaterThanOrEqual(node.start);
    }
  });

  it("handles syntax errors gracefully (still parses, error nodes appear)", () => {
    const graph = parser.parse("function (", "/virtual/bad.ts");
    expect(graph.nodes.length).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// Substrate contract
// ---------------------------------------------------------------------------

describe("ReferenceSqliteSubstrate (Substrate contract)", () => {
  let workdir: string;
  let dbPath: string;

  beforeEach(() => {
    workdir = mkdtempSync(join(tmpdir(), "provekit-substrate-test-"));
    dbPath = join(workdir, "test.db");
  });

  afterEach(() => {
    rmSync(workdir, { recursive: true, force: true });
  });

  it("ingests a file via buildSASTForFile and exposes node count", () => {
    const filePath = join(workdir, "sample.ts");
    writeFileSync(filePath, "export const x = 42;\n");
    const db = openDb(dbPath);
    migrate(db, { migrationsFolder: DRIZZLE_FOLDER });

    const sub = new ReferenceSqliteSubstrate(db);
    const result = sub.ingestFile(filePath);
    expect(result.nodeCount).toBeGreaterThan(0);
    expect(result.fileId).toBeGreaterThan(0);
    expect(result.rootNodeId).toBeTruthy();
  });

  it("query() returns a node id when one covers the position", () => {
    const filePath = join(workdir, "pos.ts");
    writeFileSync(filePath, "const a = 1;\nconst b = 2;\n");
    const db = openDb(dbPath);
    migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
    const sub = new ReferenceSqliteSubstrate(db);
    sub.ingestFile(filePath);
    const id = sub.query({ filePath, line: 1 });
    expect(id).not.toBeNull();
  });

  it("query() returns null for a nonexistent file path", () => {
    const db = openDb(dbPath);
    migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
    const sub = new ReferenceSqliteSubstrate(db);
    const id = sub.query({ filePath: "/no/such/file.ts", line: 1 });
    expect(id).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Sandbox contract
// ---------------------------------------------------------------------------

describe("ReferenceGitWorktreeSandbox (Sandbox contract)", () => {
  let repoRoot: string;

  beforeEach(() => {
    repoRoot = mkdtempSync(join(tmpdir(), "provekit-sandbox-test-"));
    execFileSync("git", ["init", "--initial-branch=main"], { cwd: repoRoot, stdio: "pipe" });
    execFileSync("git", ["config", "user.email", "t@t"], { cwd: repoRoot, stdio: "pipe" });
    execFileSync("git", ["config", "user.name", "t"], { cwd: repoRoot, stdio: "pipe" });
    writeFileSync(join(repoRoot, "README.md"), "hello\n");
    writeFileSync(join(repoRoot, "package.json"), '{"name":"sb-test","version":"0.0.0"}\n');
    writeFileSync(join(repoRoot, "main.ts"), "export const x = 1;\n");
    execFileSync("git", ["add", "."], { cwd: repoRoot, stdio: "pipe" });
    execFileSync("git", ["commit", "-m", "init"], { cwd: repoRoot, stdio: "pipe" });
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("opens a sandbox handle on a git repo", async () => {
    const sandbox = new ReferenceGitWorktreeSandbox();
    const handle = await sandbox.open(repoRoot, { locusFile: join(repoRoot, "main.ts") });
    expect(handle.worktreePath).toBeTruthy();
    expect(existsSync(handle.worktreePath)).toBe(true);
    expect(handle.closed).toBe(false);
    await sandbox.close(handle);
  });

  it("apply() writes file edits into the worktree", async () => {
    const sandbox = new ReferenceGitWorktreeSandbox();
    const handle = await sandbox.open(repoRoot, { locusFile: join(repoRoot, "main.ts") });
    await sandbox.apply(handle, {
      fileEdits: [{ file: "main.ts", newContent: "export const x = 99;\n" }],
      description: "test",
    });
    const written = readFileSync(join(handle.worktreePath, "main.ts"), "utf-8");
    expect(written).toBe("export const x = 99;\n");
    await sandbox.close(handle);
  });

  it("runCommand() returns RunResult with exit code", async () => {
    const sandbox = new ReferenceGitWorktreeSandbox();
    const handle = await sandbox.open(repoRoot, { locusFile: join(repoRoot, "main.ts") });
    const r = sandbox.runCommand(handle, ["true"]);
    expect(r.exitCode).toBe(0);
    const r2 = sandbox.runCommand(handle, ["false"]);
    expect(r2.exitCode).not.toBe(0);
    await sandbox.close(handle);
  });

  it("close() removes the worktree", async () => {
    const sandbox = new ReferenceGitWorktreeSandbox();
    const handle = await sandbox.open(repoRoot, { locusFile: join(repoRoot, "main.ts") });
    const wt = handle.worktreePath;
    expect(existsSync(wt)).toBe(true);
    await sandbox.close(handle);
    expect(existsSync(wt)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// SourcePatternScanner contract
// ---------------------------------------------------------------------------

describe("ReferenceTypeScriptPatternScanner (SourcePatternScanner contract)", () => {
  const scanner = new ReferenceTypeScriptPatternScanner();

  it("matches asc-or-desc when desc is present", () => {
    const ctx = { filePath: "x.ts", source: "orderBy(desc(table.col))", line: 1 };
    expect(scanner.scan(ctx, { kind: "order/asc-or-desc" })).toBe(true);
  });

  it("matches asc-or-desc when asc is present", () => {
    const ctx = { filePath: "x.ts", source: "orderBy(asc(table.col))", line: 1 };
    expect(scanner.scan(ctx, { kind: "order/asc-or-desc" })).toBe(true);
  });

  it("returns false when neither asc nor desc is present", () => {
    const ctx = { filePath: "x.ts", source: "table.findMany()", line: 1 };
    expect(scanner.scan(ctx, { kind: "order/asc-or-desc" })).toBe(false);
  });

  it("order/asc returns false when both asc and desc appear in the window", () => {
    const ctx = {
      filePath: "x.ts",
      source: "orderBy(asc(a))\norderBy(desc(b))",
      line: 1,
      windowLines: 2,
    };
    expect(scanner.scan(ctx, { kind: "order/asc" })).toBe(false);
    expect(scanner.scan(ctx, { kind: "order/desc" })).toBe(false);
    expect(scanner.scan(ctx, { kind: "order/asc-or-desc" })).toBe(true);
  });

  it("returns false for an unknown pattern kind", () => {
    const ctx = { filePath: "x.ts", source: "asc(x)", line: 1 };
    expect(scanner.scan(ctx, { kind: "no-such-pattern" })).toBe(false);
  });

  it("returns false when line is out of bounds", () => {
    const ctx = { filePath: "x.ts", source: "asc(x)", line: 99 };
    expect(scanner.scan(ctx, { kind: "order/asc-or-desc" })).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// PatchApplicator contract
// ---------------------------------------------------------------------------

describe("ReferenceGitApplyApplicator (PatchApplicator contract)", () => {
  let repoRoot: string;

  beforeEach(() => {
    repoRoot = mkdtempSync(join(tmpdir(), "provekit-applicator-test-"));
    execFileSync("git", ["init", "--initial-branch=main"], { cwd: repoRoot, stdio: "pipe" });
    execFileSync("git", ["config", "user.email", "t@t"], { cwd: repoRoot, stdio: "pipe" });
    execFileSync("git", ["config", "user.name", "t"], { cwd: repoRoot, stdio: "pipe" });
    writeFileSync(join(repoRoot, "README.md"), "hello\n");
    execFileSync("git", ["add", "."], { cwd: repoRoot, stdio: "pipe" });
    execFileSync("git", ["commit", "-m", "init"], { cwd: repoRoot, stdio: "pipe" });
  });

  afterEach(() => {
    rmSync(repoRoot, { recursive: true, force: true });
  });

  it("writes file edits and returns ok=true with files list", async () => {
    const sandbox = new ReferenceGitWorktreeSandbox();
    const handle = await sandbox.open(repoRoot, { locusFile: join(repoRoot, "README.md") });

    const applicator = new ReferenceGitApplyApplicator();
    const r = await applicator.apply(handle, {
      fileEdits: [
        { file: "new-file.ts", newContent: "export {}\n" },
        { file: "README.md", newContent: "rewritten\n" },
      ],
      description: "test",
    });
    expect(r.ok).toBe(true);
    expect(r.filesWritten).toEqual(["new-file.ts", "README.md"]);
    expect(readFileSync(join(handle.worktreePath, "README.md"), "utf-8")).toBe("rewritten\n");
    expect(readFileSync(join(handle.worktreePath, "new-file.ts"), "utf-8")).toBe("export {}\n");
    expect(handle.modifiedFiles.has("new-file.ts")).toBe(true);
    expect(handle.modifiedFiles.has("README.md")).toBe(true);
    await sandbox.close(handle);
  });

  it("applies an empty patch as ok=true with empty filesWritten", async () => {
    const sandbox = new ReferenceGitWorktreeSandbox();
    const handle = await sandbox.open(repoRoot, { locusFile: join(repoRoot, "README.md") });
    const applicator = new ReferenceGitApplyApplicator();
    const r = await applicator.apply(handle, { fileEdits: [], description: "noop" });
    expect(r.ok).toBe(true);
    expect(r.filesWritten).toEqual([]);
    await sandbox.close(handle);
  });
});

// ---------------------------------------------------------------------------
// BuildTestRunner contract — shape verification only.
// ---------------------------------------------------------------------------

describe("ReferenceVitestPnpmRunner (BuildTestRunner contract — shape)", () => {
  it("exposes a name and a run() function", () => {
    const runner = new ReferenceVitestPnpmRunner();
    expect(runner.name).toBe("vitest+registry");
    expect(typeof runner.run).toBe("function");
  });

  it("returns a no-runner-detected RunResult on a blank tempdir", () => {
    const dir = mkdtempSync(join(tmpdir(), "provekit-runner-test-"));
    const runner = new ReferenceVitestPnpmRunner();
    // Build a synthetic OverlayHandle. The runner only reads worktreePath.
    const handle = {
      worktreePath: dir,
      sastDbPath: "",
      sastDb: {} as never,
      baseRef: "HEAD",
      modifiedFiles: new Set<string>(),
      closed: false,
    };
    const r = runner.run(handle, { testFilePath: null });
    // No package.json, no vitest config — detect returns "none". The runner
    // surfaces an exit-0 with a stderr explanation.
    expect(r.exitCode).toBe(0);
    expect(r.stderr).toContain("no test runner");
    rmSync(dir, { recursive: true, force: true });
  });
});

// ---------------------------------------------------------------------------
// createReferenceImplementations factory
// ---------------------------------------------------------------------------

describe("createReferenceImplementations()", () => {
  it("returns instances of every reference class", () => {
    const refs = createReferenceImplementations();
    expect(refs.parser).toBeInstanceOf(ReferenceTsMorphParser);
    expect(refs.sandbox).toBeInstanceOf(ReferenceGitWorktreeSandbox);
    expect(refs.testRunner).toBeInstanceOf(ReferenceVitestPnpmRunner);
    expect(refs.scanner).toBeInstanceOf(ReferenceTypeScriptPatternScanner);
    expect(refs.applicator).toBeInstanceOf(ReferenceGitApplyApplicator);
    expect(refs.substrate).toBeUndefined(); // no db passed
  });

  it("includes a ReferenceSqliteSubstrate when db is supplied", () => {
    const dir = mkdtempSync(join(tmpdir(), "provekit-factory-test-"));
    const db = openDb(join(dir, "test.db"));
    migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
    const refs = createReferenceImplementations(db);
    expect(refs.substrate).toBeInstanceOf(ReferenceSqliteSubstrate);
    rmSync(dir, { recursive: true, force: true });
  });
});
