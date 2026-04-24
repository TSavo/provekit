/**
 * B4: cli.fix tests — runFixLoopCli with stub deps.
 *
 * Uses StubLLMProvider with canned responses. Uses in-memory SQLite via
 * the same pattern as locate.test.ts. Mocks stdin/stdout via Readable/Writable.
 */

import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
  existsSync,
} from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { Readable, Writable, PassThrough } from "stream";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

import { openDb } from "./db/index.js";
import { gapReports, clauses } from "./db/schema/index.js";
import { buildSASTForFile } from "./sast/builder.js";
import { _clearIntakeRegistry } from "./fix/intakeRegistry.js";
import {
  _clearRemediationLayerRegistry,
  registerRemediationLayer,
} from "./fix/remediationLayerRegistry.js";
import { registerAll as registerAllAdapters } from "./fix/intakeAdapters/index.js";
import { registerAll as registerAllLayers } from "./fix/remediationLayers/index.js";
import { StubLLMProvider } from "./fix/types.js";
import type { FixLoopResult } from "./fix/types.js";
import { runFixLoopCli } from "./cli.fix.js";
import type { RunFixArgs } from "./cli.fix.js";
import type { RunFixLoopArgs } from "./fix/orchestrator.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function openTestDb() {
  const tmpDir = mkdtempSync(join(tmpdir(), "provekit-clifix-test-"));
  const dbPath = join(tmpDir, "test.db");
  const db = openDb(dbPath);
  migrate(db, { migrationsFolder: "./drizzle" });
  return { db, tmpDir };
}

import { dirname } from "path";

function writeFixture(dir: string, filename: string, source: string): string {
  const filePath = join(dir, filename);
  mkdirSync(dirname(filePath), { recursive: true });
  writeFileSync(filePath, source, "utf8");
  return filePath;
}

function makeStdin(text: string): Readable {
  return Readable.from([text]);
}

function makeCapture() {
  let data = "";
  const stream = new Writable({
    write(chunk: Buffer | string, _enc: string, cb: () => void) {
      data += chunk.toString();
      cb();
    },
  });
  return { stream, get text() { return data; } };
}

/**
 * Build a StubLLMProvider that handles:
 *   - Intake (report adapter): keys on "Bug report"
 *   - Intake (gap_report adapter): keys on "SAST gap finding"
 *   - Classify: keys on "classifying a bug report"
 */
function makeStubLLM(overrides?: {
  intakeJson?: string;
  gapIntakeJson?: string;
  classifyJson?: string;
}): StubLLMProvider {
  const intakeResponse = overrides?.intakeJson ?? JSON.stringify({
    summary: "signup returns 500 when OAuth token missing",
    failureDescription: "POST /auth/signup throws when process.env.GOOGLE_OAUTH_SECRET is undefined",
    fixHint: "add a startup assertion so the server fails fast",
    codeReferences: [{ file: "src/auth/signup.ts", line: 42, function: "signup" }],
    bugClassHint: "config-missing",
  });

  const gapIntakeResponse = overrides?.gapIntakeJson ?? JSON.stringify({
    summary: "Possible null dereference at src/api.ts:10.",
    failureDescription: "Value may be null at assignment in fetchUser.",
  });

  const classifyResponse = overrides?.classifyJson ?? JSON.stringify({
    primaryLayer: "code_invariant",
    secondaryLayers: ["config"],
    artifacts: [
      { kind: "code_patch", rationale: "validate secret before call" },
      { kind: "regression_test", rationale: "verify error on missing secret" },
      { kind: "startup_assert", envVar: "OAUTH_SECRET" },
      { kind: "principle_candidate" },
    ],
    rationale: "The signup function calls an external OAuth endpoint without checking that the required secret is present in process.env.",
  });

  return new StubLLMProvider(
    new Map([
      ["Bug report", intakeResponse],
      ["SAST gap finding", gapIntakeResponse],
      ["classifying a bug report", classifyResponse],
    ]),
  );
}

/**
 * Build a stub FixLoopResult for tests that don't care about orchestrator internals.
 * Returns a successful result with a minimal FixBundle.
 */
function makeStubFixLoopResult(overrides?: Partial<FixLoopResult>): FixLoopResult {
  const bundle: FixLoopResult["bundle"] = overrides?.bundle !== undefined
    ? overrides.bundle
    : {
        bundleId: 1,
        bundleType: "fix",
        bugSignal: {} as never,
        plan: {} as never,
        artifacts: {
          primaryFix: null,
          complementary: [],
          test: null,
          principle: null,
          capabilitySpec: null,
        },
        coherence: {
          sastStructural: true,
          z3SemanticConsistency: true,
          fullSuiteGreen: true,
          noNewGapsIntroduced: true,
          migrationSafe: null,
          crossCodebaseRegression: null,
          extractorCoverage: null,
          substrateConsistency: null,
          principleNeedsCapability: null,
        },
        confidence: 0.9,
        auditTrail: [],
      };

  return {
    bundle,
    applied: overrides?.applied ?? true,
    auditTrail: overrides?.auditTrail ?? [],
    reason: overrides?.reason,
    applyResult: overrides?.applyResult,
  };
}

/**
 * A no-op runFixLoopFn stub. Returns a successful result without running any stages.
 * Used as the default in makeArgs() so existing tests aren't affected by the new wiring.
 */
function makeNoOpFixLoopFn() {
  return vi.fn(async (_args: RunFixLoopArgs): Promise<FixLoopResult> => {
    return makeStubFixLoopResult();
  });
}

/** Args builder with sensible defaults */
function makeArgs(
  overrides: Partial<RunFixArgs> & { ref: string; db: ReturnType<typeof openDb> },
): RunFixArgs {
  const out = makeCapture();
  const err = makeCapture();
  return {
    ref: overrides.ref,
    db: overrides.db,
    llm: overrides.llm ?? makeStubLLM(),
    confirm: overrides.confirm ?? false, // default: --no-confirm
    dryRun: overrides.dryRun ?? false,
    apply: overrides.apply ?? false,
    maxSites: overrides.maxSites ?? 10,
    stdout: overrides.stdout ?? out.stream,
    stderr: overrides.stderr ?? err.stream,
    stdin: overrides.stdin ?? makeStdin(""),
    // Default: no-op stub so tests don't invoke real orchestrator stages
    runFixLoopFn: overrides.runFixLoopFn ?? makeNoOpFixLoopFn(),
  };
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("runFixLoopCli()", () => {
  let tmpDir: string;
  let db: ReturnType<typeof openDb>;

  beforeEach(() => {
    _clearIntakeRegistry();
    registerAllAdapters();
    _clearRemediationLayerRegistry();
    registerAllLayers();
  });

  afterEach(() => {
    try { db.$client.close(); } catch { /* ignore */ }
    if (tmpDir && existsSync(tmpDir)) {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  // -------------------------------------------------------------------------
  // Test 1: gap_report ref — seed a row, assert flow
  // -------------------------------------------------------------------------
  it("gap_report ref: seeds gap_report row, runs intake → locate → classify, exits 0 (--no-confirm)", async () => {
    ({ db, tmpDir } = openTestDb());

    // Write a fixture and build SAST so locate() can find a node
    const source = "function fetchUser() { return null; }\n";
    const fixturePath = writeFixture(tmpDir, "src/api.ts", source);
    buildSASTForFile(db, fixturePath);

    // Insert clause + gap_report row
    const clause = db
      .insert(clauses)
      .values({
        contractKey: `${fixturePath}/fetchUser[1]`,
        verdict: "violation",
        smt2: "(assert false)",
        clauseHash: "h1",
        principleName: "null-safety",
      })
      .returning()
      .get();

    db.insert(gapReports).values({
      clauseId: clause.id,
      kind: "null_undefined",
      atNodeRef: `${fixturePath}:1:fetchUser`,
      explanation: "Possible null dereference",
    }).run();

    const out = makeCapture();
    const err = makeCapture();
    const gapLlm = new StubLLMProvider(
      new Map([
        ["SAST gap finding", JSON.stringify({
          summary: "Possible null dereference in fetchUser.",
          failureDescription: "Value may be null at line 1.",
        })],
        ["classifying a bug report", JSON.stringify({
          primaryLayer: "code_invariant",
          secondaryLayers: [],
          artifacts: [{ kind: "code_patch" }],
          rationale: "Null return without guard.",
        })],
      ]),
    );

    const exitCode = await runFixLoopCli({
      ref: "gap_report:1",
      db,
      llm: gapLlm,
      confirm: false,
      dryRun: false,
      stdout: out.stream,
      stderr: err.stream,
      stdin: makeStdin(""),
      runFixLoopFn: makeNoOpFixLoopFn(),
    });

    expect(exitCode).toBe(0);
    expect(out.text).toContain("Intake");
    expect(out.text).toContain("Locate");
    expect(out.text).toContain("Classify");
    expect(out.text).toContain("Plan ready");
  });

  // -------------------------------------------------------------------------
  // Test 2: File path ref
  // -------------------------------------------------------------------------
  it("file path ref: reads file, runs full flow, exits 0", async () => {
    ({ db, tmpDir } = openTestDb());

    // Write SAST fixture
    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    // Write bug report file
    const bugReportPath = join(tmpDir, "bug-report.md");
    writeFileSync(bugReportPath, "signup returns 500 when OAuth token missing", "utf8");

    // Stub LLM: intake references the SAST fixture file so locate() works
    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup returns 500 when OAuth token missing",
        failureDescription: "POST /auth/signup throws when GOOGLE_OAUTH_SECRET is undefined",
        fixHint: "add startup assertion",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
        bugClassHint: "config-missing",
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "config",
        secondaryLayers: [],
        artifacts: [{ kind: "startup_assert", envVar: "OAUTH_SECRET" }],
        rationale: "Missing env var.",
      })],
    ]));

    const out = makeCapture();
    const exitCode = await runFixLoopCli(
      makeArgs({ ref: bugReportPath, db, llm, stdout: out.stream }),
    );

    expect(exitCode).toBe(0);
    expect(out.text).toContain("Intake");
    expect(out.text).toContain("Source: report");
    expect(out.text).toContain("Locate");
    expect(out.text).toContain("Classify");
  });

  // -------------------------------------------------------------------------
  // Test 3: Plain text ref
  // -------------------------------------------------------------------------
  it("plain text ref: routes through report adapter, exits 0 (no SAST → exit 2)", async () => {
    ({ db, tmpDir } = openTestDb());

    // No SAST indexed — locate will return null → exit 2
    const out = makeCapture();
    const err = makeCapture();
    const exitCode = await runFixLoopCli({
      ref: "NullPointerException in signup",
      db,
      llm: makeStubLLM(),
      confirm: false,
      dryRun: false,
      stdout: out.stream,
      stderr: err.stream,
      stdin: makeStdin(""),
    });

    // Intake succeeds but locate returns null (no SAST indexed for this ref)
    expect(exitCode).toBe(2);
    expect(err.text).toContain("Unable to resolve code references");
  });

  // -------------------------------------------------------------------------
  // Test 4: gh:<number> ref — treated as report source (v1)
  // -------------------------------------------------------------------------
  it("gh:<number> ref: treated as report adapter (v1 stub), intake runs", async () => {
    ({ db, tmpDir } = openTestDb());

    const out = makeCapture();
    const err = makeCapture();

    // No SAST → locate null → exit 2, but intake ran with report source
    // We verify intake ran by checking no intake error (error code would be 1)
    const exitCode = await runFixLoopCli({
      ref: "gh:4217",
      db,
      llm: makeStubLLM(),
      confirm: false,
      dryRun: false,
      stdout: out.stream,
      stderr: err.stream,
      stdin: makeStdin(""),
    });

    // exit 2 (locate null) confirms intake did NOT fail
    expect(exitCode).toBe(2);
    expect(err.text).not.toContain("Intake error");
  });

  // -------------------------------------------------------------------------
  // Test 5: Unknown adapter (clear registry) → exit 1
  // -------------------------------------------------------------------------
  it("unknown adapter: clearing intake registry causes intake error, exits 1", async () => {
    ({ db, tmpDir } = openTestDb());

    // Clear registry so all adapters are gone
    _clearIntakeRegistry();
    // Re-add only a dummy adapter with name "dummy" — now "report" is unregistered
    // Actually: resolveRef will produce source="report" for plain text, which will fail.

    const out = makeCapture();
    const err = makeCapture();
    const exitCode = await runFixLoopCli({
      ref: "some plain text bug report",
      db,
      llm: makeStubLLM(),
      confirm: false,
      dryRun: false,
      stdout: out.stream,
      stderr: err.stream,
      stdin: makeStdin(""),
    });

    expect(exitCode).toBe(1);
    expect(err.text).toContain("Intake error");
  });

  // -------------------------------------------------------------------------
  // Test 6: Locate returns null → exit 2
  // -------------------------------------------------------------------------
  it("locate null: signal with empty codeReferences returns exit 2", async () => {
    ({ db, tmpDir } = openTestDb());

    // LLM returns signal with no code references
    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "Some vague bug",
        failureDescription: "Something fails somewhere",
        codeReferences: [],
        fixHint: undefined,
        bugClassHint: undefined,
      })],
      ["classifying a bug report", "{}"],
    ]));

    const err = makeCapture();
    const exitCode = await runFixLoopCli(
      makeArgs({ ref: "vague bug report with no code refs", db, llm, stderr: err.stream }),
    );

    expect(exitCode).toBe(2);
    expect(err.text).toContain("Unable to resolve code references");
  });

  // -------------------------------------------------------------------------
  // Test 7: Classify failure (unknown primary layer) → exit 3
  // -------------------------------------------------------------------------
  it("classify failure: unknown primaryLayer returns exit 3 with error message", async () => {
    ({ db, tmpDir } = openTestDb());

    // Write SAST fixture so locate() works
    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "it crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "mythical",
        secondaryLayers: [],
        artifacts: [],
        rationale: "made up",
      })],
    ]));

    const err = makeCapture();
    const exitCode = await runFixLoopCli(
      makeArgs({ ref: "signup fails", db, llm, stderr: err.stream }),
    );

    expect(exitCode).toBe(3);
    expect(err.text).toContain("Classify error");
    expect(err.text).toContain("mythical");
  });

  // -------------------------------------------------------------------------
  // Test 8: --dry-run prints JSON, no confirm prompt, exits 0
  // -------------------------------------------------------------------------
  it("--dry-run: prints JSON to stdout, no confirm prompt, exits 0", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes on missing token",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [{ kind: "code_patch" }],
        rationale: "logic bug",
      })],
    ]));

    const out = makeCapture();
    const err = makeCapture();
    // Use PassThrough so the stdin read never blocks
    const stdin = new PassThrough();
    stdin.end();

    const exitCode = await runFixLoopCli({
      ref: "signup fails",
      db,
      llm,
      confirm: true,
      dryRun: true,
      stdout: out.stream,
      stderr: err.stream,
      stdin,
    });

    expect(exitCode).toBe(0);
    // Output should be valid JSON
    const parsed = JSON.parse(out.text) as { plan: { primaryLayer: string } };
    expect(parsed.plan.primaryLayer).toBe("code_invariant");
    // No "Proceed?" prompt
    expect(out.text).not.toContain("Proceed?");
  });

  // -------------------------------------------------------------------------
  // Test 9: --no-confirm (confirm: false) — no prompt, exits 0
  // -------------------------------------------------------------------------
  it("--no-confirm: no interaction required, exits 0", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "logic bug",
      })],
    ]));

    const out = makeCapture();
    const exitCode = await runFixLoopCli(
      makeArgs({ ref: "signup fails", db, llm, confirm: false, stdout: out.stream }),
    );

    expect(exitCode).toBe(0);
    expect(out.text).toContain("Plan ready");
    expect(out.text).not.toContain("Proceed?");
  });

  // -------------------------------------------------------------------------
  // Test 10: Confirmation "n" response → exit 4
  // -------------------------------------------------------------------------
  it("confirmation 'n': user declines, exits 4", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "logic bug",
      })],
    ]));

    const out = makeCapture();
    const stdin = Readable.from(["n\n"]);

    const exitCode = await runFixLoopCli({
      ref: "signup fails",
      db,
      llm,
      confirm: true,
      dryRun: false,
      stdout: out.stream,
      stderr: makeCapture().stream,
      stdin,
    });

    expect(exitCode).toBe(4);
    expect(out.text).toContain("Proceed?");
  });

  // -------------------------------------------------------------------------
  // Test 11: Confirmation "y" response → runs orchestrator, exits 0
  // -------------------------------------------------------------------------
  it("confirmation 'y': user confirms, runFixLoopFn called, exits 0", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "logic bug",
      })],
    ]));

    const out = makeCapture();
    const stdin = Readable.from(["y\n"]);
    const runFixLoopFn = makeNoOpFixLoopFn();

    const exitCode = await runFixLoopCli({
      ref: "signup fails",
      db,
      llm,
      confirm: true,
      dryRun: false,
      stdout: out.stream,
      stderr: makeCapture().stream,
      stdin,
      runFixLoopFn,
    });

    expect(exitCode).toBe(0);
    expect(runFixLoopFn).toHaveBeenCalledOnce();
    expect(out.text).toContain("Running fix loop");
  });

  // -------------------------------------------------------------------------
  // Test 12: confirm y, orchestrator succeeds — bundle summary in stdout
  // -------------------------------------------------------------------------
  it("confirm y, orchestrator succeeds: stdout includes bundle summary, exits 0", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "logic bug",
      })],
    ]));

    const out = makeCapture();
    const stdin = Readable.from(["y\n"]);
    const runFixLoopFn = vi.fn(async (): Promise<FixLoopResult> =>
      makeStubFixLoopResult({ confidence: 0.87 } as Partial<FixLoopResult>),
    );

    const exitCode = await runFixLoopCli({
      ref: "signup fails",
      db,
      llm,
      confirm: true,
      dryRun: false,
      stdout: out.stream,
      stderr: makeCapture().stream,
      stdin,
      runFixLoopFn,
    });

    expect(exitCode).toBe(0);
    expect(out.text).toContain("Bundle type:");
    expect(out.text).toContain("Confidence:");
    expect(out.text).toContain("Coherence:");
  });

  // -------------------------------------------------------------------------
  // Test 13: confirm y, orchestrator fails (null bundle) → exit 5
  // -------------------------------------------------------------------------
  it("confirm y, orchestrator fails: null bundle → exit 5", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "logic bug",
      })],
    ]));

    const err = makeCapture();
    const stdin = Readable.from(["y\n"]);
    const runFixLoopFn = vi.fn(async (): Promise<FixLoopResult> => ({
      bundle: null,
      applied: false,
      auditTrail: [
        { stage: "C3", kind: "error", detail: "generateFixCandidate blew up", timestamp: Date.now() },
      ],
      reason: "aborted at C3",
    }));

    const exitCode = await runFixLoopCli({
      ref: "signup fails",
      db,
      llm,
      confirm: true,
      dryRun: false,
      stdout: makeCapture().stream,
      stderr: err.stream,
      stdin,
      runFixLoopFn,
    });

    expect(exitCode).toBe(5);
    expect(err.text).toContain("Fix loop failed");
    expect(err.text).toContain("C3");
  });

  // -------------------------------------------------------------------------
  // Test 14: --apply flag routes autoApply: true to runFixLoopFn
  // -------------------------------------------------------------------------
  it("--apply flag: runFixLoopFn receives options.autoApply: true", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "logic bug",
      })],
    ]));

    const runFixLoopFn = makeNoOpFixLoopFn();

    const exitCode = await runFixLoopCli(
      makeArgs({
        ref: "signup fails",
        db,
        llm,
        confirm: false,
        apply: true,
        runFixLoopFn,
      }),
    );

    expect(exitCode).toBe(0);
    expect(runFixLoopFn).toHaveBeenCalledOnce();
    const callArgs = runFixLoopFn.mock.calls[0][0] as RunFixLoopArgs;
    expect(callArgs.options.autoApply).toBe(true);
  });

  // -------------------------------------------------------------------------
  // Test 15: prDraft mode writes patch + body files to cwd
  // -------------------------------------------------------------------------
  it("prDraft mode: writes provekit-fix.patch and provekit-fix.md to cwd", async () => {
    ({ db, tmpDir } = openTestDb());

    const source = "function signup(token: string) { return token; }\n";
    const fixturePath = writeFixture(tmpDir, "src/auth/signup.ts", source);
    buildSASTForFile(db, fixturePath);

    const llm = new StubLLMProvider(new Map([
      ["Bug report", JSON.stringify({
        summary: "signup fails",
        failureDescription: "crashes",
        codeReferences: [{ file: fixturePath, line: 1, function: "signup" }],
      })],
      ["classifying a bug report", JSON.stringify({
        primaryLayer: "code_invariant",
        secondaryLayers: [],
        artifacts: [],
        rationale: "logic bug",
      })],
    ]));

    const runFixLoopFn = vi.fn(async (): Promise<FixLoopResult> =>
      makeStubFixLoopResult({
        applyResult: {
          applied: false,
          prDraft: {
            patch: "--- a/signup.ts\n+++ b/signup.ts\n",
            prBody: "## Fix\nApplied invariant check.",
          },
        },
      }),
    );

    // Use tmpDir as cwd for file writes
    const origCwd = process.cwd();
    process.chdir(tmpDir);
    try {
      const exitCode = await runFixLoopCli(
        makeArgs({
          ref: "signup fails",
          db,
          llm,
          confirm: false,
          apply: false,
          runFixLoopFn,
        }),
      );

      expect(exitCode).toBe(0);
      expect(existsSync(join(tmpDir, "provekit-fix.patch"))).toBe(true);
      expect(existsSync(join(tmpDir, "provekit-fix.md"))).toBe(true);
    } finally {
      process.chdir(origCwd);
    }
  });
});
