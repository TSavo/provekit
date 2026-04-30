/**
 * Retire workflow integration test. Verifies:
 * 1. Manifest loads cleanly with one Stage and one Action.
 * 2. End-to-end runManifest mints a verdict:decayed memento.
 * 3. The Action appends a must.skip marker to the target file.
 * 4. The Action runs AFTER the Stage (the manifest's runAfter clause).
 * 5. Reason validation propagates from the Stage.
 */

import { describe, it, expect } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  existsSync,
} from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../db/index.js";
import { WorkflowRunner } from "../workflow/runner.js";
import { runManifest, manifestToWorkflow } from "../workflow/manifest.js";
import {
  loadRetireManifest,
  registerRetireRegistries,
  RETIRE_STAGE_CAPABILITIES,
  RETIRE_ACTION_CAPABILITIES,
  type RetireWorkflowInput,
} from "./retire.js";
import type { MintDeprecationOutput } from "../workflow/producers/mintDeprecation.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "retire-workflow-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

describe("retire workflow", () => {
  it("loads the manifest cleanly", () => {
    const manifest = loadRetireManifest();
    expect(manifest.name).toBe("retire");
    expect(manifest.nodes).toHaveLength(1);
    expect(manifest.nodes[0].capability).toBe("mint-deprecation");
    expect(manifest.actions).toHaveLength(1);
    expect(manifest.actions![0].action).toBe("write-invariant-file");
    expect(manifest.actions![0].runAfter).toBe("$node.mint");
  });

  it("mints a decayed verdict and appends the must.skip marker", async () => {
    const db = makeDb();
    const tmp = mkdtempSync(join(tmpdir(), "retire-target-"));
    const target = join(tmp, "src", "math.invariant.ts");
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(
      target,
      `import { property } from 'provekit/ir';\nproperty("foo", x => x > 0);\n`,
      "utf-8",
    );

    const manifest = loadRetireManifest();
    const { registry, actionRegistry } = registerRetireRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    const input: RetireWorkflowInput = {
      retiredPropertyHash: "abcdef0123456789",
      propertyName: "foo",
      reason: "superseded by foo-v2",
      filePath: target,
      skipMarker: `\n// retired: foo (superseded by foo-v2)\nproperty.skip("foo", x => x > 0);\n`,
    };
    const result = await runManifest(runner, registry, manifest, input, actionRegistry);

    const out = result.output as MintDeprecationOutput;
    expect(out.verdict).toBe("decayed");
    expect(out.propertyName).toBe("foo");
    expect(out.reason).toBe("superseded by foo-v2");

    // The Action must have appended the marker.
    const final = readFileSync(target, "utf-8");
    expect(final).toContain(`property("foo", x => x > 0);`);
    expect(final).toContain("// retired: foo");
    expect(final).toContain(`property.skip("foo", x => x > 0);`);
  });

  it("requires a non-empty reason — propagated from the Stage", async () => {
    const db = makeDb();
    const tmp = mkdtempSync(join(tmpdir(), "retire-target-"));
    const target = join(tmp, "fresh.invariant.ts");

    const manifest = loadRetireManifest();
    const { registry, actionRegistry } = registerRetireRegistries();
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);

    await expect(
      runManifest(
        runner,
        registry,
        manifest,
        {
          retiredPropertyHash: "abc",
          propertyName: "x",
          reason: "",
          filePath: target,
          skipMarker: "// hint\n",
        },
        actionRegistry,
      ),
    ).rejects.toThrow(/non-empty reason/);

    // The Action runs AFTER the Stage; if the Stage throws, the file
    // is not touched.
    expect(existsSync(target)).toBe(false);
  });

  it("declares the expected capabilities", () => {
    expect(RETIRE_STAGE_CAPABILITIES).toEqual(["mint-deprecation"]);
    expect(RETIRE_ACTION_CAPABILITIES).toEqual(["write-invariant-file"]);
  });
});
