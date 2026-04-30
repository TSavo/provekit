/**
 * runPrincipleLibraryLint Stage tests. Direct producer unit tests; the
 * end-to-end manifest-driven test lives in src/workflows/lint.test.ts.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import {
  makeRunPrincipleLibraryLintStage,
  runPrincipleLibraryLint,
  RUN_PRINCIPLE_LIBRARY_LINT_CAPABILITY,
} from "./runPrincipleLibraryLint.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeFixtureProject(opts: { withSource: boolean }): {
  projectRoot: string;
  principlesDir: string;
} {
  const projectRoot = mkdtempSync(join(tmpdir(), "lint-stage-"));
  if (opts.withSource) {
    mkdirSync(join(projectRoot, "src"), { recursive: true });
    writeFileSync(
      join(projectRoot, "src", "a.ts"),
      `export const x = 1;\n`,
      "utf-8",
    );
  }
  const principlesDir = join(projectRoot, ".provekit", "principles");
  mkdirSync(principlesDir, { recursive: true });
  return { projectRoot, principlesDir };
}

describe("runPrincipleLibraryLint", () => {
  it("exposes the canonical capability name", () => {
    expect(RUN_PRINCIPLE_LIBRARY_LINT_CAPABILITY).toBe(
      "run-principle-library-lint",
    );
  });

  it("returns zero matches and zero files for an empty project tree", async () => {
    const { projectRoot, principlesDir } = makeFixtureProject({
      withSource: false,
    });
    const out = await runPrincipleLibraryLint({
      projectRoot,
      principlesDir,
      drizzleFolder: DRIZZLE_FOLDER,
      verbose: false,
    });
    expect(out.filesDiscovered).toBe(0);
    expect(out.filesIndexed).toBe(0);
    expect(out.parserFailures).toBe(0);
    expect(out.matches).toEqual([]);
  });

  it("indexes a discovered .ts file", async () => {
    const { projectRoot, principlesDir } = makeFixtureProject({
      withSource: true,
    });
    const out = await runPrincipleLibraryLint({
      projectRoot,
      principlesDir,
      drizzleFolder: DRIZZLE_FOLDER,
      verbose: false,
    });
    expect(out.filesDiscovered).toBe(1);
    expect(out.filesIndexed).toBe(1);
    expect(out.parserFailures).toBe(0);
  });

  it("throws when principlesDir does not exist", async () => {
    const { projectRoot } = makeFixtureProject({ withSource: false });
    await expect(
      runPrincipleLibraryLint({
        projectRoot,
        principlesDir: join(projectRoot, "does-not-exist"),
        drizzleFolder: DRIZZLE_FOLDER,
        verbose: false,
      }),
    ).rejects.toThrow(/principles directory not found/);
  });

  it("Stage shape: serializeInput excludes drizzleFolder", () => {
    const stage = makeRunPrincipleLibraryLintStage();
    const serialized = stage.serializeInput({
      projectRoot: "/p",
      principlesDir: "/p/.provekit/principles",
      drizzleFolder: "/p/drizzle",
      verbose: true,
    });
    expect(serialized).toEqual({
      projectRoot: "/p",
      principlesDir: "/p/.provekit/principles",
      verbose: true,
    });
  });

  it("Stage shape: round-trips output through serialize/deserialize", () => {
    const stage = makeRunPrincipleLibraryLintStage();
    const out = {
      matches: [
        {
          principleName: "p1",
          severity: "violation",
          message: "m",
          sourceLine: 3,
          path: "/x/a.ts",
        },
      ],
      filesIndexed: 1,
      parserFailures: 0,
      filesDiscovered: 1,
      principlesEvaluated: 1,
      principleErrors: 0,
    };
    const witness = stage.serializeOutput(out);
    expect(stage.deserializeOutput(witness)).toEqual(out);
  });
});
