import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mkdtempSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { IgnoreFilter } from "./IgnoreFilter";

describe("IgnoreFilter", () => {
  let projectRoot: string;
  let logSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    projectRoot = mkdtempSync(join(tmpdir(), "provekit-ignore-"));
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
    rmSync(projectRoot, { recursive: true, force: true });
  });

  it("returns no patterns when .provekitignore is missing", () => {
    const filter = new IgnoreFilter(projectRoot);
    expect(filter.getPatterns()).toEqual([]);
    expect(filter.isIgnored("anywhere/foo.ts")).toBe(false);
  });

  it("loads patterns and ignores comments + blank lines", () => {
    writeFileSync(
      join(projectRoot, ".provekitignore"),
      [
        "# this is a comment",
        "",
        "node_modules",
        "dist",
        "  ",
        "# another comment",
        "*.log",
      ].join("\n"),
    );
    const filter = new IgnoreFilter(projectRoot);
    expect(filter.getPatterns()).toEqual(["node_modules", "dist", "*.log"]);
  });

  it("matches direct path equality and directory-prefix forms", () => {
    writeFileSync(join(projectRoot, ".provekitignore"), "node_modules\ndist\n");
    const filter = new IgnoreFilter(projectRoot);

    expect(filter.isIgnored("node_modules")).toBe(true);
    expect(filter.isIgnored("node_modules/foo/bar.ts")).toBe(true);
    expect(filter.isIgnored("src/node_modules/x.ts")).toBe(true);
    expect(filter.isIgnored("dist/index.js")).toBe(true);
    expect(filter.isIgnored("src/index.ts")).toBe(false);
  });

  it("supports glob patterns with * and **", () => {
    writeFileSync(
      join(projectRoot, ".provekitignore"),
      "*.log\n**/*.test.ts\nbuild/**\n",
    );
    const filter = new IgnoreFilter(projectRoot);

    expect(filter.isIgnored("server.log")).toBe(true);
    // Single-star "*.log" doesn't span "/", so a nested .log file does not match
    // *.log directly; it would need **/ prefix.
    expect(filter.isIgnored("foo/bar.test.ts")).toBe(true);
    expect(filter.isIgnored("build/anything/in/here.js")).toBe(true);
    expect(filter.isIgnored("src/foo.ts")).toBe(false);
  });

  it("strips projectRoot prefix from absolute paths before matching", () => {
    writeFileSync(join(projectRoot, ".provekitignore"), "node_modules\n");
    const filter = new IgnoreFilter(projectRoot);
    const abs = join(projectRoot, "node_modules", "x.ts");
    expect(filter.isIgnored(abs)).toBe(true);
  });
});
