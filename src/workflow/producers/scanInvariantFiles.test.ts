/**
 * scanInvariantFiles Stage tests.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  makeScanInvariantFilesStage,
  runScanInvariantFiles,
  SCAN_INVARIANT_FILES_CAPABILITY,
} from "./scanInvariantFiles.js";

describe("scanInvariantFiles", () => {
  it("exposes the canonical capability name", () => {
    expect(SCAN_INVARIANT_FILES_CAPABILITY).toBe("scan-invariant-files");
  });

  it("returns empty list when scanRoot does not exist", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "scan-empty-"));
    const out = await runScanInvariantFiles({
      scanRoot: join(projectRoot, "does-not-exist"),
      projectRoot,
    });
    expect(out.files).toEqual([]);
  });

  it("returns empty list for a tree with no .invariant.ts files", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "scan-no-invariants-"));
    mkdirSync(join(projectRoot, "src"), { recursive: true });
    writeFileSync(join(projectRoot, "src", "a.ts"), "export {};\n");
    const out = await runScanInvariantFiles({
      scanRoot: join(projectRoot, "src"),
      projectRoot,
    });
    expect(out.files).toEqual([]);
  });

  it("discovers .invariant.ts files and computes contentHash + relative path", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "scan-found-"));
    mkdirSync(join(projectRoot, "src", "billing"), { recursive: true });
    const invPath = join(projectRoot, "src", "billing", "invoice.invariant.ts");
    writeFileSync(invPath, `// hello\n`, "utf-8");

    const out = await runScanInvariantFiles({
      scanRoot: join(projectRoot, "src"),
      projectRoot,
    });
    expect(out.files).toHaveLength(1);
    const f = out.files[0]!;
    expect(f.path).toBe("src/billing/invoice.invariant.ts");
    expect(f.resolvedModulePath).toBe(invPath);
    expect(f.contentHash).toMatch(/^[0-9a-f]{64}$/);
  });

  it("skips node_modules / dist / lib / dot-directories", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "scan-skip-"));
    for (const skipped of ["node_modules", "dist", "lib", ".git"]) {
      mkdirSync(join(projectRoot, "src", skipped), { recursive: true });
      writeFileSync(
        join(projectRoot, "src", skipped, "x.invariant.ts"),
        "// nope\n",
      );
    }
    mkdirSync(join(projectRoot, "src", "real"), { recursive: true });
    writeFileSync(
      join(projectRoot, "src", "real", "y.invariant.ts"),
      "// yes\n",
    );

    const out = await runScanInvariantFiles({
      scanRoot: join(projectRoot, "src"),
      projectRoot,
    });
    expect(out.files).toHaveLength(1);
    expect(out.files[0]!.path).toBe("src/real/y.invariant.ts");
  });

  it("returns files in sorted order by relative path", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "scan-sorted-"));
    mkdirSync(join(projectRoot, "src", "a"), { recursive: true });
    mkdirSync(join(projectRoot, "src", "b"), { recursive: true });
    writeFileSync(join(projectRoot, "src", "b", "z.invariant.ts"), "");
    writeFileSync(join(projectRoot, "src", "a", "k.invariant.ts"), "");
    const out = await runScanInvariantFiles({
      scanRoot: join(projectRoot, "src"),
      projectRoot,
    });
    expect(out.files.map((f) => f.path)).toEqual([
      "src/a/k.invariant.ts",
      "src/b/z.invariant.ts",
    ]);
  });

  it("Stage shape: serializeInput is the cache key", () => {
    const stage = makeScanInvariantFilesStage();
    expect(
      stage.serializeInput({ scanRoot: "/p/src", projectRoot: "/p" }),
    ).toEqual({ scanRoot: "/p/src", projectRoot: "/p" });
  });
});
