import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./index.js";

describe("openDb", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("opens a connection against a new sqlite file and runs a trivial query", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "provekit-test-"));
    const dbPath = join(tmpDir, "test.db");
    const db = openDb(dbPath);
    const result = db.$client.prepare("select 1 as x").get() as { x: number };
    expect(result.x).toBe(1);
    db.$client.close();
  });
});
