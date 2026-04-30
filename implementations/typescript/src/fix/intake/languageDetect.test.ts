/**
 * Tests for language detection (task #134).
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  detectLanguages,
  resolvePartitionDirs,
  PRINCIPLE_PARTITIONS,
} from "./languageDetect.js";

describe("detectLanguages", () => {
  let scratch: string;

  beforeEach(() => {
    scratch = mkdtempSync(join(tmpdir(), "provekit-langdetect-"));
  });

  afterEach(() => {
    try { rmSync(scratch, { recursive: true, force: true }); } catch {}
  });

  it("returns typescript for a project with package.json + tsconfig.json", () => {
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "tsconfig.json"), "{}");
    expect(detectLanguages(scratch)).toEqual(["typescript"]);
  });

  it("returns javascript for a project with package.json + no tsconfig + no .ts files", () => {
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "index.js"), "");
    expect(detectLanguages(scratch)).toEqual(["javascript"]);
  });

  it("promotes JS package with .ts files to typescript", () => {
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "src.ts"), "");
    expect(detectLanguages(scratch)).toEqual(["typescript"]);
  });

  it("returns rust for Cargo.toml", () => {
    writeFileSync(join(scratch, "Cargo.toml"), "");
    expect(detectLanguages(scratch)).toEqual(["rust"]);
  });

  it("returns go for go.mod", () => {
    writeFileSync(join(scratch, "go.mod"), "");
    expect(detectLanguages(scratch)).toEqual(["go"]);
  });

  it("returns python for pyproject.toml", () => {
    writeFileSync(join(scratch, "pyproject.toml"), "");
    expect(detectLanguages(scratch)).toEqual(["python"]);
  });

  it("returns python for requirements.txt", () => {
    writeFileSync(join(scratch, "requirements.txt"), "");
    expect(detectLanguages(scratch)).toEqual(["python"]);
  });

  it("returns java for pom.xml", () => {
    writeFileSync(join(scratch, "pom.xml"), "");
    expect(detectLanguages(scratch)).toEqual(["java"]);
  });

  it("returns java for build.gradle", () => {
    writeFileSync(join(scratch, "build.gradle"), "");
    expect(detectLanguages(scratch)).toEqual(["java"]);
  });

  it("returns cpp when .cpp files present", () => {
    writeFileSync(join(scratch, "main.cpp"), "");
    expect(detectLanguages(scratch)).toEqual(["cpp"]);
  });

  it("returns c when only .c files present (no .cpp)", () => {
    writeFileSync(join(scratch, "main.c"), "");
    expect(detectLanguages(scratch)).toEqual(["c"]);
  });

  it("returns cpp when both .c and .cpp files present (cpp wins)", () => {
    writeFileSync(join(scratch, "main.c"), "");
    writeFileSync(join(scratch, "lib.cpp"), "");
    expect(detectLanguages(scratch)).toEqual(["cpp"]);
  });

  it("returns cpp for CMakeLists.txt without source files", () => {
    writeFileSync(join(scratch, "CMakeLists.txt"), "");
    expect(detectLanguages(scratch)).toEqual(["cpp"]);
  });

  it("returns multiple languages for a polyglot repo", () => {
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "tsconfig.json"), "{}");
    writeFileSync(join(scratch, "Cargo.toml"), "");
    expect(detectLanguages(scratch)).toEqual(["typescript", "rust"]);
  });

  it("returns empty array for an empty project root", () => {
    expect(detectLanguages(scratch)).toEqual([]);
  });

  it("skips heavy directories during the file-extension scan", () => {
    // .cpp file under node_modules should NOT promote cpp.
    mkdirSync(join(scratch, "node_modules"), { recursive: true });
    writeFileSync(join(scratch, "node_modules", "vendored.cpp"), "");
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "tsconfig.json"), "{}");
    expect(detectLanguages(scratch)).toEqual(["typescript"]);
  });
});

describe("resolvePartitionDirs", () => {
  let scratch: string;

  beforeEach(() => {
    scratch = mkdtempSync(join(tmpdir(), "provekit-partition-"));
  });

  afterEach(() => {
    try { rmSync(scratch, { recursive: true, force: true }); } catch {}
  });

  it("always includes universal/ when it exists", () => {
    const principles = join(scratch, ".provekit", "principles");
    mkdirSync(join(principles, "universal"), { recursive: true });
    const dirs = resolvePartitionDirs(principles, scratch);
    expect(dirs).toEqual([join(principles, "universal")]);
  });

  it("adds typescript/ when TS detected and dir exists", () => {
    const principles = join(scratch, ".provekit", "principles");
    mkdirSync(join(principles, "universal"), { recursive: true });
    mkdirSync(join(principles, "typescript"), { recursive: true });
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "tsconfig.json"), "{}");
    const dirs = resolvePartitionDirs(principles, scratch);
    expect(dirs).toEqual([
      join(principles, "universal"),
      join(principles, "typescript"),
    ]);
  });

  it("does NOT add a partition dir for a detected language if the dir is missing", () => {
    const principles = join(scratch, ".provekit", "principles");
    mkdirSync(join(principles, "universal"), { recursive: true });
    // No typescript/ dir
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "tsconfig.json"), "{}");
    const dirs = resolvePartitionDirs(principles, scratch);
    expect(dirs).toEqual([join(principles, "universal")]);
  });

  it("does NOT add partitions for non-detected languages", () => {
    const principles = join(scratch, ".provekit", "principles");
    for (const p of PRINCIPLE_PARTITIONS) {
      mkdirSync(join(principles, p), { recursive: true });
    }
    writeFileSync(join(scratch, "package.json"), "{}");
    writeFileSync(join(scratch, "tsconfig.json"), "{}");
    const dirs = resolvePartitionDirs(principles, scratch);
    // Should be universal + typescript ONLY — NOT cpp/rust/etc.
    expect(dirs).toEqual([
      join(principles, "universal"),
      join(principles, "typescript"),
    ]);
  });
});
