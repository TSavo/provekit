/**
 * Tests for `provekit attest`. Exercises argv parsing, key handling,
 * and the "no invariant files" early-exit. Heavy producer logic
 * (runVerifyProjectInvariants) is mocked at the module boundary so
 * these tests don't touch ed25519 or the project-DAG composition.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";

const { runVerifyProjectInvariantsMock } = vi.hoisted(() => ({
  runVerifyProjectInvariantsMock: vi.fn(),
}));
vi.mock("./workflow/producers/verifyProjectInvariants.js", () => ({
  runVerifyProjectInvariants: runVerifyProjectInvariantsMock,
}));

import { runAttest } from "./cli.attest";

interface CapturedStream {
  read(): string;
}

function captureStdio(): {
  stdout: CapturedStream;
  stderr: CapturedStream;
  restore: () => void;
} {
  const out: string[] = [];
  const err: string[] = [];
  const origStdoutWrite = process.stdout.write.bind(process.stdout);
  const origStderrWrite = process.stderr.write.bind(process.stderr);
  process.stdout.write = ((chunk: string | Uint8Array) => {
    out.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf-8"));
    return true;
  }) as typeof process.stdout.write;
  process.stderr.write = ((chunk: string | Uint8Array) => {
    err.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf-8"));
    return true;
  }) as typeof process.stderr.write;
  return {
    stdout: { read: () => out.join("") },
    stderr: { read: () => err.join("") },
    restore: () => {
      process.stdout.write = origStdoutWrite;
      process.stderr.write = origStderrWrite;
    },
  };
}

function captureExit(): { code: () => number | undefined; restore: () => void } {
  let code: number | undefined;
  const orig = process.exit;
  process.exit = ((c?: number) => {
    code = c ?? 0;
    throw new Error("__test_exit__");
  }) as never;
  return {
    code: () => code,
    restore: () => {
      process.exit = orig;
    },
  };
}

describe("runAttest", () => {
  let tmpDir: string;
  let exit: ReturnType<typeof captureExit>;
  let stdio: ReturnType<typeof captureStdio>;

  beforeEach(() => {
    runVerifyProjectInvariantsMock.mockReset();
    exit = captureExit();
    stdio = captureStdio();
  });

  afterEach(() => {
    stdio.restore();
    exit.restore();
    if (tmpDir) {
      try {
        rmSync(tmpDir, { recursive: true, force: true });
      } catch {
        /* ignore */
      }
    }
    vi.restoreAllMocks();
  });

  it("--help prints usage to stdout and returns without invoking the verifier", async () => {
    await runAttest(["--help"]);
    const out = stdio.stdout.read();
    expect(out).toContain("provekit attest");
    expect(out).toContain("Usage:");
    expect(runVerifyProjectInvariantsMock).not.toHaveBeenCalled();
  });

  it("-h is treated identically to --help", async () => {
    await runAttest(["-h"]);
    const out = stdio.stdout.read();
    expect(out).toContain("provekit attest");
    expect(runVerifyProjectInvariantsMock).not.toHaveBeenCalled();
  });

  it("with no *.invariant.ts files: writes notice to stderr and exits 0", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "attest-empty-"));
    mkdirSync(join(tmpDir, "src"), { recursive: true });
    // src/ exists but contains no *.invariant.ts files.

    let caught: Error | null = null;
    try {
      await runAttest([tmpDir]);
    } catch (e) {
      caught = e as Error;
    }

    // The runtime expects process.exit(0) for "no files" — our
    // captureExit throws to short-circuit, so we expect the sentinel.
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(0);

    const err = stdio.stderr.read();
    expect(err).toContain("no *.invariant.ts files found");
    expect(runVerifyProjectInvariantsMock).not.toHaveBeenCalled();
  });

  it("finds invariant files, mints, and reports zero null roots", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "attest-mint-clean-"));
    mkdirSync(join(tmpDir, "src"), { recursive: true });
    writeFileSync(
      join(tmpDir, "src", "foo.invariant.ts"),
      "// invariant placeholder\n",
      "utf-8",
    );
    writeFileSync(
      join(tmpDir, "package.json"),
      JSON.stringify({ name: "demo-project", version: "1.2.3" }),
      "utf-8",
    );

    runVerifyProjectInvariantsMock.mockResolvedValue({
      declarations: [
        {
          declarationKind: "property",
          declarationName: "demo.property.foo",
          cid: "deadbeef".repeat(4),
        },
      ],
      projectRootCid: "cafefeed".repeat(4),
      nullRoots: [],
    });

    await runAttest([tmpDir]);
    expect(runVerifyProjectInvariantsMock).toHaveBeenCalledTimes(1);
    const callInput = runVerifyProjectInvariantsMock.mock.calls[0][0];
    expect(callInput.projectName).toBe("demo-project");
    expect(callInput.projectVersion).toBe("1.2.3");
    expect(callInput.invariantFiles).toHaveLength(1);
    expect(callInput.invariantFiles[0].path).toBe("foo.invariant.ts");

    const err = stdio.stderr.read();
    expect(err).toContain("provekit attest demo-project@1.2.3");
    expect(err).toContain("found 1 invariant file");
    expect(err).toContain("provably correct: 0 null roots");
  });

  it("--ci with non-empty nullRoots exits 1", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "attest-mint-nullroots-"));
    mkdirSync(join(tmpDir, "src"), { recursive: true });
    writeFileSync(
      join(tmpDir, "src", "bar.invariant.ts"),
      "// placeholder\n",
      "utf-8",
    );

    runVerifyProjectInvariantsMock.mockResolvedValue({
      declarations: [
        {
          declarationKind: "property",
          declarationName: "demo.property.bar",
          cid: "feedbeef".repeat(4),
        },
      ],
      projectRootCid: "cafe1234".repeat(4),
      nullRoots: ["unreachable-cid-1", "unreachable-cid-2"],
    });

    let caught: Error | null = null;
    try {
      await runAttest([tmpDir, "--ci"]);
    } catch (e) {
      caught = e as Error;
    }
    expect(caught?.message).toBe("__test_exit__");
    expect(exit.code()).toBe(1);

    const err = stdio.stderr.read();
    expect(err).toContain("verification incomplete: 2 null root(s)");
    expect(err).toContain("unreachable-cid-1");
    expect(err).toContain("unreachable-cid-2");
  });

  it("without --ci, nullRoots are reported but exit code is 0", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "attest-mint-no-ci-"));
    mkdirSync(join(tmpDir, "src"), { recursive: true });
    writeFileSync(
      join(tmpDir, "src", "baz.invariant.ts"),
      "// placeholder\n",
      "utf-8",
    );

    runVerifyProjectInvariantsMock.mockResolvedValue({
      declarations: [],
      projectRootCid: "11112222".repeat(4),
      nullRoots: ["dangling-1"],
    });

    // No --ci flag: runAttest returns normally without calling exit().
    await runAttest([tmpDir]);
    expect(exit.code()).toBeUndefined();

    const err = stdio.stderr.read();
    expect(err).toContain("verification incomplete: 1 null root(s)");
  });

  it("warns when generating an ephemeral keypair (no --key, no env)", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "attest-ephemeral-"));
    mkdirSync(join(tmpDir, "src"), { recursive: true });
    writeFileSync(
      join(tmpDir, "src", "x.invariant.ts"),
      "// placeholder\n",
      "utf-8",
    );

    runVerifyProjectInvariantsMock.mockResolvedValue({
      declarations: [],
      projectRootCid: "33334444".repeat(4),
      nullRoots: [],
    });

    const savedKey = process.env.PROVEKIT_KEY;
    delete process.env.PROVEKIT_KEY;
    try {
      await runAttest([tmpDir]);
      const err = stdio.stderr.read();
      expect(err).toContain("warning: no key supplied");
      expect(err).toContain("generating ephemeral keypair");
      // Final note also surfaces ephemeral status.
      expect(err).toContain("ephemeral keypair used");
    } finally {
      if (savedKey !== undefined) process.env.PROVEKIT_KEY = savedKey;
    }
  });

  it("--out writes attest-summary.json to the requested directory", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "attest-out-"));
    mkdirSync(join(tmpDir, "src"), { recursive: true });
    writeFileSync(
      join(tmpDir, "src", "y.invariant.ts"),
      "// placeholder\n",
      "utf-8",
    );

    runVerifyProjectInvariantsMock.mockResolvedValue({
      declarations: [
        {
          declarationKind: "property",
          declarationName: "demo.y",
          cid: "1".repeat(32),
        },
      ],
      projectRootCid: "2".repeat(32),
      nullRoots: [],
    });

    const outDir = join(tmpDir, "attest-out");
    await runAttest([tmpDir, "--out", outDir]);

    const { readFileSync, existsSync } = await import("fs");
    const summaryPath = join(outDir, "attest-summary.json");
    expect(existsSync(summaryPath)).toBe(true);
    const summary = JSON.parse(readFileSync(summaryPath, "utf-8"));
    expect(summary.projectRootCid).toBe("2".repeat(32));
    expect(summary.declarations).toHaveLength(1);
  });

  it("walks subdirectories under src/ recursively for *.invariant.ts", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "attest-recursive-"));
    mkdirSync(join(tmpDir, "src", "deep", "deeper"), { recursive: true });
    writeFileSync(
      join(tmpDir, "src", "shallow.invariant.ts"),
      "// shallow\n",
      "utf-8",
    );
    writeFileSync(
      join(tmpDir, "src", "deep", "deeper", "leaf.invariant.ts"),
      "// leaf\n",
      "utf-8",
    );
    // Files that should be ignored:
    writeFileSync(
      join(tmpDir, "src", "not-invariant.ts"),
      "// noise\n",
      "utf-8",
    );

    runVerifyProjectInvariantsMock.mockResolvedValue({
      declarations: [],
      projectRootCid: "0".repeat(32),
      nullRoots: [],
    });

    await runAttest([tmpDir]);
    const callInput = runVerifyProjectInvariantsMock.mock.calls[0][0];
    expect(callInput.invariantFiles).toHaveLength(2);
    const paths = callInput.invariantFiles.map((f: { path: string }) => f.path).sort();
    expect(paths).toEqual([
      "deep/deeper/leaf.invariant.ts",
      "shallow.invariant.ts",
    ]);
  });
});
