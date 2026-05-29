import { describe, expect, test } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const repoRoot = join(__dirname, "../../..");
const legacyRoot = ["e", "x", "a", "m"].join("");
const legacyPascal = legacyRoot[0].toUpperCase() + legacyRoot.slice(1);
const forbidden = [
  new RegExp(`${legacyRoot}[-_ ]manifest`, "i"),
  new RegExp(`${legacyRoot}[-_ ]question`, "i"),
  new RegExp(`${legacyRoot}[-_ ]coverage`, "i"),
  new RegExp(`${legacyPascal}(Manifest|Question|Coverage)`),
  new RegExp(`${legacyRoot.toUpperCase()}(_|\\b)`),
  new RegExp(`provekit-${legacyRoot}-manifest`, "i"),
  new RegExp(`(^|[^A-Za-z])${legacyRoot}($|[^A-Za-z])`, "i"),
];
const ignoredDirs = new Set([
  ".git",
  ".neurallog",
  ".worktrees",
  "build",
  "dist",
  "node_modules",
  "target",
]);
const ignoredExtensions = new Set([
  ".a",
  ".dylib",
  ".lock",
  ".o",
  ".png",
  ".rlib",
  ".so",
  ".wasm",
  ".zip",
]);

function walk(dir: string): string[] {
  const entries = readdirSync(dir, { withFileTypes: true });
  return entries.flatMap((entry) => {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      return ignoredDirs.has(entry.name) ? [] : walk(full);
    }
    if (!entry.isFile()) {
      return [];
    }
    if ([...ignoredExtensions].some((ext) => entry.name.endsWith(ext))) {
      return [];
    }
    return [full];
  });
}

describe("legacy ontology removal", () => {
  test("repo has no remaining legacy question-manifest identifiers", () => {
    const hits = walk(repoRoot).flatMap((file) => {
      const size = statSync(file).size;
      if (size > 2_000_000) {
        return [];
      }
      const text = readFileSync(file, "utf8");
      return forbidden.some((pattern) => pattern.test(text)) ? [relative(repoRoot, file)] : [];
    });

    expect(hits).toEqual([]);
  }, 30_000);
});
