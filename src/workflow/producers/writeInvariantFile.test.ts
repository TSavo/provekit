/**
 * WriteInvariantFile action tests. The action is side-effecting (writes
 * to disk), so each test uses a fresh tmpdir and cleans up after itself.
 *
 * Load-bearing claims:
 * 1. overwrite mode replaces contents.
 * 2. append mode preserves existing contents.
 * 3. Each invocation writes an audit memento (no cache hit).
 * 4. Two invocations with identical input produce DIFFERENT auditCids
 *    (the runner's _auditSalt forces uniqueness).
 * 5. Non-absolute paths are rejected.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { WorkflowRunner } from "../runner.js";
import {
  makeWriteInvariantFileAction,
  WRITE_INVARIANT_FILE_CAPABILITY,
} from "./writeInvariantFile.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "write-invariant-file-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-write-invariant-file-test-v1" };

describe("writeInvariantFile Action", () => {
  it("overwrite mode replaces existing file contents", async () => {
    const db = makeDb();
    const action = makeWriteInvariantFileAction();
    const runner = new WorkflowRunner(db, wf);

    const tmp = mkdtempSync(join(tmpdir(), "wf-target-"));
    const target = join(tmp, "src", "math.invariant.ts");
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, "// existing content\n", "utf-8");

    const result = await runner.runAction(action, {
      path: target,
      surfaceText: "// new content\n",
      mode: "overwrite",
    });

    expect(readFileSync(target, "utf-8")).toBe("// new content\n");
    expect(result.resource.path).toBe(target);
    expect(result.resource.mode).toBe("overwrite");
    expect(result.resource.bytesWritten).toBe(15);
    expect(result.auditCid).toBeTruthy();
  });

  it("append mode preserves existing contents", async () => {
    const db = makeDb();
    const action = makeWriteInvariantFileAction();
    const runner = new WorkflowRunner(db, wf);

    const tmp = mkdtempSync(join(tmpdir(), "wf-target-"));
    const target = join(tmp, "math.invariant.ts");
    writeFileSync(target, "// original\n", "utf-8");

    const result = await runner.runAction(action, {
      path: target,
      surfaceText: "// appended\n",
      mode: "append",
    });

    expect(readFileSync(target, "utf-8")).toBe("// original\n// appended\n");
    expect(result.resource.mode).toBe("append");
  });

  it("append mode creates file when missing", async () => {
    const db = makeDb();
    const action = makeWriteInvariantFileAction();
    const runner = new WorkflowRunner(db, wf);

    const tmp = mkdtempSync(join(tmpdir(), "wf-target-"));
    const target = join(tmp, "fresh.invariant.ts");

    const result = await runner.runAction(action, {
      path: target,
      surfaceText: "// fresh\n",
      mode: "append",
    });

    expect(readFileSync(target, "utf-8")).toBe("// fresh\n");
    expect(result.resource.path).toBe(target);
  });

  it("two invocations with identical input produce different audit cids", async () => {
    const db = makeDb();
    const action = makeWriteInvariantFileAction();
    const runner = new WorkflowRunner(db, wf);

    const tmp = mkdtempSync(join(tmpdir(), "wf-target-"));
    const target = join(tmp, "math.invariant.ts");

    const a = await runner.runAction(action, {
      path: target,
      surfaceText: "// hello\n",
      mode: "overwrite",
    });
    const b = await runner.runAction(action, {
      path: target,
      surfaceText: "// hello\n",
      mode: "overwrite",
    });

    expect(a.auditCid).not.toBe(b.auditCid);
    expect(a.resource.contentSha256).toBe(b.resource.contentSha256);
  });

  it("rejects non-absolute paths", async () => {
    const db = makeDb();
    const action = makeWriteInvariantFileAction();
    const runner = new WorkflowRunner(db, wf);

    await expect(
      runner.runAction(action, {
        path: "relative/path.ts",
        surfaceText: "// nope\n",
        mode: "overwrite",
      }),
    ).rejects.toThrow(/must be absolute/);
  });

  it("describeResource returns a sensible string", async () => {
    const db = makeDb();
    const action = makeWriteInvariantFileAction();
    const runner = new WorkflowRunner(db, wf);

    const tmp = mkdtempSync(join(tmpdir(), "wf-target-"));
    const target = join(tmp, "x.invariant.ts");

    const result = await runner.runAction(action, {
      path: target,
      surfaceText: "x\n",
      mode: "overwrite",
    });
    const description = action.describeResource(result.resource);

    expect(description).toContain(target);
    expect(description).toMatch(/sha256:/);
    expect(description).toMatch(/2 bytes/);
  });

  it("capability constant matches the conventional name", () => {
    expect(WRITE_INVARIANT_FILE_CAPABILITY).toBe("write-invariant-file");
  });
});
