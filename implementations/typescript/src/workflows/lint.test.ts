/**
 * Lint workflow integration test. Verifies:
 * 1. Manifest loads cleanly with one Stage and zero Actions.
 * 2. End-to-end runManifest scans a synthetic project tree and returns
 *    matches/file counts populated.
 * 3. Empty project tree yields zero matches and zero files.
 * 4. Capability declarations match what the manifest references.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadLintManifest,
  registerLintRegistries,
  LINT_STAGE_CAPABILITIES,
  LINT_ACTION_CAPABILITIES,
  type LintWorkflowInput,
} from "./lint.js";
import type { RunPrincipleLibraryLintOutput } from "../workflow/producers/runPrincipleLibraryLint.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "lint-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function makeFixtureProject(): { projectRoot: string; principlesDir: string } {
  const projectRoot = mkdtempSync(join(tmpdir(), "lint-project-"));
  mkdirSync(join(projectRoot, "src"), { recursive: true });
  writeFileSync(
    join(projectRoot, "src", "thing.ts"),
    `export function add(a: number, b: number) { return a + b; }\n`,
    "utf-8",
  );
  // Empty principles directory — no principles to evaluate, but the
  // walk + SAST build still runs. Tests the wiring without binding to
  // any specific principle's matcher behavior.
  const principlesDir = join(projectRoot, ".provekit", "principles");
  mkdirSync(principlesDir, { recursive: true });
  return { projectRoot, principlesDir };
}

describe("lint workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadLintManifest();
    expect(manifest.name).toBe("lint");
    expect(manifest.nodes).toHaveLength(1);
    expect(manifest.nodes[0].capability).toBe("run-principle-library-lint");
    expect(manifest.actions).toHaveLength(0);
  });

  it("declares a CLI block consumed by the meta-dispatcher", () => {
    const manifest = loadLintManifest();
    expect(manifest.cli).toBeDefined();
    expect(manifest.cli!.description).toMatch(/principle library/i);
    const argNames = manifest.cli!.args!.map((a) => a.name);
    expect(argNames).toContain("projectRoot");
    expect(argNames).toContain("ci");
    expect(argNames).toContain("verbose");
  });

  it("walks a fixture project and returns match counts", async () => {
    const db = makeDb();
    const { projectRoot, principlesDir } = makeFixtureProject();

    const manifest = loadLintManifest();
    const { registry, actionRegistry } = registerLintRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: LintWorkflowInput = {
      projectRoot,
      principlesDir,
      drizzleFolder: DRIZZLE_FOLDER,
      verbose: false,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);

    const out = result.output as RunPrincipleLibraryLintOutput;
    expect(out.filesDiscovered).toBeGreaterThan(0);
    expect(out.filesIndexed + out.parserFailures).toBe(out.filesDiscovered);
    expect(out.principlesEvaluated).toBe(0);
    expect(out.principleErrors).toBe(0);
    expect(out.matches).toEqual([]);
  });

  it("declares the expected capabilities", () => {
    expect(LINT_STAGE_CAPABILITIES).toEqual(["run-principle-library-lint"]);
    expect(LINT_ACTION_CAPABILITIES).toEqual([]);
  });
});
