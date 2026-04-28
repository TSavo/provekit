/**
 * Tests for partition-aware principle enumeration (task #134).
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  enumeratePrincipleFiles,
  resolveWritePartition,
} from "./principleEnumeration.js";

describe("enumeratePrincipleFiles", () => {
  let scratch: string;
  let principlesDir: string;

  beforeEach(() => {
    scratch = mkdtempSync(join(tmpdir(), "provekit-enum-"));
    principlesDir = join(scratch, ".provekit", "principles");
    mkdirSync(principlesDir, { recursive: true });
  });

  afterEach(() => {
    try { rmSync(scratch, { recursive: true, force: true }); } catch {}
  });

  it("returns empty when principlesDir does not exist", () => {
    const result = enumeratePrincipleFiles(join(scratch, "missing"));
    expect(result.dslPaths).toEqual([]);
    expect(result.jsonPaths).toEqual([]);
  });

  it("loads universal/ when no language detected", () => {
    mkdirSync(join(principlesDir, "universal"));
    writeFileSync(join(principlesDir, "universal", "p1.dsl"), "");
    writeFileSync(join(principlesDir, "universal", "p1.json"), "{}");

    const result = enumeratePrincipleFiles(principlesDir, { projectRoot: scratch });
    expect(result.dslPaths.map((p) => p.replace(principlesDir, ""))).toEqual([
      "/universal/p1.dsl",
    ]);
    expect(result.jsonPaths.map((p) => p.replace(principlesDir, ""))).toEqual([
      "/universal/p1.json",
    ]);
  });

  it("loads universal/ + typescript/ when TS detected", () => {
    mkdirSync(join(principlesDir, "universal"));
    mkdirSync(join(principlesDir, "typescript"));
    mkdirSync(join(principlesDir, "rust"));
    writeFileSync(join(principlesDir, "universal", "uni.dsl"), "");
    writeFileSync(join(principlesDir, "typescript", "ts.dsl"), "");
    writeFileSync(join(principlesDir, "rust", "rs.dsl"), "");
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "tsconfig.json"), "{}");

    const result = enumeratePrincipleFiles(principlesDir, { projectRoot: scratch });
    const dsl = result.dslPaths.map((p) => p.replace(principlesDir, ""));
    expect(dsl).toContain("/universal/uni.dsl");
    expect(dsl).toContain("/typescript/ts.dsl");
    expect(dsl).not.toContain("/rust/rs.dsl");
  });

  it("loadAllPartitions=true walks every partition regardless of detection", () => {
    mkdirSync(join(principlesDir, "universal"));
    mkdirSync(join(principlesDir, "typescript"));
    mkdirSync(join(principlesDir, "rust"));
    writeFileSync(join(principlesDir, "universal", "uni.dsl"), "");
    writeFileSync(join(principlesDir, "typescript", "ts.dsl"), "");
    writeFileSync(join(principlesDir, "rust", "rs.dsl"), "");

    const result = enumeratePrincipleFiles(principlesDir, {
      loadAllPartitions: true,
    });
    const dsl = result.dslPaths.map((p) => p.replace(principlesDir, ""));
    expect(dsl).toContain("/universal/uni.dsl");
    expect(dsl).toContain("/typescript/ts.dsl");
    expect(dsl).toContain("/rust/rs.dsl");
  });

  it("never loads disabled/ subdir", () => {
    mkdirSync(join(principlesDir, "universal"));
    mkdirSync(join(principlesDir, "disabled"));
    writeFileSync(join(principlesDir, "universal", "u.dsl"), "");
    writeFileSync(join(principlesDir, "disabled", "old.dsl"), "");

    const result = enumeratePrincipleFiles(principlesDir, {
      loadAllPartitions: true,
    });
    const dsl = result.dslPaths.map((p) => p.replace(principlesDir, ""));
    expect(dsl).toContain("/universal/u.dsl");
    expect(dsl).not.toContain("/disabled/old.dsl");
  });

  it("never loads runtime retired/ subdir", () => {
    mkdirSync(join(principlesDir, "universal"));
    mkdirSync(join(principlesDir, "retired"));
    writeFileSync(join(principlesDir, "universal", "u.dsl"), "");
    writeFileSync(join(principlesDir, "retired", "dead.dsl"), "");

    const result = enumeratePrincipleFiles(principlesDir, {
      loadAllPartitions: true,
    });
    const dsl = result.dslPaths.map((p) => p.replace(principlesDir, ""));
    expect(dsl).not.toContain("/retired/dead.dsl");
  });

  it("includes flat-root files for backward compatibility", () => {
    mkdirSync(join(principlesDir, "universal"));
    writeFileSync(join(principlesDir, "universal", "u.dsl"), "");
    writeFileSync(join(principlesDir, "legacy.dsl"), ""); // flat root

    const result = enumeratePrincipleFiles(principlesDir, { projectRoot: scratch });
    const dsl = result.dslPaths.map((p) => p.replace(principlesDir, ""));
    expect(dsl).toContain("/universal/u.dsl");
    expect(dsl).toContain("/legacy.dsl");
  });

  it("includeFlatRoot=false omits flat-root files", () => {
    mkdirSync(join(principlesDir, "universal"));
    writeFileSync(join(principlesDir, "universal", "u.dsl"), "");
    writeFileSync(join(principlesDir, "legacy.dsl"), "");

    const result = enumeratePrincipleFiles(principlesDir, {
      projectRoot: scratch,
      includeFlatRoot: false,
    });
    const dsl = result.dslPaths.map((p) => p.replace(principlesDir, ""));
    expect(dsl).toContain("/universal/u.dsl");
    expect(dsl).not.toContain("/legacy.dsl");
  });

  it("returns sorted paths for deterministic load order", () => {
    mkdirSync(join(principlesDir, "universal"));
    writeFileSync(join(principlesDir, "universal", "z.dsl"), "");
    writeFileSync(join(principlesDir, "universal", "a.dsl"), "");
    writeFileSync(join(principlesDir, "universal", "m.dsl"), "");

    const result = enumeratePrincipleFiles(principlesDir, { projectRoot: scratch });
    const names = result.dslPaths.map((p) => p.split("/").pop());
    expect(names).toEqual(["a.dsl", "m.dsl", "z.dsl"]);
  });
});

describe("resolveWritePartition", () => {
  it("defaults to universal/ without language", () => {
    expect(resolveWritePartition("/x/principles")).toBe("/x/principles/universal");
  });

  it("routes to typescript/ when language=typescript", () => {
    expect(resolveWritePartition("/x/principles", "typescript")).toBe(
      "/x/principles/typescript",
    );
  });

  it("falls back to universal/ for unknown language", () => {
    expect(resolveWritePartition("/x/principles", "klingon" as any)).toBe(
      "/x/principles/universal",
    );
  });
});
