/**
 * Attest workflow integration test. Verifies:
 * 1. Manifest loads cleanly with two Stages and one Action.
 * 2. End-to-end runManifest scans an empty tree, produces a project root
 *    memento, writes the summary file.
 * 3. The Action runs AFTER verify; the summary file's projectRootCid
 *    matches the Stage output.
 * 4. Capability declarations match what the manifest references.
 */

import { describe, it, expect } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  existsSync,
  writeFileSync,
} from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import { generateKeypair } from "../producerKeys/index.js";
import {
  loadAttestManifest,
  registerAttestRegistries,
  ATTEST_STAGE_CAPABILITIES,
  ATTEST_ACTION_CAPABILITIES,
  type AttestWorkflowInput,
} from "./attest.js";
import type { VerifyProjectInvariantsStageOutput } from "../workflow/producers/verifyProjectInvariants.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "attest-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function fixedKey() {
  const seed = Buffer.from(
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "hex",
  );
  return generateKeypair({ seed });
}

describe("attest workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadAttestManifest();
    expect(manifest.name).toBe("attest");
    expect(manifest.nodes).toHaveLength(2);
    expect(manifest.nodes.map((n) => n.capability)).toEqual([
      "scan-invariant-files",
      "verify-project-invariants",
    ]);
    expect(manifest.actions).toHaveLength(1);
    expect(manifest.actions![0].action).toBe("write-attest-summary");
    expect(manifest.actions![0].runAfter).toBe("$node.verify");
  });

  it("declares a CLI block with projectRoot/out/ci", () => {
    const manifest = loadAttestManifest();
    expect(manifest.cli).toBeDefined();
    const argNames = manifest.cli!.args!.map((a) => a.name);
    expect(argNames).toEqual(expect.arrayContaining(["projectRoot", "out", "ci"]));
  });

  it("scans an empty tree, produces a project root, writes summary file", async () => {
    const db = makeDb();
    const projectRoot = mkdtempSync(join(tmpdir(), "attest-project-"));
    mkdirSync(join(projectRoot, "src"), { recursive: true });
    const outDir = mkdtempSync(join(tmpdir(), "attest-out-"));
    const outPath = join(outDir, "attest-summary.json");

    const { privateKey } = fixedKey();
    const manifest = loadAttestManifest();
    const { registry, actionRegistry } = registerAttestRegistries({
      privateKey,
      producerVersion: "attest@test",
      producedAt: "2026-01-01T00:00:00.000Z",
    });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: AttestWorkflowInput = {
      scanRoot: join(projectRoot, "src"),
      projectRoot,
      projectName: "test-pkg",
      projectVersion: "0.0.1",
      locallyAvailableCids: [],
      outPath,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);
    const out = result.output as VerifyProjectInvariantsStageOutput;

    expect(out.declarations).toEqual([]);
    expect(out.nullRoots).toEqual([]);
    expect(typeof out.projectRootCid).toBe("string");

    // Summary file landed.
    expect(existsSync(outPath)).toBe(true);
    const summary = JSON.parse(readFileSync(outPath, "utf-8"));
    expect(summary.projectName).toBe("test-pkg");
    expect(summary.projectRootCid).toBe(out.projectRootCid);
    expect(summary.declarations).toEqual([]);
    expect(summary.nullRoots).toEqual([]);
  });

  it("scans a project with no .invariant.ts and reports zero declarations", async () => {
    const db = makeDb();
    const projectRoot = mkdtempSync(join(tmpdir(), "attest-empty-"));
    mkdirSync(join(projectRoot, "src", "deep"), { recursive: true });
    writeFileSync(
      join(projectRoot, "src", "ordinary.ts"),
      `export const x = 1;\n`,
      "utf-8",
    );
    const outDir = mkdtempSync(join(tmpdir(), "attest-out-empty-"));
    const outPath = join(outDir, "summary.json");

    const { privateKey } = fixedKey();
    const manifest = loadAttestManifest();
    const { registry, actionRegistry } = registerAttestRegistries({
      privateKey,
      producerVersion: "attest@test",
      producedAt: "2026-01-01T00:00:00.000Z",
    });
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: AttestWorkflowInput = {
      scanRoot: join(projectRoot, "src"),
      projectRoot,
      projectName: "empty-pkg",
      projectVersion: "0.0.0",
      locallyAvailableCids: [],
      outPath,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);
    const out = result.output as VerifyProjectInvariantsStageOutput;
    expect(out.declarations).toEqual([]);
  });

  it("declares the expected capabilities", () => {
    expect(ATTEST_STAGE_CAPABILITIES).toEqual([
      "scan-invariant-files",
      "verify-project-invariants",
    ]);
    expect(ATTEST_ACTION_CAPABILITIES).toEqual(["write-attest-summary"]);
  });
});
