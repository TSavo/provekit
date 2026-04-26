import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { appendHarvestProvenance } from "./provenance.js";

describe("appendHarvestProvenance", () => {
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "harvest-prov-"));
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  function writePrinciple(id: string, contents: object): void {
    writeFileSync(join(dir, `${id}.json`), JSON.stringify(contents, null, 2), "utf-8");
  }

  function readPrinciple(id: string): any {
    return JSON.parse(readFileSync(join(dir, `${id}.json`), "utf-8"));
  }

  it("appends an entry to a principle that has no provenance", () => {
    writePrinciple("foo", { id: "foo", bug_class_id: "foo" });

    const r = appendHarvestProvenance(
      [{ principleId: "foo", projectId: "express", bugId: "1", timestamp: "2026-04-26T00:00:00Z" }],
      dir,
    );
    expect(r.appended).toBe(1);
    expect(r.missingPrinciples).toEqual([]);

    const p = readPrinciple("foo");
    expect(p.provenance).toEqual([
      { source: "harvest", projectId: "express", bugId: "1", timestamp: "2026-04-26T00:00:00Z" },
    ]);
  });

  it("appends to an existing array provenance", () => {
    writePrinciple("foo", {
      id: "foo",
      bug_class_id: "foo",
      provenance: [{ source: "seed", timestamp: "2026-01-01T00:00:00Z" }],
    });

    const r = appendHarvestProvenance(
      [{ principleId: "foo", projectId: "express", bugId: "5", timestamp: "2026-04-26T00:00:00Z" }],
      dir,
    );
    expect(r.appended).toBe(1);

    const p = readPrinciple("foo");
    expect(p.provenance).toHaveLength(2);
    expect(p.provenance[1]).toEqual({
      source: "harvest", projectId: "express", bugId: "5", timestamp: "2026-04-26T00:00:00Z",
    });
  });

  it("normalizes a single-object provenance into an array on write", () => {
    writePrinciple("foo", {
      id: "foo",
      bug_class_id: "foo",
      provenance: { source: "seed", timestamp: "2026-01-01T00:00:00Z" },
    });

    appendHarvestProvenance(
      [{ principleId: "foo", projectId: "express", bugId: "5" }],
      dir,
    );

    const p = readPrinciple("foo");
    expect(Array.isArray(p.provenance)).toBe(true);
    expect(p.provenance).toHaveLength(2);
  });

  it("skips duplicate {projectId, bugId} entries (idempotent)", () => {
    writePrinciple("foo", { id: "foo", bug_class_id: "foo" });

    const entry = { principleId: "foo", projectId: "express", bugId: "1", timestamp: "2026-04-26T00:00:00Z" };
    appendHarvestProvenance([entry], dir);
    const r2 = appendHarvestProvenance([entry], dir);

    expect(r2.appended).toBe(0);
    expect(r2.duplicates).toBe(1);

    const p = readPrinciple("foo");
    expect(p.provenance).toHaveLength(1);
  });

  it("reports principles that aren't in the library directory", () => {
    writePrinciple("foo", { id: "foo", bug_class_id: "foo" });

    const r = appendHarvestProvenance(
      [
        { principleId: "foo", projectId: "express", bugId: "1" },
        { principleId: "missing", projectId: "express", bugId: "2" },
      ],
      dir,
    );

    expect(r.appended).toBe(1);
    expect(r.missingPrinciples).toEqual(["missing"]);
  });

  it("batches multiple entries for the same principle into one write", () => {
    writePrinciple("foo", { id: "foo", bug_class_id: "foo" });

    const r = appendHarvestProvenance(
      [
        { principleId: "foo", projectId: "express", bugId: "1", timestamp: "t1" },
        { principleId: "foo", projectId: "express", bugId: "2", timestamp: "t2" },
        { principleId: "foo", projectId: "eslint", bugId: "100", timestamp: "t3" },
      ],
      dir,
    );

    expect(r.appended).toBe(3);
    const p = readPrinciple("foo");
    expect(p.provenance).toHaveLength(3);
  });
});
